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
//!    `ConstKind::FnDef(def, args, mode)` names one, queued through the worklist since a
//!    function's own declared generics can be zipped against a substituted `args` list; a
//!    closure nested inside is handled eagerly instead, recursing immediately with the *same*
//!    substitution map, since a closure declares no generics of its own to zip against at all --
//!    every `TyKind::Generic` its body mentions names a parameter of the *enclosing* definition.
//! 3. **Terminate**: the queue empties, or a depth guard reports a clear internal error instead
//!    of hanging on a pathological, ever-growing instantiation chain.
//!
//! One limitation this version has, called out here rather than silently mishandled: a method
//! declared inside a *generic* `extend<T> Foo<T> { .. }` block can reference the block's own
//! `T`, not just the method's own generics, and `TypeResolutions::call`'s recorded arguments
//! (see the prerequisite typeck change this pass relies on) only carry the method's own. Such a
//! method's body is substituted using only its own generics, which is correct whenever it
//! declares none of its own beyond `Self`'s already-concrete type, and wrong for one that mixes
//! both -- a narrower, follow-up prerequisite (recording the impl-level substitution too) would
//! close this, and is not attempted here.

mod subst;
#[cfg(test)]
mod tests;

use std::collections::HashMap;

use crate::hir::{DefId, Hir};
use crate::mir::lower::LoweredProgram;
use crate::mir::{
    AggregateKind, AnyMode, Body, ConstKind, Instance, Operand, Rvalue, StatementKind,
    TerminatorKind,
};
use crate::typeck::ty::Ty;
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
pub fn monomorphize(
    hir: &Hir,
    tcx: &mut TyCtx,
    program: &LoweredProgram,
) -> HashMap<Instance, Body> {
    let mut output = HashMap::new();
    let mut worklist: Vec<Instance> = Vec::new();

    for (&(def, any_mode), body) in &program.bodies {
        if !body_mentions_generic(tcx, body) {
            worklist.push(Instance {
                def,
                any_mode,
                args: Vec::new(),
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
            panic!("mir::monomorphize: no lowered body for {instance:?}");
        };
        let subst = build_subst(hir, instance.def, &instance.args);
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

fn build_subst(hir: &Hir, def: DefId, args: &[Ty]) -> HashMap<crate::hir::HirId, Ty> {
    hir.function(def)
        .generics
        .iter()
        .copied()
        .zip(args.iter().copied())
        .collect()
}

fn body_mentions_generic(tcx: &TyCtx, body: &Body) -> bool {
    body.local_decls
        .iter()
        .any(|decl| subst::mentions_generic(tcx, decl.ty))
}

/// Substitutes every `Ty` `generic_body` contains, discovering further instances along the way.
/// A nested closure is substituted eagerly, right here, and inserted into `output` directly,
/// since it shares `instance`'s own substitution rather than needing a worklist entry of its
/// own; a nested `FnDef` call is pushed onto `discovered` instead, since it has its own declared
/// generics an argument list can be zipped against independently.
#[allow(clippy::too_many_arguments)]
fn process_body(
    tcx: &mut TyCtx,
    program: &LoweredProgram,
    generic_body: &Body,
    subst: &HashMap<crate::hir::HirId, Ty>,
    instance: &Instance,
    output: &mut HashMap<Instance, Body>,
    discovered: &mut Vec<Instance>,
) -> Body {
    let mut body = Body {
        def_id: generic_body.def_id,
        basic_blocks: Vec::with_capacity(generic_body.basic_blocks.len()),
        local_decls: Vec::with_capacity(generic_body.local_decls.len()),
        arg_count: generic_body.arg_count,
        span: generic_body.span,
    };

    for decl in &generic_body.local_decls {
        body.local_decls.push(crate::mir::LocalDecl {
            ty: subst::subst_ty(tcx, decl.ty, subst),
            mutability: decl.mutability,
            name: decl.name,
            span: decl.span,
        });
    }

    for block in &generic_body.basic_blocks {
        let statements = block
            .statements
            .iter()
            .map(|stmt| crate::mir::Statement {
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
    program: &LoweredProgram,
    kind: StatementKind,
    subst: &HashMap<crate::hir::HirId, Ty>,
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
    program: &LoweredProgram,
    rvalue: Rvalue,
    subst: &HashMap<crate::hir::HirId, Ty>,
    instance: &Instance,
    output: &mut HashMap<Instance, Body>,
    discovered: &mut Vec<Instance>,
) -> Rvalue {
    match rvalue {
        Rvalue::Use(operand) => Rvalue::Use(subst_operand(tcx, operand, subst, output, discovered)),
        Rvalue::Ref { mutability, place } => Rvalue::Ref { mutability, place },
        Rvalue::BinaryOp(op, lhs, rhs) => Rvalue::BinaryOp(
            op,
            subst_operand(tcx, lhs, subst, output, discovered),
            subst_operand(tcx, rhs, subst, output, discovered),
        ),
        Rvalue::CheckedBinaryOp(op, lhs, rhs) => Rvalue::CheckedBinaryOp(
            op,
            subst_operand(tcx, lhs, subst, output, discovered),
            subst_operand(tcx, rhs, subst, output, discovered),
        ),
        Rvalue::UnaryOp(op, operand) => {
            Rvalue::UnaryOp(op, subst_operand(tcx, operand, subst, output, discovered))
        }
        Rvalue::Cast { operand, ty, kind } => Rvalue::Cast {
            operand: subst_operand(tcx, operand, subst, output, discovered),
            ty: subst::subst_ty(tcx, ty, subst),
            kind,
        },
        Rvalue::Aggregate(kind, operands) => {
            if let AggregateKind::Closure { def } = *kind {
                let closure_instance = Instance {
                    def,
                    any_mode: None,
                    args: instance.args.clone(),
                };
                if !output.contains_key(&closure_instance) {
                    let Some(closure_generic_body) = program.bodies.get(&(def, None)) else {
                        panic!("mir::monomorphize: no lowered body for closure {def:?}");
                    };
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
                .map(|op| subst_operand(tcx, op, subst, output, discovered))
                .collect();
            Rvalue::Aggregate(kind, operands)
        }
        Rvalue::Discriminant(place) => Rvalue::Discriminant(place),
        Rvalue::Len(place) => Rvalue::Len(place),
    }
}

fn subst_operand(
    tcx: &mut TyCtx,
    operand: Operand,
    subst: &HashMap<crate::hir::HirId, Ty>,
    output: &mut HashMap<Instance, Body>,
    discovered: &mut Vec<Instance>,
) -> Operand {
    let Operand::Constant(constant) = operand else {
        return operand;
    };
    let ty = subst::subst_ty(tcx, constant.ty, subst);
    let kind = match constant.kind {
        ConstKind::FnDef(def, args, mode) => {
            let args: Vec<Ty> = args
                .iter()
                .map(|&a| subst::subst_ty(tcx, a, subst))
                .collect();
            queue_fn_def(def, mode, args.clone(), output, discovered);
            ConstKind::FnDef(def, args, mode)
        }
        other => other,
    };
    Operand::Constant(crate::mir::Constant { ty, kind })
}

fn queue_fn_def(
    def: DefId,
    mode: Option<AnyMode>,
    args: Vec<Ty>,
    output: &HashMap<Instance, Body>,
    discovered: &mut Vec<Instance>,
) {
    let instance = Instance {
        def,
        any_mode: mode,
        args,
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
    kind: TerminatorKind,
    subst: &HashMap<crate::hir::HirId, Ty>,
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
            func: subst_operand(tcx, func, subst, output, discovered),
            args: args
                .into_iter()
                .map(|a| subst_operand(tcx, a, subst, output, discovered))
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
            cond: subst_operand(tcx, cond, subst, output, discovered),
            expected,
            msg,
            target,
        },
        other => other,
    }
}
