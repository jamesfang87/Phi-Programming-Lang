use crate::ast::interner::Interner;
use crate::diagnostics::typeck::display::DisplayCx;
use crate::diagnostics::typeck::traits::solve::show_goal;
use crate::diagnostics::{DiagCtx, Diagnostic};
use crate::driver::source::SrcSpan;
use crate::hir::{DefId, Hir, OwnerNode, Path};
use crate::typeck::traits::solve::Obligation;

pub fn report_unsatisfied_bound(hir: &Hir, cx: DisplayCx<'_>, goal: &Obligation) {
    let mut diag = Diagnostic::error(
        format!(
            "the trait bound {} is not satisfied",
            show_goal(hir, cx, goal)
        ),
        goal.cause,
    )
    .with_label("this instantiation does not meet the bound its declaration writes")
    .with_help(
        "either write an `extend .. with` block implementing the trait for this type, or \
             pass a type that already has one",
    );

    if let Some(declared_at) = goal.declared_at {
        diag = diag.with_secondary(declared_at, "required by this bound");
    }

    DiagCtx::emit(diag);
}

pub fn report_annotations_needed(hir: &Hir, cx: DisplayCx<'_>, goal: &Obligation) {
    let mut diag = Diagnostic::error(
        format!(
            "type annotations needed: cannot tell whether {} holds",
            show_goal(hir, cx, goal)
        ),
        goal.cause,
    )
    .with_label("the type here is still unknown")
    .with_help(
        "nothing in this body pins the type down, so whether it satisfies the bound \
             cannot be decided; write the type out",
    );

    if let Some(declared_at) = goal.declared_at {
        diag = diag.with_secondary(declared_at, "the bound that has to be decided is here");
    }

    DiagCtx::emit(diag);
}

pub fn report_bound_is_not_a_trait(path: &Path) {
    let name = path
        .segments
        .last()
        .map_or("this path", |segment| Interner::resolve(segment.text));

    DiagCtx::emit(
        Diagnostic::error(format!("`{name}` is not a trait"), path.span)
            .with_label("not a trait")
            .with_help(
                "a bound says what a type parameter must implement, and only a trait can be \
                     implemented; a bound naming anything else promises the body something \
                     nothing could ever supply",
            ),
    );
}

pub fn report_arg_count_mismatch(
    hir: &Hir,
    def: DefId,
    declared: usize,
    found: usize,
    span: SrcSpan,
) {
    let plural = if declared == 1 { "" } else { "s" };
    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "`{}` takes {declared} generic argument{plural} but {found} {} supplied",
                defined_name(hir, def),
                if found == 1 { "was" } else { "were" }
            ),
            span,
        )
        .with_label(format!("expected {declared} argument{plural}"))
        .with_secondary(
            defined_span(hir, def),
            format!(
                "`{}` declares {declared} type parameter{plural} here",
                defined_name(hir, def)
            ),
        )
        .with_help(
            "every parameter has to be given an argument, since the declaration is written in \
                 terms of all of them",
        ),
    );
}

fn defined_name(hir: &Hir, def: DefId) -> &'static str {
    let name = match hir.def(def) {
        OwnerNode::Function(f) => f.name.text,
        OwnerNode::Struct(s) => s.name.text,
        OwnerNode::Enum(e) => e.name.text,
        OwnerNode::Trait(t) => t.name.text,
        OwnerNode::Extend(_) | OwnerNode::Module(_) | OwnerNode::Closure(_) => {
            unreachable!("only a named definition is ever applied to generic arguments")
        }
    };
    Interner::resolve(name)
}

fn defined_span(hir: &Hir, def: DefId) -> SrcSpan {
    match hir.def(def) {
        OwnerNode::Function(f) => f.name.span,
        OwnerNode::Struct(s) => s.name.span,
        OwnerNode::Enum(e) => e.name.span,
        OwnerNode::Trait(t) => t.name.span,
        OwnerNode::Extend(_) | OwnerNode::Module(_) | OwnerNode::Closure(_) => {
            unreachable!("only a named definition is ever applied to generic arguments")
        }
    }
}
