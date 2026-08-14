//! A `Diagnostic` carries one span and no second label, so an ambiguity cannot point at each
//! candidate's declaration. It points at the call -- the thing that has to change -- and names
//! the candidates in prose, the way `coherence`, `members` and `bounds` already do.

use crate::ast::interner::Interner;
use crate::ast::{Ident, SelfMode};
use crate::diag::{DiagCtx, Diagnostic};
use crate::diagnostics::typeck::display::DisplayCx;
use crate::driver::source::SrcSpan;
use crate::hir::{DefId, Hir, Node, OwnerNode};
use crate::typeck::ty::Ty;
use crate::typeck::unify::UnifyError;

pub fn report_receiver_unknown(member: Ident, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "type annotations needed: the type of the value `{}` is reached on is still \
                     unknown",
                Interner::resolve(member.text)
            ),
            span,
        )
        .with_label("the type here is still unknown")
        .with_help(
            "which `.` this is depends on the type it is written on, and unlike a trait \
                 bound it cannot wait for a later pass -- what it produces is what everything \
                 around it is checked against; write the type out",
        ),
    );
}

pub fn report_no_method(cx: DisplayCx<'_>, member: Ident, base: Ty) {
    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "no method `{}` on `{}`",
                Interner::resolve(member.text),
                cx.show(base)
            ),
            member.span,
        )
        .with_label("not found")
        .with_help(
            "a method comes from an `extend` block for this type, or from a trait it \
                 implements; a method on a type parameter comes from a bound written on it",
        ),
    );
}

/// `candidates` is each ambiguous trait's name alongside where it declares the method -- computed
/// by the caller, since only it can read the private fields of the `Candidate`s that produced
/// them.
pub fn report_ambiguous_method(member: Ident, candidates: &[(&str, SrcSpan)]) {
    let traits: Vec<String> = candidates
        .iter()
        .map(|&(name, _)| format!("`{name}`"))
        .collect();

    let mut diag = Diagnostic::error(
        format!(
            "ambiguous method call: `{}` is declared by more than one trait in scope: {}",
            Interner::resolve(member.text),
            traits.join(", ")
        ),
        member.span,
    )
    .with_label("cannot tell which one is meant")
    .with_help(
        "each of these traits declares a method of this name and the receiver reaches \
             all of them, so nothing here says which was meant",
    );

    // Every candidate gets underlined, not just the first two. Which ones collided is the
    // whole question here, and the answer is the set.
    for &(name, span) in candidates {
        diag = diag.with_secondary(span, format!("`{name}` declares it here"));
    }

    DiagCtx::emit(diag);
}

pub fn report_no_receiver(hir: &Hir, member: Ident, method: DefId) {
    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "`{}` takes no receiver, so it cannot be called on a value",
                Interner::resolve(member.text)
            ),
            member.span,
        )
        .with_label("declared without a `self` parameter")
        .with_secondary(
            function_name_span(hir, method),
            "declared here, taking no receiver",
        )
        .with_help(
            "a function declared in an `extend` block without a `self` parameter belongs to \
                 the type rather than to a value of it",
        ),
    );
}

pub fn report_receiver_mode(
    hir: &Hir,
    member: Ident,
    mode: SelfMode,
    span: SrcSpan,
    method: DefId,
) {
    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "`{}` takes {}, which this receiver cannot provide",
                Interner::resolve(member.text),
                show_self_mode(mode)
            ),
            span,
        )
        .with_label(format!("expected {}", show_self_mode(mode)))
        .with_secondary(
            method_receiver_span(hir, method),
            format!("declared taking {} here", show_self_mode(mode)),
        )
        .with_help(match mode {
            SelfMode::Move => {
                "this method takes its receiver by value, and the value here is behind a \
                     reference"
            }
            _ => "a shared reference cannot be used where a mutable one is required",
        }),
    );
}

pub fn report_receiver_not_a_place(
    hir: &Hir,
    member: Ident,
    mode: SelfMode,
    span: SrcSpan,
    method: DefId,
) {
    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "`{}` takes {}, and this receiver is a temporary",
                Interner::resolve(member.text),
                show_self_mode(mode)
            ),
            span,
        )
        .with_label("nowhere to take a reference to")
        .with_secondary(
            method_receiver_span(hir, method),
            format!("declared taking {} here", show_self_mode(mode)),
        )
        .with_help(
            "the reference the call would take is to a value that exists only for the \
                 length of this expression; bind it to a name first",
        ),
    );
}

pub fn report_no_field(cx: DisplayCx<'_>, member: Ident, base: Ty) {
    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "no field `{}` on `{}`",
                Interner::resolve(member.text),
                cx.show(base)
            ),
            member.span,
        )
        .with_label("not a field of this type"),
    );
}

pub fn report_field_is_a_method(cx: DisplayCx<'_>, member: Ident, base: Ty) {
    let name = Interner::resolve(member.text);
    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "no field `{name}` on `{}`; there is a method `{name}`",
                cx.show(base)
            ),
            member.span,
        )
        .with_label("this is a method, not a field")
        .with_help(format!(
            "did you mean to call it, as `{name}(..)`? a method cannot be named without \
                 calling it"
        )),
    );
}

pub fn report_not_callable(cx: DisplayCx<'_>, sig: Ty, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(
            format!("`{}` is not something that can be called", cx.show(sig)),
            span,
        )
        .with_label("not a function"),
    );
}

pub fn report_call_arg_count(name: &str, found: usize, expected: usize, span: SrcSpan) {
    let plural = if expected == 1 { "" } else { "s" };
    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "{name} takes {expected} argument{plural} but {found} {} supplied",
                if found == 1 { "was" } else { "were" }
            ),
            span,
        )
        .with_label(format!("expected {expected} argument{plural}")),
    );
}

pub fn report_call_arg_mismatch(cx: DisplayCx<'_>, err: UnifyError, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(cx.show(err).to_string(), span)
            .with_label("this argument does not match the parameter it is passed to"),
    );
}

/// Where a function's name is written, for a diagnostic pointing at the declaration it found.
pub fn function_name_span(hir: &Hir, method: DefId) -> SrcSpan {
    let OwnerNode::Function(function) = hir.def(method) else {
        unreachable!("a candidate's method is always a function");
    };
    function.name.span
}

/// Where a method's receiver is written, so a diagnostic about a receiver can point at what it
/// was checked against. Falls back to the method's name for one that declares none.
pub fn method_receiver_span(hir: &Hir, method: DefId) -> SrcSpan {
    let OwnerNode::Function(function) = hir.def(method) else {
        unreachable!("a candidate's method is always a function");
    };
    match function.self_param.map(|id| hir.node(id)) {
        Some(Node::SelfParam(self_param)) => self_param.span,
        Some(_) => unreachable!("a function's self param slot always holds a Node::SelfParam"),
        None => function.name.span,
    }
}

/// How a receiver reads in a diagnostic.
fn show_self_mode(mode: SelfMode) -> &'static str {
    match mode {
        SelfMode::Immutable => "`&self`",
        SelfMode::Mutable => "`&mut self`",
        SelfMode::Move => "`self`",
        SelfMode::Any => "`any self`",
    }
}
