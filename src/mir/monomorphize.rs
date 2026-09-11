//! Monomorphization: the MIR-to-MIR pass that substitutes a generic [`Body`]'s remaining
//! `TyKind::Generic`/`SelfTy` occurrences into a concrete one, per instantiation actually used.
//!
//! A worklist pass, phased exactly as the plan calls for:
//!
//! 1. **Seed**: every `(DefId, Option<AnyMode>)` [`Body`] `mir::lower` actually built that
//!    mentions no `TyKind::Generic`/`SelfTy` anywhere in it is trivially its own root instance,
//!    queued with an empty argument list.
//! 2. **Process**: pop an [`Instance`]; if it was already emitted, skip (this is what keeps a
//!    recursive-but-not-unbounded generic, such as `fun f<T>() { f::<T>(); }`, from
//!    re-processing forever). Otherwise substitute every `Ty` the matching generic `Body`
//!    contains via [`subst::subst_ty`], discovering further instances along the way: a
//!    `ConstKind::FunDef(def, args, mode, self_ty)` names one, queued through the worklist
//!    since a function's own declared generics can be zipped against a substituted `args`
//!    list (and a `self_ty` is carried for a trait's own method, whose body is substituted
//!    once per implementing type); a closure nested inside is handled eagerly instead,
//!    recursing immediately with the *same* substitution map, since a closure declares no
//!    generics of its own to zip against at all -- every `TyKind::Generic` its body mentions
//!    names a parameter of the *enclosing* definition.
//! 3. **Terminate**: the queue empties, or a depth guard reports a clear internal error instead
//!    of hanging on a pathological, ever-growing instantiation chain.
//!
//! A call inside a trait's own default body names the trait's declaration of the method, which
//! may be abstract or overridden. At substitution time the instance's concrete `self_ty` picks
//! the implementing type's own method out of `Mir::vtables`, so the default body dispatches
//! statically once per implementing type; a callee the implementing type does not provide keeps
//! naming the trait's own (default) body.

pub(crate) mod subst;
#[cfg(test)]
mod tests;

use std::collections::HashMap;

use crate::hir::{DefId, HirId};
use crate::mir::lower::Mir;
use crate::mir::{
    AggregateKind, AnyMode, AssertMessage, Body, ConstKind, DefKind, Instance, Operand, Rvalue,
    StatementKind, TerminatorKind,
};
use crate::typeck::fold::Subst;
use crate::typeck::ty::{Ty, TyKind};
use crate::typeck::tyctx::TyCtx;

/// A generous ceiling on the number of instances one `monomorphize` call will produce, past
/// which further instantiation is treated as a pathological, unbounded chain rather than a
/// legitimate program -- the same pragmatic limit real compilers reach for (`rustc`'s own
/// generic-depth overflow error) rather than solving unbounded monomorphization outright, which
/// is out of scope here.
const INSTANTIATION_LIMIT: usize = 4096;

/// Runs monomorphization over every `Body` `mir::lower` produced, returning one concrete `Body`
/// per instance actually used, keyed by the same [`Instance`] the spec's "Generic
/// monomorphization" section describes.
pub fn monomorphize(tcx: &mut TyCtx, program: &Mir) -> HashMap<Instance, Body> {
    let mut output = HashMap::new();
    let mut worklist: Vec<Instance> = Vec::new();

    // Seed the worklist with the bodies that need no substitution. This only decides *roots*:
    // a body something else calls is reached through `discovered` regardless, so the bodies for
    // which this test is load-bearing are the ones nothing calls -- in practice `main`.
    //
    // `main` is therefore seeded on its own terms, not on the test's. It is checked to declare
    // no generics (`typeck::entry_point`) and nothing calls it, so it is a root by
    // definition, with an empty argument list. Leaving it to `body_mentions_generic` made it
    // hostage to every type in its body being concrete, and a lowering bug that put a stray
    // generic in one of its locals dropped the program's entry point silently.
    for (&(def, any_mode), body) in &program.bodies {
        if program.def_infos.kind(def) == DefKind::Closure {
            continue;
        }
        if Some(def) == program.main || !body_mentions_generic(tcx, body) {
            worklist.push(Instance {
                def,
                any_mode,
                args: Vec::new(),
                self_ty: None,
            });
        }
    }

    let mut processed = 0usize;
    while let Some(instance) = worklist.pop() {
        if output.contains_key(&instance) {
            continue;
        }
        processed += 1;
        if processed > INSTANTIATION_LIMIT {
            panic!(
                "mir::monomorphize: more than {INSTANTIATION_LIMIT} instances were requested; \
                 this almost always means an unbounded generic instantiation chain"
            );
        }

        let Some(generic_body) = program.bodies.get(&(instance.def, instance.any_mode)) else {
            continue;
        };
        let subst = build_subst(program, instance.def, &instance.args, instance.self_ty);
        let mut discovered = Vec::new();
        let concrete = process_body(
            tcx,
            program,
            generic_body,
            &subst,
            &instance,
            &mut output,
            &mut discovered,
        );
        output.insert(instance.clone(), concrete);
        worklist.extend(discovered);
    }

    output
}

fn build_subst(program: &Mir, def: DefId, args: &[Ty], self_ty: Option<Ty>) -> Subst {
    let infos = &program.def_infos;
    let mut params: Vec<HirId> = Vec::new();
    if let Some(parent) = infos.parent(def) {
        match infos.kind(parent) {
            DefKind::Extend | DefKind::Trait => params.extend(infos.generics(parent)),
            _ => {}
        }
    }
    params.extend(infos.generics(def));
    Subst {
        generics: params.into_iter().zip(args.iter().copied()).collect(),
        self_ty,
    }
}

/// Whether any of `body`'s locals is typed with a generic still in it.
///
/// Used as the stand-in for "this body needs substituting before it can be emitted", which holds
/// only while every local of a non-generic body is typed concretely. A lowering that puts a
/// declared, un-substituted type into a caller's locals breaks that equivalence and holds the
/// caller back from the roots -- see `lower_receiver_operand`, which types the `&self` temp from
/// the receiver rather than from the method's declared `&self` for exactly this reason.
///
/// When it does misfire, the body is still reached if anything calls it, and `main` is seeded
/// regardless; what is left over is a body nothing calls, which is dead code either way.
fn body_mentions_generic(tcx: &TyCtx, body: &Body) -> bool {
    body.local_decls
        .iter()
        .any(|decl| subst::mentions_generic(tcx, decl.ty))
}

/// Substitutes every `Ty` `generic_body` contains, discovering further instances along the way.
/// A nested closure is substituted eagerly, right here, and inserted into `output` directly,
/// since it shares `instance`'s own substitution rather than needing a worklist entry of its
/// own; a nested `FunDef` call is pushed onto `discovered` instead, since it has its own declared
/// generics an argument list can be zipped against independently.
#[allow(clippy::too_many_arguments)]
fn process_body(
    tcx: &mut TyCtx,
    program: &Mir,
    generic_body: &Body,
    subst: &Subst,
    instance: &Instance,
    output: &mut HashMap<Instance, Body>,
    discovered: &mut Vec<Instance>,
) -> Body {
    let mut body = Body {
        def_id: generic_body.def_id,
        basic_blocks: Vec::with_capacity(generic_body.basic_blocks.len()),
        local_decls: Vec::with_capacity(generic_body.local_decls.len()),
        param_count: generic_body.param_count,
        span: generic_body.span,
    };

    for decl in &generic_body.local_decls {
        body.local_decls.push(crate::mir::LocalDecl {
            ty: subst::subst_ty(tcx, decl.ty, subst),
            name: decl.name,
            span: decl.span,
        });
    }

    for block in &generic_body.basic_blocks {
        let statements = block
            .statements
            .iter()
            .map(|stmt| crate::mir::Statement {
                id: stmt.id,
                kind: subst_stmt(
                    tcx,
                    program,
                    stmt.kind.clone(),
                    subst,
                    instance,
                    output,
                    discovered,
                ),
                span: stmt.span,
            })
            .collect();
        let terminator = crate::mir::Terminator {
            kind: subst_terminator(
                tcx,
                program,
                block.terminator.kind.clone(),
                subst,
                output,
                discovered,
            ),
            span: block.terminator.span,
        };
        body.basic_blocks.push(crate::mir::BasicBlockData {
            statements,
            terminator,
        });
    }

    body
}

#[allow(clippy::too_many_arguments)]
fn subst_stmt(
    tcx: &mut TyCtx,
    program: &Mir,
    kind: StatementKind,
    subst: &Subst,
    instance: &Instance,
    output: &mut HashMap<Instance, Body>,
    discovered: &mut Vec<Instance>,
) -> StatementKind {
    match kind {
        StatementKind::Assign(place, rvalue) => StatementKind::Assign(
            place,
            subst_rvalue(tcx, program, rvalue, subst, instance, output, discovered),
        ),
        other => other,
    }
}

#[allow(clippy::too_many_arguments)]
fn subst_rvalue(
    tcx: &mut TyCtx,
    program: &Mir,
    rvalue: Rvalue,
    subst: &Subst,
    instance: &Instance,
    output: &mut HashMap<Instance, Body>,
    discovered: &mut Vec<Instance>,
) -> Rvalue {
    match rvalue {
        Rvalue::Use(operand) => Rvalue::Use(subst_operand(
            tcx, program, operand, subst, output, discovered,
        )),
        Rvalue::Ref { mutability, place } => Rvalue::Ref { mutability, place },
        Rvalue::BinaryOp(op, lhs, rhs) => Rvalue::BinaryOp(
            op,
            subst_operand(tcx, program, lhs, subst, output, discovered),
            subst_operand(tcx, program, rhs, subst, output, discovered),
        ),
        Rvalue::CheckedBinaryOp(op, lhs, rhs) => Rvalue::CheckedBinaryOp(
            op,
            subst_operand(tcx, program, lhs, subst, output, discovered),
            subst_operand(tcx, program, rhs, subst, output, discovered),
        ),
        Rvalue::UnaryOp(op, operand) => Rvalue::UnaryOp(
            op,
            subst_operand(tcx, program, operand, subst, output, discovered),
        ),
        Rvalue::Cast { operand, ty, kind } => Rvalue::Cast {
            operand: subst_operand(tcx, program, operand, subst, output, discovered),
            ty: subst::subst_ty(tcx, ty, subst),
            kind,
        },
        Rvalue::Aggregate(mut kind, operands) => {
            if let AggregateKind::Closure { def, args, self_ty } = &mut *kind {
                *args = instance.args.clone();
                *self_ty = instance.self_ty.clone();
                let def = *def;
                let closure_instance = Instance {
                    def,
                    any_mode: None,
                    args: instance.args.clone(),
                    self_ty: instance.self_ty.clone(),
                };
                if !output.contains_key(&closure_instance)
                    && let Some(closure_generic_body) = program.bodies.get(&(def, None))
                {
                    let closure_body = process_body(
                        tcx,
                        program,
                        closure_generic_body,
                        subst,
                        &closure_instance,
                        output,
                        discovered,
                    );
                    output.insert(closure_instance.clone(), closure_body);
                }
            }
            let operands = operands
                .into_iter()
                .map(|op| subst_operand(tcx, program, op, subst, output, discovered))
                .collect();
            Rvalue::Aggregate(kind, operands)
        }
        Rvalue::Unsize { operand, trait_ } => Rvalue::Unsize {
            operand: subst_operand(tcx, program, operand, subst, output, discovered),
            trait_,
        },
        Rvalue::Discriminant(place) => Rvalue::Discriminant(place),
        Rvalue::Len(place) => Rvalue::Len(place),
        Rvalue::New(operand) => Rvalue::New(subst_operand(
            tcx, program, operand, subst, output, discovered,
        )),
        Rvalue::NewArray { elem, count } => Rvalue::NewArray {
            elem: subst_operand(tcx, program, elem, subst, output, discovered),
            count: subst_operand(tcx, program, count, subst, output, discovered),
        },
    }
}

fn subst_operand(
    tcx: &mut TyCtx,
    program: &Mir,
    operand: Operand,
    subst: &Subst,
    output: &mut HashMap<Instance, Body>,
    discovered: &mut Vec<Instance>,
) -> Operand {
    let Operand::Constant(constant) = operand else {
        return operand;
    };
    let ty = subst::subst_ty(tcx, constant.ty, subst);
    let kind = match constant.kind {
        ConstKind::FunDef(def, args, mode, self_ty) => {
            let args: Vec<Ty> = args
                .iter()
                .map(|&a| subst::subst_ty(tcx, a, subst))
                .collect();
            let self_ty = self_ty.map(|ty| subst::subst_ty(tcx, ty, subst));
            let dyn_dispatch = self_ty.is_some_and(|ty| matches!(tcx.kind(ty), TyKind::Dyn { .. }));
            let (def, args, self_ty) = if dyn_dispatch {
                (def, args, self_ty)
            } else {
                match redirect_trait_default(program, tcx, def, self_ty, &args) {
                    Some((impl_method, impl_args)) => (impl_method, impl_args, None),
                    None => (def, args, self_ty),
                }
            };
            let self_ty = match program.def_infos.parent(def) {
                Some(parent) if program.def_infos.kind(parent) == DefKind::Trait => self_ty,
                _ => None,
            };
            let args = match trait_args_for(program, tcx, def, self_ty) {
                Some(trait_args) => trait_args.into_iter().chain(args.into_iter()).collect(),
                None => args,
            };
            if !dyn_dispatch {
                queue_fn_def(def, mode, args.clone(), self_ty.clone(), output, discovered);
            }
            ConstKind::FunDef(def, args, mode, self_ty)
        }
        other => other,
    };
    Operand::Constant(crate::mir::Constant { ty, kind })
}

/// The implementing type's own method for a call made inside a trait's default body, with the
/// argument list the impl's body needs: the block's own generic arguments first (picked out by
/// matching the impl's written self type against the concrete `self_ty`), then the method's own.
fn redirect_trait_default(
    program: &Mir,
    tcx: &mut TyCtx,
    def: DefId,
    self_ty: Option<Ty>,
    args: &[Ty],
) -> Option<(DefId, Vec<Ty>)> {
    let self_ty = self_ty?;
    let (trait_owner, index) = program.def_infos.trait_method(def)?;
    let (info, binds) = matching_vtable(program, tcx, trait_owner, self_ty)?;
    let impl_method = info.methods[index as usize]?;
    let mut redirected: Vec<Ty> = info
        .extend_generics
        .iter()
        .map(|generic| binds[generic])
        .collect();
    redirected.extend(args.iter().copied());
    Some((impl_method, redirected))
}

/// The trait arguments the implementing type realizes, taken from the `extend .. with` block
/// whose written self type matches `self_ty`, with the block's own generics substituted out.
fn trait_args_for(
    program: &Mir,
    tcx: &mut TyCtx,
    def: DefId,
    self_ty: Option<Ty>,
) -> Option<Vec<Ty>> {
    let self_ty = self_ty?;
    let (trait_owner, _) = program.def_infos.trait_method(def)?;
    let (info, binds) = matching_vtable(program, tcx, trait_owner, self_ty)?;
    let subst = Subst {
        generics: binds,
        self_ty: None,
    };
    Some(
        info.trait_args
            .iter()
            .map(|&arg| subst::subst_ty(tcx, arg, &subst))
            .collect(),
    )
}

fn matching_vtable<'a>(
    program: &'a Mir,
    tcx: &mut TyCtx,
    trait_owner: DefId,
    self_ty: Ty,
) -> Option<(&'a crate::mir::vtables::VtableInfo, HashMap<HirId, Ty>)> {
    for ((pattern, trait_def), info) in &program.vtables {
        if *trait_def != trait_owner {
            continue;
        }
        let mut binds: HashMap<HirId, Ty> = HashMap::new();
        if matches_impl_self(tcx, *pattern, self_ty, &mut binds) {
            return Some((info, binds));
        }
    }
    None
}

fn matches_impl_self(
    tcx: &mut TyCtx,
    pattern: Ty,
    self_ty: Ty,
    binds: &mut HashMap<HirId, Ty>,
) -> bool {
    match (tcx.kind(pattern).clone(), tcx.kind(self_ty).clone()) {
        (TyKind::Generic(param), _) => match binds.get(&param) {
            Some(bound) => *bound == self_ty,
            None => {
                binds.insert(param, self_ty);
                true
            }
        },
        (TyKind::Adt { def: a, args: x }, TyKind::Adt { def: b, args: y }) => {
            a == b
                && x.len() == y.len()
                && x.iter()
                    .zip(y.iter())
                    .all(|(&p, &q)| matches_impl_self(tcx, p, q, binds))
        }
        (
            TyKind::Ref {
                base: a,
                mutability: m,
            },
            TyKind::Ref {
                base: b,
                mutability: n,
            },
        ) => m == n && matches_impl_self(tcx, a, b, binds),
        (TyKind::Iso(a), TyKind::Iso(b)) => matches_impl_self(tcx, a, b, binds),
        (TyKind::Tuple(a), TyKind::Tuple(b)) => {
            a.len() == b.len()
                && a.iter()
                    .zip(b.iter())
                    .all(|(&p, &q)| matches_impl_self(tcx, p, q, binds))
        }
        (
            TyKind::Array {
                elem: a,
                len: len_a,
            },
            TyKind::Array {
                elem: b,
                len: len_b,
            },
        ) => len_a == len_b && matches_impl_self(tcx, a, b, binds),
        (
            TyKind::Fun {
                params: a,
                ret: r_a,
            },
            TyKind::Fun {
                params: b,
                ret: r_b,
            },
        ) => {
            a.len() == b.len()
                && a.iter()
                    .zip(b.iter())
                    .all(|(&p, &q)| matches_impl_self(tcx, p, q, binds))
                && match (r_a, r_b) {
                    (Some(r_a), Some(r_b)) => matches_impl_self(tcx, r_a, r_b, binds),
                    (None, None) => true,
                    _ => false,
                }
        }
        _ => pattern == self_ty,
    }
}

fn queue_fn_def(
    def: DefId,
    mode: Option<AnyMode>,
    args: Vec<Ty>,
    self_ty: Option<Ty>,
    output: &HashMap<Instance, Body>,
    discovered: &mut Vec<Instance>,
) {
    let instance = Instance {
        def,
        any_mode: mode,
        args,
        self_ty,
    };
    if !output.contains_key(&instance) {
        discovered.push(instance);
    }
}

/// `Call::func` is frequently a `Constant(FnDef(..))` embedded directly in the terminator with
/// no corresponding `Assign` elsewhere in the body (a direct call, per `mir::lower::call`'s own
/// construction), so this needs the same discovery-capable substitution `subst_operand` gives a
/// statement's operands, not a version that skips it.
fn subst_terminator(
    tcx: &mut TyCtx,
    program: &Mir,
    kind: TerminatorKind,
    subst: &Subst,
    output: &mut HashMap<Instance, Body>,
    discovered: &mut Vec<Instance>,
) -> TerminatorKind {
    match kind {
        TerminatorKind::Call {
            func,
            args,
            destination,
            target,
        } => TerminatorKind::Call {
            func: subst_operand(tcx, program, func, subst, output, discovered),
            args: args
                .into_iter()
                .map(|a| subst_operand(tcx, program, a, subst, output, discovered))
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
            cond: subst_operand(tcx, program, cond, subst, output, discovered),
            expected,
            msg: subst_assert_message(tcx, program, msg, subst, output, discovered),
            target,
        },
        other => other,
    }
}

fn subst_assert_message(
    tcx: &mut TyCtx,
    program: &Mir,
    msg: AssertMessage,
    subst: &Subst,
    output: &mut HashMap<Instance, Body>,
    discovered: &mut Vec<Instance>,
) -> AssertMessage {
    match msg {
        AssertMessage::Assert(m) => AssertMessage::Assert(
            m.map(|op| subst_operand(tcx, program, op, subst, output, discovered)),
        ),
        AssertMessage::Panic(m) => AssertMessage::Panic(
            m.map(|op| subst_operand(tcx, program, op, subst, output, discovered)),
        ),
        AssertMessage::Unreachable(m) => AssertMessage::Unreachable(
            m.map(|op| subst_operand(tcx, program, op, subst, output, discovered)),
        ),
        other => other,
    }
}
