use crate::ast::{Ident, SelfMode};
use crate::diagnostics::Diagnostic;
use crate::diagnostics::codes;
use crate::diagnostics::display::DisplayCtx;
use crate::diagnostics::typeck::show_self_mode;
use crate::driver::source::SrcSpan;
use crate::hir::{DefId, Hir};
use crate::session::Session;
use crate::typeck::ty::Ty;
use crate::typeck::unify::UnifyError;

pub fn report_receiver_type_unknown(session: &Session, member: Ident, span: SrcSpan) {
    session.emit(
        Diagnostic::error(
            format!(
                "type annotations needed: the type of the value `{}` is reached on is still \
                     unknown",
                session.resolve(member.text)
            ),
            span,
        )
        .with_code(codes::RECEIVER_TYPE_UNKNOWN)
        .with_label("the type here is still unknown")
        .with_help(
            "which `.` this is depends on the type it is written on, and unlike a trait \
                 bound it cannot wait for a later pass -- what it produces is what everything \
                 around it is checked against; write the type out",
        ),
    );
}

pub fn report_no_method(cx: DisplayCtx<'_>, member: Ident, base: Ty) {
    cx.emit(
        Diagnostic::error(
            format!(
                "no method `{}` on `{}`",
                cx.resolve(member.text),
                cx.show(base)
            ),
            member.span,
        )
        .with_code(codes::NO_METHOD)
        .with_label("not found")
        .with_help(
            "a method comes from an `extend` block for this type, or from a trait it \
                 implements; a method on a type parameter comes from a bound written on it",
        ),
    );
}

pub fn report_ambiguous_method(session: &Session, member: Ident, candidates: &[(&str, SrcSpan)]) {
    let traits: Vec<String> = candidates
        .iter()
        .map(|&(name, _)| format!("`{name}`"))
        .collect();

    let mut diag = Diagnostic::error(
        format!(
            "ambiguous method call: `{}` is declared by more than one trait in scope: {}",
            session.resolve(member.text),
            traits.join(", ")
        ),
        member.span,
    )
    .with_code(codes::AMBIGUOUS_METHOD)
    .with_label("cannot tell which one is meant")
    .with_help(
        "each of these traits declares a method of this name and the receiver reaches \
             all of them, so nothing here says which was meant",
    );

    for &(name, span) in candidates {
        diag = diag.with_secondary(span, format!("`{name}` declares it here"));
    }

    session.emit(diag);
}

pub fn report_no_receiver(session: &Session, hir: &Hir, member: Ident, method: DefId) {
    session.emit(
        Diagnostic::error(
            format!(
                "`{}` takes no receiver, so it cannot be called on a value",
                session.resolve(member.text)
            ),
            member.span,
        )
        .with_code(codes::NO_RECEIVER)
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
    session: &Session,
    hir: &Hir,
    member: Ident,
    mode: SelfMode,
    span: SrcSpan,
    method: DefId,
) {
    session.emit(
        Diagnostic::error(
            format!(
                "`{}` takes {}, which this receiver cannot provide",
                session.resolve(member.text),
                show_self_mode(mode)
            ),
            span,
        )
        .with_code(codes::RECEIVER_MODE)
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
    session: &Session,
    hir: &Hir,
    member: Ident,
    mode: SelfMode,
    span: SrcSpan,
    method: DefId,
) {
    session.emit(
        Diagnostic::error(
            format!(
                "`{}` takes {}, and this receiver is a temporary",
                session.resolve(member.text),
                show_self_mode(mode)
            ),
            span,
        )
        .with_code(codes::RECEIVER_NOT_A_PLACE)
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

pub fn report_field_is_a_method(cx: DisplayCtx<'_>, member: Ident, base: Ty) {
    let name = cx.resolve(member.text);
    cx.emit(
        Diagnostic::error(
            format!(
                "no field `{name}` on `{}`; there is a method `{name}`",
                cx.show(base)
            ),
            member.span,
        )
        .with_code(codes::FIELD_IS_A_METHOD)
        .with_label("this is a method, not a field")
        .with_help(format!(
            "did you mean to call it, as `{name}(..)`? a method cannot be named without \
                 calling it"
        )),
    );
}

pub fn report_not_callable(cx: DisplayCtx<'_>, sig: Ty, span: SrcSpan) {
    cx.emit(
        Diagnostic::error(
            format!("`{}` is not something that can be called", cx.show(sig)),
            span,
        )
        .with_code(codes::NOT_CALLABLE)
        .with_label("not a function"),
    );
}

pub fn report_call_arg_count(
    session: &Session,
    name: &str,
    found: usize,
    expected: usize,
    span: SrcSpan,
) {
    let plural = if expected == 1 { "" } else { "s" };
    session.emit(
        Diagnostic::error(
            format!(
                "{name} takes {expected} argument{plural} but {found} {} supplied",
                if found == 1 { "was" } else { "were" }
            ),
            span,
        )
        .with_code(codes::WRONG_ARG_COUNT)
        .with_label(format!("expected {expected} argument{plural}")),
    );
}

pub fn report_call_arg_mismatch(cx: DisplayCtx<'_>, err: UnifyError, span: SrcSpan) {
    cx.emit_unify(
        err,
        span,
        "this argument does not match the parameter it is passed to",
    );
}

pub fn function_name_span(hir: &Hir, method: DefId) -> SrcSpan {
    hir.function(method).name.span
}

pub fn method_receiver_span(hir: &Hir, method: DefId) -> SrcSpan {
    let function = hir.function(method);
    match function.self_param {
        Some(id) => hir.self_param(id).span,
        None => function.name.span,
    }
}

pub fn report_dyn_self_by_value(session: &Session, hir: &Hir, member: Ident, method: DefId) {
    session.emit(
        Diagnostic::error(
            format!(
                "`{}` takes `self` by value, so it cannot be called through a `dyn` receiver",
                session.resolve(member.text)
            ),
            member.span,
        )
        .with_code(codes::DYN_SELF_BY_VALUE)
        .with_label("this method consumes its receiver")
        .with_secondary(
            function_name_span(hir, method),
            "declared here, taking `self` by value",
        )
        .with_help(
            "a `dyn` value only carries a borrowed pointer to the concrete data, so a method \
             reached through one must borrow its receiver",
        ),
    );
}

pub fn report_dyn_method_mentions_self(session: &Session, hir: &Hir, member: Ident, method: DefId) {
    session.emit(
        Diagnostic::error(
            format!(
                "`{}` mentions `Self` outside its receiver, so it cannot be called through a \
                 `dyn` receiver",
                session.resolve(member.text)
            ),
            member.span,
        )
        .with_code(codes::DYN_METHOD_MENTIONS_SELF)
        .with_label("this method's signature depends on the concrete type")
        .with_secondary(
            function_name_span(hir, method),
            "declared here, mentioning `Self` in a parameter or the return type",
        )
        .with_help(
            "a call through a vtable must work for every implementing type alike, so the \
             method's other parameters and its return type cannot be the concrete `Self`",
        ),
    );
}
