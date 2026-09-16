use crate::diagnostics::Diagnostic;
use crate::diagnostics::codes;
use crate::driver::source::SrcSpan;
use crate::hir::{DefId, Hir, OwnerNode, Path};
use crate::session::Session;

pub fn report_bound_is_not_a_trait(session: &Session, path: &Path) {
    let name = path
        .segments
        .last()
        .map_or("this path", |segment| session.resolve(segment.text));

    session.emit(
        Diagnostic::error(format!("`{name}` is not a trait"), path.span())
            .with_code(codes::BOUND_IS_NOT_A_TRAIT)
            .with_label("not a trait")
            .with_help(
                "a bound says what a type parameter must implement, and only a trait can be \
                     implemented; a bound naming anything else promises the body something \
                     nothing could ever supply",
            ),
    );
}

pub fn report_arg_count_mismatch(
    session: &Session,
    hir: &Hir,
    def: DefId,
    declared: usize,
    found: usize,
    span: SrcSpan,
) {
    let plural = if declared == 1 { "" } else { "s" };
    session.emit(
        Diagnostic::error(
            format!(
                "`{}` takes {declared} generic argument{plural} but {found} {} supplied",
                defined_name(session, hir, def),
                if found == 1 { "was" } else { "were" }
            ),
            span,
        )
        .with_code(codes::ARG_COUNT_MISMATCH)
        .with_label(format!("expected {declared} argument{plural}"))
        .with_secondary(
            defined_span(hir, def),
            format!(
                "`{}` declares {declared} type parameter{plural} here",
                defined_name(session, hir, def)
            ),
        )
        .with_help(
            "every parameter has to be given an argument, since the declaration is written in \
                 terms of all of them",
        ),
    );
}

fn defined_name(session: &Session, hir: &Hir, def: DefId) -> &'static str {
    let name = match hir.def(def) {
        OwnerNode::Function(f) => f.name.text,
        OwnerNode::Struct(s) => s.name.text,
        OwnerNode::Enum(e) => e.name.text,
        OwnerNode::Trait(t) => t.name.text,
        OwnerNode::Extend(_) | OwnerNode::Module(_) | OwnerNode::Closure(_) => {
            unreachable!("only a named definition is ever applied to generic arguments")
        }
    };
    session.resolve(name)
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
