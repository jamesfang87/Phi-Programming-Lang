use crate::ast::Ident;
use crate::ast::interner::Interner;
use crate::diagnostics::typeck::display::DisplayCx;
use crate::diagnostics::{DiagCtx, Diagnostic};
use crate::driver::source::SrcSpan;
use crate::hir::{Hir, HirId};
use crate::typeck::ty::Ty;
use crate::typeck::unify::UnifyError;

// -----------------------------------------------------------------
// Assignment
// -----------------------------------------------------------------

pub fn report_not_assignable(span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error("this expression cannot be assigned to", span)
            .with_label("not a place")
            .with_help(
                "the left side of an assignment has to name somewhere a value lives -- a \
                     local, a field, or an element -- rather than produce one",
            ),
    );
}

pub fn report_assign_mismatch(cx: DisplayCx<'_>, err: UnifyError, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(cx.show(err).to_string(), span)
            .with_label("this value cannot be assigned to the place on the left"),
    );
}

pub fn report_compound_assign_mismatch(cx: DisplayCx<'_>, err: UnifyError, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(cx.show(err).to_string(), span)
            .with_label("both sides of a compound assignment must have the same type"),
    );
}

pub fn report_compound_assign_result_mismatch(cx: DisplayCx<'_>, err: UnifyError, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(cx.show(err).to_string(), span)
            .with_label("this operator does not produce the type it would be assigned back to"),
    );
}

// -----------------------------------------------------------------
// Dereference
// -----------------------------------------------------------------

pub fn report_deref_not_a_reference(cx: DisplayCx<'_>, ty: Ty, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(format!("`{}` cannot be dereferenced", cx.show(ty)), span)
            .with_label("not a reference type")
            .with_help("`*` only applies to a value of type `&T` or `&mut T`"),
    );
}

// -----------------------------------------------------------------
// Indexing
// -----------------------------------------------------------------

pub fn report_index_base_unknown(span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(
            "type annotations needed: the type being indexed is still unknown",
            span,
        )
        .with_label("the type here is still unknown")
        .with_help(
            "what `[..]` means depends on the type it is written on: an array indexes \
                 built-in, and everything else through an `extend .. with Index` block",
        ),
    );
}

pub fn report_index_not_int(cx: DisplayCx<'_>, err: UnifyError, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(cx.show(err).to_string(), span)
            .with_label("an array is indexed by an integer"),
    );
}

pub fn report_not_indexable(cx: DisplayCx<'_>, base: Ty, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(format!("`{}` cannot be indexed", cx.show(base)), span)
            .with_label("no `index` method on this type")
            .with_help(
                "indexing an array is built in; every other type is indexed through an \
                 `extend .. with Index<K, V>` block",
            ),
    );
}

// -----------------------------------------------------------------
// Building a nominal value
// -----------------------------------------------------------------

pub fn report_elided_ctor_unknown(span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(
            "type annotations needed: `.{ .. }` names no struct, and the type it is expected \
                 to produce is unknown here",
            span,
        )
        .with_label("cannot tell which struct this builds")
        .with_help(
            "write the struct's name instead, or give the surrounding binding, parameter, or \
                 return type an annotation",
        ),
    );
}

pub fn report_ctor_not_a_struct(span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error("only a struct can be built with `{ .. }`", span)
            .with_label("not a struct"),
    );
}

pub fn report_not_a_struct_literal(cx: DisplayCx<'_>, ty: Ty, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(format!("`{}` is not a struct", cx.show(ty)), span)
            .with_label("only a struct is built with `{ .. }`")
            .with_help("an enum variant is built with `.variant`, not with a struct literal"),
    );
}

pub fn report_no_such_field(cx: DisplayCx<'_>, field: Ident, ty: Ty) {
    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "no field `{}` on `{}`",
                Interner::resolve(field.text),
                cx.show(ty)
            ),
            field.span,
        )
        .with_label("not a field of this struct"),
    );
}

pub fn report_private_field(field: Ident) {
    DiagCtx::emit(
        Diagnostic::error(
            format!("field `{}` is private", Interner::resolve(field.text)),
            field.span,
        )
        .with_label("not visible from here")
        .with_help("mark the field `public` to use it outside its declaring module"),
    );
}

pub fn report_duplicate_field(field: Ident) {
    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "field `{}` is given a value twice",
                Interner::resolve(field.text)
            ),
            field.span,
        )
        .with_label("already given a value above"),
    );
}

pub fn report_field_type_mismatch(cx: DisplayCx<'_>, err: UnifyError, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(cx.show(err).to_string(), span)
            .with_label("this value does not match the field's declared type"),
    );
}

pub fn report_missing_fields(cx: DisplayCx<'_>, missing: &[&str], ty: Ty, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(
            format!("`{}` is missing {}", cx.show(ty), list(missing)),
            span,
        )
        .with_label("every field has to be given a value"),
    );
}

pub fn report_variant_enum_unknown(variant: Ident, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "type annotations needed: the enum `.{}` belongs to is unknown here",
                Interner::resolve(variant.text)
            ),
            span,
        )
        .with_label("cannot tell which enum this variant belongs to")
        .with_help(
            "a `.variant` takes its enum from the type it is expected to produce -- from a \
                 binding's annotation, a parameter, or the enclosing function's return type",
        ),
    );
}

pub fn report_no_such_variant(cx: DisplayCx<'_>, variant: Ident, ty: Ty) {
    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "no variant `{}` on `{}`",
                Interner::resolve(variant.text),
                cx.show(ty)
            ),
            variant.span,
        )
        .with_label("not a variant of this type"),
    );
}

pub fn report_variant_payload_mismatch(cx: DisplayCx<'_>, err: UnifyError, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(cx.show(err).to_string(), span)
            .with_label("this value does not match the variant's declared payload"),
    );
}

pub fn report_variant_expr_payload_shape(
    hir: &Hir,
    variant: Ident,
    span: SrcSpan,
    declared: &str,
    variant_id: HirId,
) {
    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "variant `{}` carries {declared}",
                Interner::resolve(variant.text)
            ),
            span,
        )
        .with_label(format!("built with a payload that is not {declared}"))
        .with_secondary(hir.variant(variant_id).span, "declared here"),
    );
}

pub fn report_record_field_unknown(hir: &Hir, field: Ident, variant: HirId) {
    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "no field `{}` on this variant",
                Interner::resolve(field.text)
            ),
            field.span,
        )
        .with_label("not declared by this variant")
        .with_secondary(hir.variant(variant).span, "declared here"),
    );
}

pub fn report_variant_missing_fields(hir: &Hir, variant: HirId, missing: &[&str]) {
    DiagCtx::emit(
        Diagnostic::error(
            format!("this variant's payload is missing {}", list(missing)),
            hir.variant(variant).span,
        )
        .with_label("every declared field has to be given a value"),
    );
}

// -----------------------------------------------------------------
// Branching
// -----------------------------------------------------------------

pub fn report_if_cond_not_bool(cx: DisplayCx<'_>, err: UnifyError, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(cx.show(err).to_string(), span)
            .with_label("an `if` condition has to be a `bool`"),
    );
}

pub fn report_if_no_else_mismatch(cx: DisplayCx<'_>, err: UnifyError, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(cx.show(err).to_string(), span)
            .with_label("an `if` with no `else` produces no value")
            .with_help(
                "the block's last expression would be the `if`'s value, and there is \
                             no `else` branch to produce one on the other path",
            ),
    );
}

pub fn report_if_branches_mismatch(cx: DisplayCx<'_>, err: UnifyError, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(cx.show(err).to_string(), span)
            .with_label("both branches of an `if` have to produce the same type"),
    );
}

pub fn report_match_arm_mismatch(cx: DisplayCx<'_>, err: UnifyError, arm_span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(cx.show(err).to_string(), arm_span)
            .with_label("every arm of a `match` has to produce the same type"),
    );
}

pub fn report_match_guard_not_bool(cx: DisplayCx<'_>, err: UnifyError, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(cx.show(err).to_string(), span)
            .with_label("a match guard has to be a `bool`"),
    );
}

// -----------------------------------------------------------------
// Error propagation
// -----------------------------------------------------------------

pub fn report_try_operand_unknown(span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(
            "type annotations needed: the type `?` is applied to is still unknown",
            span,
        )
        .with_label("the type here is still unknown")
        .with_help("`?` produces what a `Result` or an `Option` carries, so it needs one"),
    );
}

pub fn report_not_try(cx: DisplayCx<'_>, ty: Ty, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(format!("`?` cannot be applied to `{}`", cx.show(ty)), span)
            .with_label("not a `Result` or an `Option`")
            .with_help(
                "`?` takes the value out of a `Result` or an `Option`, propagating the rest",
            ),
    );
}

pub fn report_try_outside(cx: DisplayCx<'_>, operand_ty: Ty, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "`?` on `{}` has nowhere to propagate to",
                cx.show(operand_ty)
            ),
            span,
        )
        .with_label("the enclosing definition declares no return type")
        .with_help(
            "`?` returns early on the failing case, so the enclosing function has to return \
                 the same kind of value",
        ),
    );
}

pub fn report_try_return_mismatch(cx: DisplayCx<'_>, operand_ty: Ty, ret: Ty, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(
            format!(
                "`?` on `{}` cannot propagate out of a function returning `{}`",
                cx.show(operand_ty),
                cx.show(ret)
            ),
            span,
        )
        .with_label("the two are not the same kind of value")
        .with_help(
            "`?` returns early with what it did not unwrap, so the enclosing function's return \
                 type has to be able to carry it",
        ),
    );
}

pub fn report_try_error_mismatch(cx: DisplayCx<'_>, err: UnifyError, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(cx.show(err).to_string(), span).with_label(
            "`?` propagates this error out of the function, whose declared error type it \
                     has to match",
        ),
    );
}

// -----------------------------------------------------------------
// Ranges
// -----------------------------------------------------------------

pub fn report_range_endpoints_mismatch(cx: DisplayCx<'_>, err: UnifyError, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(cx.show(err).to_string(), span)
            .with_label("a range's two endpoints have to have the same type"),
    );
}

/// TODO: Removes when std library Range is implemented
pub fn report_no_range_type(span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error("a range expression has no type yet", span)
            .with_label("`..` produces a value the core library declares no type for")
            .with_help(
                "a range is a value of a `Range` type, and there is no such type in `core` \
                     and no lang item naming one; iterate with `for x in ..` over a collection \
                     instead",
            ),
    );
}

// -----------------------------------------------------------------
// Closures
// -----------------------------------------------------------------

pub fn report_closure_body_mismatch(cx: DisplayCx<'_>, err: UnifyError, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(cx.show(err).to_string(), span).with_label(
            "this closure's body does not produce the return type it was checked against",
        ),
    );
}

// -----------------------------------------------------------------
// Casting
// -----------------------------------------------------------------

pub fn report_cast_target_not_primitive(cx: DisplayCx<'_>, ty: Ty, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(format!("cannot cast to `{}`", cx.show(ty)), span)
            .with_label("not a primitive type")
            .with_help(
                "`as` only ever converts between the primitive types -- the integers, the \
                 floats, `bool`, and `char`",
            ),
    );
}

pub fn report_cast_source_not_primitive(cx: DisplayCx<'_>, ty: Ty, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(
            format!("cannot cast a value of type `{}`", cx.show(ty)),
            span,
        )
        .with_label("not a primitive type")
        .with_help(
            "`as` only ever converts between the primitive types -- the integers, the \
                 floats, `bool`, and `char`",
        ),
    );
}

pub fn report_cast_operand_unknown(span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(
            "type annotations needed: the type being cast is still unknown",
            span,
        )
        .with_label("the type here is still unknown")
        .with_help(
            "give this value a concrete type first -- a literal suffix like `1_i32`, or a \
                 `let` annotation, both work -- since whether the cast loses anything depends \
                 on which type it starts from",
        ),
    );
}

pub fn report_cast_not_allowed(cx: DisplayCx<'_>, from: Ty, to: Ty, reason: &str, span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error(
            format!("cannot cast `{}` to `{}`", cx.show(from), cx.show(to)),
            span,
        )
        .with_label(reason.to_string())
        .with_help(
            "`as` only allows conversions that can never lose information; write out how the \
             value should be narrowed instead, e.g. by comparing it against the target type's \
             bounds first",
        ),
    );
}

fn list(names: &[&str]) -> String {
    let quoted: Vec<String> = names.iter().map(|name| format!("`{name}`")).collect();
    match quoted.split_last() {
        None => String::new(),
        Some((last, [])) => format!("field {last}"),
        Some((last, rest)) => format!("fields {} and {last}", rest.join(", ")),
    }
}
