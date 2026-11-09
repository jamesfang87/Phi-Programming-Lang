pub(crate) mod subst;
#[cfg(test)]
mod tests;

use std::collections::HashMap;

use crate::hir::{DefId, HirId};
use crate::mir::lower::Mir;
use crate::mir::{
    AggregateKind, AnyMode, AssertMessage, Body, BodyKind, ConstKind, FunRef, Instance, LocalDecl,
    Operand, Rvalue, StatementKind, TerminatorKind,
};
use crate::typeck::ty::ctx::TyCtx;
use crate::typeck::ty::visitor::Subst;
use crate::typeck::ty::{Ty, TyKind};

const INSTANTIATION_LIMIT: usize = 4096;

/// The shared state of one monomorphization run: the type context being extended with each
/// instance's concrete types, the generic program, and the instances already emitted and newly
/// discovered.
struct InstantiationCx<'a> {
    tcx: &'a mut TyCtx,
    program: &'a Mir,
    output: &'a mut HashMap<Instance, Body>,
    discovered: &'a mut Vec<Instance>,
}

/// Returns the concrete body of every instance reachable from `main` and from the program's
/// non-generic bodies.
pub fn monomorphize(
    tcx: &mut TyCtx,
    program: &Mir,
    main: Option<DefId>,
) -> HashMap<Instance, Body> {
    let mut output = HashMap::new();
    let mut discovered = collect_root_instances(tcx, program, main);
    let mut processed = 0usize;

    while let Some(instance) = discovered.pop() {
        if output.contains_key(&instance) {
            continue;
        }
        processed += 1;
        check_instantiation_limit(processed);
        instantiate_instance(tcx, program, instance, &mut output, &mut discovered);
    }

    output
}

/// Returns the instances a run starts from: `main` and every non-generic body, but no closure,
/// which is discovered from the aggregate that builds it.
fn collect_root_instances(tcx: &TyCtx, program: &Mir, main: Option<DefId>) -> Vec<Instance> {
    let mut roots = Vec::new();
    for (&(def, any_mode), body) in &program.bodies {
        if body.kind == BodyKind::Closure {
            continue;
        }
        if Some(def) == main || !body_mentions_generic(tcx, body) {
            roots.push(Instance {
                def,
                any_mode,
                args: Vec::new(),
                self_ty: None,
            });
        }
    }
    roots
}

fn check_instantiation_limit(processed: usize) {
    if processed > INSTANTIATION_LIMIT {
        panic!(
            "mir::monomorphize: more than {INSTANTIATION_LIMIT} instances were requested; \
             this almost always means an unbounded generic instantiation chain"
        );
    }
}

fn instantiate_instance(
    tcx: &mut TyCtx,
    program: &Mir,
    instance: Instance,
    output: &mut HashMap<Instance, Body>,
    discovered: &mut Vec<Instance>,
) {
    let Some(generic_body) = program.bodies.get(&(instance.def, instance.any_mode)) else {
        return;
    };
    let substitution = build_subst(generic_body, &instance.args, instance.self_ty);
    let mut cx = InstantiationCx {
        tcx,
        program,
        output,
        discovered,
    };
    let concrete = cx.process_body(generic_body, &substitution, &instance);
    cx.output.insert(instance, concrete);
}

fn build_subst(body: &Body, args: &[Ty], self_ty: Option<Ty>) -> Subst {
    Subst {
        generics: body
            .generics
            .iter()
            .copied()
            .zip(args.iter().copied())
            .collect(),
        self_ty,
    }
}

fn body_mentions_generic(tcx: &TyCtx, body: &Body) -> bool {
    body.local_decls
        .iter()
        .any(|decl| subst::mentions_generic(tcx, decl.ty))
}

impl InstantiationCx<'_> {
    /// Returns `generic_body` with every generic type and `Self` replaced by `instance`'s
    /// arguments and self type.
    fn process_body(
        &mut self,
        generic_body: &Body,
        substitution: &Subst,
        instance: &Instance,
    ) -> Body {
        let body = Body {
            def_id: generic_body.def_id,
            kind: generic_body.kind,
            basic_blocks: generic_body
                .basic_blocks
                .iter()
                .map(|block| self.subst_block(block, substitution, instance))
                .collect(),
            local_decls: self.subst_local_decls(&generic_body.local_decls, substitution),
            param_count: generic_body.param_count,
            generics: generic_body.generics.clone(),
            span: generic_body.span,
        };
        self.queue_unsize_vtables(&body);
        body
    }

    /// Queues the implementation methods whose vtables a body's unsizing coercions reference.
    fn queue_unsize_vtables(&mut self, body: &Body) {
        for block in &body.basic_blocks {
            for statement in &block.statements {
                let StatementKind::Assign(destination, Rvalue::Unsize { operand, trait_ }) =
                    &statement.kind
                else {
                    continue;
                };
                let pointed = operand_ty(self.tcx, &body.local_decls, operand);
                let concrete = peel_references(self.tcx, pointed);
                let destination_ty = crate::mir::place_ty(self.tcx, &body.local_decls, destination);
                let trait_args =
                    crate::mir::vtables::trait_args_of_unsized_ty(self.tcx, destination_ty);
                let Some((info, bindings)) = crate::mir::vtables::find_vtable(
                    &self.program.vtables,
                    self.tcx,
                    concrete,
                    *trait_,
                    &trait_args,
                ) else {
                    continue;
                };
                let Some(args) = info
                    .extend_generics
                    .iter()
                    .map(|&generic| bindings.get(&generic).copied())
                    .collect::<Option<Vec<Ty>>>()
                else {
                    continue;
                };
                for &method in info.methods.iter().flatten() {
                    self.queue_fn_def(method, None, args.clone(), None);
                }
            }
        }
    }

    fn subst_local_decls(
        &mut self,
        decls: &[crate::mir::LocalDecl],
        substitution: &Subst,
    ) -> Vec<crate::mir::LocalDecl> {
        decls
            .iter()
            .map(|decl| crate::mir::LocalDecl {
                ty: subst::subst_ty(self.tcx, decl.ty, substitution),
                name: decl.name,
                span: decl.span,
            })
            .collect()
    }

    fn subst_block(
        &mut self,
        block: &crate::mir::BasicBlockData,
        substitution: &Subst,
        instance: &Instance,
    ) -> crate::mir::BasicBlockData {
        let statements = block
            .statements
            .iter()
            .map(|stmt| crate::mir::Statement {
                id: stmt.id,
                kind: self.subst_stmt(stmt.kind.clone(), substitution, instance),
                span: stmt.span,
            })
            .collect();
        let terminator = crate::mir::Terminator {
            kind: self.subst_terminator(block.terminator.kind.clone(), substitution),
            span: block.terminator.span,
        };
        crate::mir::BasicBlockData {
            statements,
            terminator,
        }
    }

    fn subst_stmt(
        &mut self,
        kind: StatementKind,
        substitution: &Subst,
        instance: &Instance,
    ) -> StatementKind {
        match kind {
            StatementKind::Assign(place, rvalue) => {
                StatementKind::Assign(place, self.subst_rvalue(rvalue, substitution, instance))
            }
            other => other,
        }
    }

    fn subst_rvalue(
        &mut self,
        rvalue: Rvalue,
        substitution: &Subst,
        instance: &Instance,
    ) -> Rvalue {
        match rvalue {
            Rvalue::Use(operand) => Rvalue::Use(self.subst_operand(operand, substitution)),
            Rvalue::Ref { mutability, place } => Rvalue::Ref { mutability, place },
            Rvalue::BinaryOp(op, lhs, rhs) => Rvalue::BinaryOp(
                op,
                self.subst_operand(lhs, substitution),
                self.subst_operand(rhs, substitution),
            ),
            Rvalue::CheckedBinaryOp(op, lhs, rhs) => Rvalue::CheckedBinaryOp(
                op,
                self.subst_operand(lhs, substitution),
                self.subst_operand(rhs, substitution),
            ),
            Rvalue::UnaryOp(op, operand) => {
                Rvalue::UnaryOp(op, self.subst_operand(operand, substitution))
            }
            Rvalue::Cast { operand, ty, kind } => Rvalue::Cast {
                operand: self.subst_operand(operand, substitution),
                ty: subst::subst_ty(self.tcx, ty, substitution),
                kind,
            },
            Rvalue::Aggregate(kind, operands) => {
                let kind = self.instantiate_aggregate_kind(*kind, substitution, instance);
                let operands = operands
                    .into_iter()
                    .map(|op| self.subst_operand(op, substitution))
                    .collect();
                Rvalue::Aggregate(kind, operands)
            }
            Rvalue::Unsize { operand, trait_ } => Rvalue::Unsize {
                operand: self.subst_operand(operand, substitution),
                trait_,
            },
            Rvalue::Discriminant(place) => Rvalue::Discriminant(place),
            Rvalue::Len(place) => Rvalue::Len(place),
            Rvalue::New(operand) => Rvalue::New(self.subst_operand(operand, substitution)),
            Rvalue::NewArray { elem, count } => Rvalue::NewArray {
                elem: self.subst_operand(elem, substitution),
                count: self.subst_operand(count, substitution),
            },
        }
    }

    /// Returns the aggregate kind with every closure it names instantiated, emitting each
    /// closure's own body along the way.
    fn instantiate_aggregate_kind(
        &mut self,
        mut kind: AggregateKind,
        substitution: &Subst,
        instance: &Instance,
    ) -> Box<AggregateKind> {
        if let AggregateKind::Closure { def, args, self_ty } = &mut kind {
            *args = instance.args.clone();
            *self_ty = instance.self_ty;
            let closure_instance = Instance {
                def: *def,
                any_mode: None,
                args: instance.args.clone(),
                self_ty: instance.self_ty,
            };
            self.instantiate_closure(*def, closure_instance, substitution);
        }
        Box::new(kind)
    }

    fn instantiate_closure(
        &mut self,
        def: DefId,
        closure_instance: Instance,
        substitution: &Subst,
    ) {
        if self.output.contains_key(&closure_instance) {
            return;
        }
        let program = self.program;
        let Some(closure_generic_body) = program.bodies.get(&(def, None)) else {
            return;
        };
        let closure_body = self.process_body(closure_generic_body, substitution, &closure_instance);
        self.output.insert(closure_instance, closure_body);
    }

    fn subst_operand(&mut self, operand: Operand, substitution: &Subst) -> Operand {
        let Operand::Constant(constant) = operand else {
            return operand;
        };
        let ty = subst::subst_ty(self.tcx, constant.ty, substitution);
        let kind = self.subst_const_kind(constant.kind, substitution);
        Operand::Constant(crate::mir::Constant { ty, kind })
    }

    fn subst_const_kind(&mut self, kind: ConstKind, substitution: &Subst) -> ConstKind {
        match kind {
            ConstKind::FunDef(fun) => self.subst_fun_def(fun, substitution),
            other => other,
        }
    }

    fn subst_fun_def(&mut self, fun: FunRef, substitution: &Subst) -> ConstKind {
        let FunRef {
            def,
            args,
            any_mode,
            self_ty,
            trait_args,
            trait_method,
        } = fun;
        let args = self.subst_tys(&args, substitution);
        let trait_args = self.subst_tys(&trait_args, substitution);
        let self_ty = self.subst_optional_ty(self_ty, substitution);

        if self.is_dyn_dispatch(self_ty) {
            return ConstKind::FunDef(FunRef {
                def,
                args,
                any_mode,
                self_ty,
                trait_args,
                trait_method,
            });
        }
        self.instantiate_concrete_fun(def, args, any_mode, self_ty, trait_args, trait_method)
    }

    /// Redirects a trait-default method to the implementing method its vtable selects, prepends
    /// the trait's own arguments, and queues the resulting function for instantiation.
    fn instantiate_concrete_fun(
        &mut self,
        def: DefId,
        args: Vec<Ty>,
        any_mode: Option<AnyMode>,
        self_ty: Option<Ty>,
        trait_args: Vec<Ty>,
        trait_method: Option<(DefId, u32)>,
    ) -> ConstKind {
        let program = self.program;
        let (def, mut args, self_ty, trait_method) = match redirect_trait_default(
            program,
            self.tcx,
            trait_method,
            self_ty,
            &args,
            &trait_args,
        ) {
            Some((impl_method, impl_args)) => (impl_method, impl_args, None, None),
            None => (def, args, self_ty, trait_method),
        };
        let self_ty = if trait_method.is_some() {
            self_ty
        } else {
            None
        };
        if trait_method.is_some() {
            args = trait_args.into_iter().chain(args).collect();
        }
        self.queue_fn_def(def, any_mode, args.clone(), self_ty);
        ConstKind::FunDef(FunRef {
            def,
            args,
            any_mode,
            self_ty,
            trait_args: Vec::new(),
            trait_method,
        })
    }

    fn subst_tys(&mut self, tys: &[Ty], substitution: &Subst) -> Vec<Ty> {
        tys.iter()
            .map(|&ty| subst::subst_ty(self.tcx, ty, substitution))
            .collect()
    }

    fn subst_optional_ty(&mut self, ty: Option<Ty>, substitution: &Subst) -> Option<Ty> {
        ty.map(|ty| subst::subst_ty(self.tcx, ty, substitution))
    }

    fn is_dyn_dispatch(&self, self_ty: Option<Ty>) -> bool {
        self_ty.is_some_and(|ty| matches!(self.tcx.kind(ty), TyKind::Dyn { .. }))
    }

    fn queue_fn_def(
        &mut self,
        def: DefId,
        mode: Option<AnyMode>,
        args: Vec<Ty>,
        self_ty: Option<Ty>,
    ) {
        let instance = Instance {
            def,
            any_mode: mode,
            args,
            self_ty,
        };
        if !self.output.contains_key(&instance) {
            self.discovered.push(instance);
        }
    }

    fn subst_terminator(&mut self, kind: TerminatorKind, substitution: &Subst) -> TerminatorKind {
        match kind {
            TerminatorKind::Call {
                func,
                args,
                destination,
                target,
            } => TerminatorKind::Call {
                func: self.subst_operand(func, substitution),
                args: args
                    .into_iter()
                    .map(|arg| self.subst_operand(arg, substitution))
                    .collect(),
                destination,
                target,
            },
            TerminatorKind::Assert {
                cond,
                expected,
                msg,
                target,
            } => TerminatorKind::Assert {
                cond: self.subst_operand(cond, substitution),
                expected,
                msg: self.subst_assert_message(msg, substitution),
                target,
            },
            other => other,
        }
    }

    fn subst_assert_message(&mut self, msg: AssertMessage, substitution: &Subst) -> AssertMessage {
        match msg {
            AssertMessage::Assert(m) => {
                AssertMessage::Assert(m.map(|op| self.subst_operand(op, substitution)))
            }
            AssertMessage::Panic(m) => {
                AssertMessage::Panic(m.map(|op| self.subst_operand(op, substitution)))
            }
            AssertMessage::Unreachable(m) => {
                AssertMessage::Unreachable(m.map(|op| self.subst_operand(op, substitution)))
            }
            other => other,
        }
    }
}

/// Returns the implementing method and its arguments when `trait_method` is a trait default and
/// `self_ty` names a type with a matching vtable.
fn redirect_trait_default(
    program: &Mir,
    tcx: &mut TyCtx,
    trait_method: Option<(DefId, u32)>,
    self_ty: Option<Ty>,
    args: &[Ty],
    trait_args: &[Ty],
) -> Option<(DefId, Vec<Ty>)> {
    let self_ty = self_ty?;
    let (trait_owner, index) = trait_method?;
    let (info, binds) = find_matching_vtable(program, tcx, trait_owner, self_ty, trait_args)?;
    let impl_method = info.methods[index as usize]?;
    let mut redirected: Vec<Ty> = info
        .extend_generics
        .iter()
        .map(|generic| binds[generic])
        .collect();
    redirected.extend(args.iter().copied());
    Some((impl_method, redirected))
}

/// Returns the vtable that proves `self_ty` implements `trait_owner` with `trait_args`, with the
/// bindings matching its self type produced along the way.
fn find_matching_vtable<'a>(
    program: &'a Mir,
    tcx: &mut TyCtx,
    trait_owner: DefId,
    self_ty: Ty,
    trait_args: &[Ty],
) -> Option<(&'a crate::mir::vtables::VtableInfo, HashMap<HirId, Ty>)> {
    crate::mir::vtables::find_vtable(&program.vtables, tcx, self_ty, trait_owner, trait_args)
}

/// Returns the type of `operand` using `local_decls` for the places it names.
fn operand_ty(tcx: &mut TyCtx, local_decls: &[LocalDecl], operand: &Operand) -> Ty {
    match operand {
        Operand::Constant(constant) => constant.ty,
        Operand::Copy(place) | Operand::Move(place) => {
            crate::mir::place_ty(tcx, local_decls, place)
        }
    }
}

/// Returns the type `ty` refers to when it is a reference, or `ty` itself otherwise.
fn peel_references(tcx: &TyCtx, mut ty: Ty) -> Ty {
    while let TyKind::Ref { base, .. } = *tcx.kind(ty) {
        ty = base;
    }
    ty
}
