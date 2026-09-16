use crate::ast::SelfMode;
use crate::diagnostics::Diagnostic;
use crate::diagnostics::codes;
use crate::diagnostics::display::DisplayCtx;
use crate::diagnostics::typeck::show_optional_self_mode;
use crate::diagnostics::typeck::traits::get_name_of_trait;
use crate::driver::source::SrcSpan;
use crate::hir::{DefId, Function, Hir, HirId};
use crate::session::Session;
use crate::typeck::traits::TraitRef;
use crate::typeck::ty::Ty;

pub fn report_missing_methods(
    hir: &Hir,
    cx: DisplayCtx<'_>,
    missing: &[DefId],
    trait_ref: &TraitRef,
    self_ty: Ty,
    impl_span: SrcSpan,
) {
    let names: Vec<String> = missing
        .iter()
        .map(|&declaration| format!("`{}`", cx.resolve(hir.function(declaration).name.text)))
        .collect();
    let (plural, these) = if missing.len() == 1 {
        ("", "this")
    } else {
        ("s", "these")
    };

    let mut diag = Diagnostic::error(
        format!(
            "missing method{plural} in the implementation of trait `{}` for `{}`: {}",
            get_name_of_trait(cx.session(), hir, trait_ref.def),
            cx.show(self_ty),
            names.join(", ")
        ),
        impl_span,
    )
    .with_code(codes::MISSING_METHODS)
    .with_label(format!("{these} method{plural} not implemented"))
    .with_help(
        "every method a trait declares without a default body has to be written out by \
             each implementation; giving the declaration a body makes it optional instead",
    );

    for &declaration in missing {
        let declaration = hir.function(declaration);
        diag = diag.with_secondary(
            declaration.name.span,
            format!(
                "`{}` is declared here, with no default body",
                cx.resolve(declaration.name.text)
            ),
        );
    }

    cx.emit(diag);
}

pub fn report_not_a_member(
    hir: &Hir,
    cx: DisplayCtx<'_>,
    method: DefId,
    trait_ref: &TraitRef,
    self_ty: Ty,
) {
    let method = hir.function(method);
    let name = cx.resolve(method.name.text);
    let declared_trait_name = get_name_of_trait(cx.session(), hir, trait_ref.def);

    cx.emit(
        Diagnostic::error(
            format!("method `{name}` is not a member of trait `{declared_trait_name}`"),
            method.span,
        )
        .with_code(codes::NOT_A_MEMBER)
        .with_label(format!("not declared by `{declared_trait_name}`"))
        .with_secondary(
            declared_trait_span(hir, trait_ref.def),
            format!("`{declared_trait_name}` is declared here"),
        )
        .with_help(format!(
            "an `extend .. with {declared_trait_name}` block may only implement what \
             `{declared_trait_name}` declares, since that is all a caller reaching `{}` through \
             the trait can see; put `{name}` in an inherent `extend` block instead",
            cx.show(self_ty)
        )),
    );
}

pub fn report_generic_count(session: &Session, found: &Function, expected: &Function) {
    let (got, want) = (found.generics.len(), expected.generics.len());
    let plural = if want == 1 { "" } else { "s" };

    session.emit(
        Diagnostic::error(
            format!(
                "method `{}` declares {got} type parameters where its declaration declares {want}",
                session.resolve(found.name.text)
            ),
            found.name.span,
        )
        .with_code(codes::GENERIC_COUNT)
        .with_label(format!("expected {want} type parameter{plural}"))
        .with_secondary(
            expected.name.span,
            format!("declared with {want} type parameter{plural} here"),
        )
        .with_help(
            "an implementation has to be as general as the declaration it fulfills, so the \
                 two parameter lists have to line up one for one",
        ),
    );
}

pub fn report_self_mode(
    session: &Session,
    hir: &Hir,
    found: &Function,
    expected: &Function,
    found_mode: Option<SelfMode>,
    expected_mode: Option<SelfMode>,
) {
    session.emit(
        Diagnostic::error(
            format!(
                "method `{}` takes {} where its declaration takes {}",
                session.resolve(found.name.text),
                show_optional_self_mode(found_mode),
                show_optional_self_mode(expected_mode)
            ),
            self_param_span(hir, found),
        )
        .with_code(codes::SELF_MODE)
        .with_label(format!(
            "expected {}",
            show_optional_self_mode(expected_mode)
        ))
        .with_secondary(
            self_param_span(hir, expected),
            format!(
                "declared taking {} here",
                show_optional_self_mode(expected_mode)
            ),
        )
        .with_help(
            "how a method takes its receiver is part of its signature: a caller reaching it \
                 through the trait is checked against what the trait declared",
        ),
    );
}

pub fn report_param_count(
    session: &Session,
    found: &Function,
    expected: &Function,
    got: usize,
    want: usize,
) {
    let offset = usize::from(found.self_param.is_some());
    let (got, want) = (got - offset, want - offset);
    let plural = if want == 1 { "" } else { "s" };

    session.emit(
        Diagnostic::error(
            format!(
                "method `{}` takes {got} parameters where its declaration takes {want}",
                session.resolve(found.name.text)
            ),
            found.name.span,
        )
        .with_code(codes::PARAM_COUNT)
        .with_label(format!("expected {want} parameter{plural}"))
        .with_secondary(
            expected.name.span,
            format!("declared taking {want} parameter{plural} here"),
        ),
    );
}

pub fn report_param_ty(
    hir: &Hir,
    cx: DisplayCtx<'_>,
    found: &Function,
    param: HirId,
    declared_param: HirId,
    got: Ty,
    want: Ty,
) {
    let param = hir.param(param);
    let declared_param = hir.param(declared_param);

    cx.emit(
        Diagnostic::error(
            format!(
                "parameter `{}` of method `{}` has type `{}` where its declaration has `{}`",
                cx.resolve(param.name.text),
                cx.resolve(found.name.text),
                cx.show(got),
                cx.show(want)
            ),
            param.span,
        )
        .with_code(codes::PARAM_TY)
        .with_label(format!("expected `{}`", cx.show(want)))
        .with_secondary(
            declared_param.span,
            format!("declared as `{}` here", cx.show(want)),
        )
        .with_help(
            "a signature has to match its declaration exactly, not merely be compatible with \
                 it: a parameter that is more general still accepts arguments the trait never \
                 promised the implementation would take",
        ),
    );
}

pub fn report_ret_ty(
    hir: &Hir,
    cx: DisplayCtx<'_>,
    found: &Function,
    expected: &Function,
    got: Option<Ty>,
    want: Option<Ty>,
) {
    cx.emit(
        Diagnostic::error(
            format!(
                "method `{}` returns {} where its declaration returns {}",
                cx.resolve(found.name.text),
                show_ret(cx, got),
                show_ret(cx, want)
            ),
            ret_span(hir, found),
        )
        .with_code(codes::RET_TY)
        .with_label(format!("expected {}", show_ret(cx, want)))
        .with_secondary(
            ret_span(hir, expected),
            format!("declared returning {} here", show_ret(cx, want)),
        ),
    );
}

fn self_param_span(hir: &Hir, function: &Function) -> SrcSpan {
    function
        .self_param
        .map_or(function.name.span, |id| hir.self_param(id).span)
}

fn ret_span(hir: &Hir, function: &Function) -> SrcSpan {
    function
        .ret
        .map_or(function.name.span, |id| hir.ty(id).span)
}

fn show_ret(cx: DisplayCtx<'_>, ret: Option<Ty>) -> String {
    match ret {
        Some(ty) => format!("`{}`", cx.show(ty)),
        None => "nothing".to_string(),
    }
}

fn declared_trait_span(hir: &Hir, def: DefId) -> SrcSpan {
    hir.trait_(def).name.span
}
