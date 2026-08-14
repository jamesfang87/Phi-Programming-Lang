//! Every mismatch here is between two places: what the implementation wrote and what the trait
//! declared. The primary span is always the implementation's, because that is the side that has
//! to change -- the trait is what it is, and a method that disagrees with it is the one in the
//! wrong. The declaration gets a secondary label, at the narrowest part of it that differs: the
//! receiver for a receiver mismatch, the return type for a return mismatch, and so on, rather
//! than the whole signature every time.

use crate::ast::SelfMode;
use crate::ast::interner::Interner;
use crate::diag::{DiagCtx, Diagnostic};
use crate::diagnostics::typeck::display::DisplayCx;
use crate::diagnostics::typeck::traits::trait_name;
use crate::driver::source::SrcSpan;
use crate::hir::{DefId, Function, Hir, HirId, Node};
use crate::typeck::traits::TraitRef;
use crate::typeck::ty::Ty;

pub fn report_missing_methods(
    hir: &Hir,
    cx: DisplayCx<'_>,
    missing: &[DefId],
    trait_ref: &TraitRef,
    self_ty: Ty,
    impl_span: SrcSpan,
) {
    let names: Vec<String> = missing
        .iter()
        .map(|&declaration| {
            format!(
                "`{}`",
                Interner::resolve(function(hir, declaration).name.text)
            )
        })
        .collect();
    let (plural, these) = if missing.len() == 1 {
        ("", "this")
    } else {
        ("s", "these")
    };

    let mut diag = Diagnostic::error(
        format!(
            "missing method{plural} in the implementation of trait `{}` for `{}`: {}",
            trait_name(hir, trait_ref.def),
            cx.show(self_ty),
            names.join(", ")
        ),
        impl_span,
    )
    .with_label(format!("{these} method{plural} not implemented"))
    .with_help(
        "every method a trait declares without a default body has to be written out by \
             each implementation; giving the declaration a body makes it optional instead",
    );

    // One label per missing method rather than one for the trait as a whole: a trait with
    // twenty methods and two missing should point at the two.
    for &declaration in missing {
        let declaration = function(hir, declaration);
        diag = diag.with_secondary(
            declaration.name.span,
            format!(
                "`{}` is declared here, with no default body",
                Interner::resolve(declaration.name.text)
            ),
        );
    }

    DiagCtx::emit(diag);
}

pub fn report_not_a_member(
    hir: &Hir,
    cx: DisplayCx<'_>,
    method: DefId,
    trait_ref: &TraitRef,
    self_ty: Ty,
) {
    let method = function(hir, method);
    let name = Interner::resolve(method.name.text);
    let declared_trait_name = trait_name(hir, trait_ref.def);

    DiagCtx::emit(
        Diagnostic::error(
            format!("method `{name}` is not a member of trait `{declared_trait_name}`"),
            method.span,
        )
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

pub fn report_generic_count(found: &Function, expected: &Function) {
    let (got, want) = (found.generics.len(), expected.generics.len());
    let plural = if want == 1 { "" } else { "s" };

    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "method `{}` declares {got} type parameters where its declaration declares {want}",
                Interner::resolve(found.name.text)
            ),
            found.name.span,
        )
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
    hir: &Hir,
    found: &Function,
    expected: &Function,
    found_mode: Option<SelfMode>,
    expected_mode: Option<SelfMode>,
) {
    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "method `{}` takes {} where its declaration takes {}",
                Interner::resolve(found.name.text),
                show_self_mode(found_mode),
                show_self_mode(expected_mode)
            ),
            self_param_span(hir, found),
        )
        .with_label(format!("expected {}", show_self_mode(expected_mode)))
        .with_secondary(
            self_param_span(hir, expected),
            format!("declared taking {} here", show_self_mode(expected_mode)),
        )
        .with_help(
            "how a method takes its receiver is part of its signature: a caller reaching it \
                 through the trait is checked against what the trait declared",
        ),
    );
}

pub fn report_param_count(found: &Function, expected: &Function, got: usize, want: usize) {
    // Reported the way the user wrote it, so `self` -- which the checker counts as the first
    // parameter -- is not counted here.
    let offset = usize::from(found.self_param.is_some());
    let (got, want) = (got - offset, want - offset);
    let plural = if want == 1 { "" } else { "s" };

    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "method `{}` takes {got} parameters where its declaration takes {want}",
                Interner::resolve(found.name.text)
            ),
            found.name.span,
        )
        .with_label(format!("expected {want} parameter{plural}"))
        .with_secondary(
            expected.name.span,
            format!("declared taking {want} parameter{plural} here"),
        ),
    );
}

pub fn report_param_ty(
    hir: &Hir,
    cx: DisplayCx<'_>,
    found: &Function,
    param: HirId,
    declared_param: HirId,
    got: Ty,
    want: Ty,
) {
    let Node::Param(param) = hir.node(param) else {
        unreachable!("a function's parameter list holds only Node::Params");
    };
    let Node::Param(declared_param) = hir.node(declared_param) else {
        unreachable!("a function's parameter list holds only Node::Params");
    };

    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "parameter `{}` of method `{}` has type `{}` where its declaration has `{}`",
                Interner::resolve(param.name.text),
                Interner::resolve(found.name.text),
                cx.show(got),
                cx.show(want)
            ),
            param.span,
        )
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
    cx: DisplayCx<'_>,
    found: &Function,
    expected: &Function,
    got: Option<Ty>,
    want: Option<Ty>,
) {
    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "method `{}` returns {} where its declaration returns {}",
                Interner::resolve(found.name.text),
                show_ret(cx, got),
                show_ret(cx, want)
            ),
            ret_span(hir, found),
        )
        .with_label(format!("expected {}", show_ret(cx, want)))
        .with_secondary(
            ret_span(hir, expected),
            format!("declared returning {} here", show_ret(cx, want)),
        ),
    );
}

/// The function `def` names. Same assumption every caller here already relies on: a trait's
/// `functions` and an extend block's `methods` hold only functions.
fn function(hir: &Hir, def: DefId) -> &Function {
    let crate::hir::OwnerNode::Function(function) = hir.def(def) else {
        unreachable!("a trait's `functions` and an extend block's `methods` hold only functions");
    };
    function
}

/// Where a function's receiver is written, or its name when it takes none -- an associated
/// function has no receiver to underline, but "this one takes no `self`" still has to point
/// somewhere.
fn self_param_span(hir: &Hir, function: &Function) -> SrcSpan {
    function
        .self_param
        .map_or(function.name.span, |id| match hir.node(id) {
            Node::SelfParam(self_param) => self_param.span,
            _ => unreachable!("a function's self param slot always holds a Node::SelfParam"),
        })
}

/// Where a function's return type is written, or its name when it declares none. Same reason as
/// [`self_param_span`]: a missing `->` is exactly what some of these diagnostics are about.
fn ret_span(hir: &Hir, function: &Function) -> SrcSpan {
    function
        .ret
        .map_or(function.name.span, |id| match hir.node(id) {
            Node::Ty(ty) => ty.span,
            _ => unreachable!("a function's return slot always holds a Node::Ty"),
        })
}

/// How a return type reads in a diagnostic. A function with no `->` produces nothing, which is a
/// different thing to say than naming a type.
fn show_ret(cx: DisplayCx<'_>, ret: Option<Ty>) -> String {
    match ret {
        Some(ty) => format!("`{}`", cx.show(ty)),
        None => "nothing".to_string(),
    }
}

/// Where a trait was declared, for a diagnostic to point back at.
fn declared_trait_span(hir: &Hir, def: DefId) -> SrcSpan {
    let crate::hir::OwnerNode::Trait(trait_) = hir.def(def) else {
        unreachable!("a TraitRef's def always names a trait; the index is what enforces it");
    };
    trait_.name.span
}

/// How a receiver reads in a diagnostic, including the absence of one.
fn show_self_mode(mode: Option<SelfMode>) -> &'static str {
    match mode {
        Some(SelfMode::Immutable) => "`&self`",
        Some(SelfMode::Mutable) => "`&mut self`",
        Some(SelfMode::Move) => "`self`",
        Some(SelfMode::Any) => "`any self`",
        None => "no receiver",
    }
}
