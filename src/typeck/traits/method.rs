use std::collections::{HashMap, HashSet};

use crate::ast::interner::Interner;
use crate::ast::{Ident, Mutability, SelfMode, Symbol, UnaryOp};
use crate::diagnostics::typeck::traits::get_name_of_trait;
use crate::diagnostics::typeck::traits::method::{
    function_name_span, report_ambiguous_method, report_call_arg_count, report_call_arg_mismatch,
    report_field_is_a_method, report_no_field, report_no_method, report_no_receiver,
    report_not_callable, report_private_field, report_receiver_mode, report_receiver_not_a_place,
    report_receiver_type_unknown,
};
use crate::driver::source::SrcSpan;
use crate::hir::{AccessArgs, DefId, ExprKind, HirId, OwnerNode, Res};
use crate::typeck::Typeck;
use crate::typeck::fold;
use crate::typeck::ty::{Ty, TyKind};

/// A function that a method call could resolve to with the substitution mapping its
/// generic parameters to the concrete types they stand for at this call site.
#[derive(Clone, Debug)]
pub(crate) struct Candidate {
    /// The function this candidate calls if selected.
    method: DefId,
    source: CandidateSource,
    /// What `Self` denotes in `method`'s signature, always the receiver type
    /// [`peel_receiver`](Typeck::peel_receiver) produced.
    self_ty: Ty,

    /// Maps each generic parameter's `HirId` to the concrete type it stands for here: for
    /// `extend<T> Wrap<T> { fun get(&self) -> T }` called on `Wrap<i32>`, `subst` maps `T`'s
    /// `HirId` to `i32`. The method's own parameters go through [`Typeck::instantiate_method`].
    subst: HashMap<HirId, Ty>,

    /// The `extend` block this candidate came from and the arguments its own generic parameters
    /// were matched to, in declared order. `None` when the candidate comes from a bound in scope
    /// or from a `dyn` receiver, neither of which is backed by a block.
    extend_block_origin: Option<(DefId, Vec<Ty>)>,
}

/// Where a candidate's method was declared.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum CandidateSource {
    /// An `extend Foo { .. }` block with no trait.
    Inherent,
    Trait(DefId),
}

/// An indirection [`Typeck::peel_receiver`] strips from a receiver type on the way to the type
/// its methods are looked up on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Layer {
    /// A reference, carrying its mutability.
    Ref(Mutability),
    /// The `any T` indirection.
    Any,
}

impl<'hir> Typeck<'hir> {
    // -----------------------------------------------------------------
    // The two expression forms
    // -----------------------------------------------------------------

    /// Checks a `.` access expression, dispatching to a method call or a plain field read.
    pub(crate) fn check_access(
        &mut self,
        id: HirId,
        base: HirId,
        member: Ident,
        args: &AccessArgs,
    ) -> Ty {
        match args {
            AccessArgs::Call(args) => self.check_method_call(id, base, member, args),
            AccessArgs::None => self.check_field(base, member),
            AccessArgs::Record(_) => todo!("check_expr: Access (variant with a record payload)"),
        }
    }

    /// Checks a call expression `callee(args)`.
    pub(crate) fn check_call(
        &mut self,
        id: HirId,
        callee: HirId,
        args: &[HirId],
        span: SrcSpan,
    ) -> Ty {
        let (sig, instantiation) = self.callee_sig(callee);

        let TyKind::Fun { params, ret } = self.tcx.kind(sig).clone() else {
            // Every argument is still checked here, so a mistake inside one of them is reported
            // alongside this diagnostic rather than only once the callee is fixed.
            for &arg in args {
                self.ty_of(arg);
            }
            if !matches!(self.tcx.kind(sig), TyKind::Error) {
                report_not_callable(self.display_cx(), sig, span);
            }
            return self.tcx.error();
        };

        self.check_args(&params, args, "this call", span);
        if let Some((def, generic_args)) = instantiation {
            let resolved = self.resolve_all(&generic_args);
            self.register_bound_obligations(def, &resolved, span, callee.owner);
            self.types.record_call(id, def, resolved);
        } else if let ExprKind::Path(path) = &self.hir.expr(callee).kind
            && let Res::Function(def) = path.res
        {
            // A named, non-generic function has nothing to instantiate, but MIR lowering still
            // needs a resolved call target to address it by `DefId` directly.
            self.types.record_call(id, def, Vec::new());
        }
        ret.unwrap_or_else(|| self.tcx.unit())
    }

    fn callee_sig(&mut self, callee: HirId) -> (Ty, Option<(DefId, Vec<Ty>)>) {
        let Some((def, generics)) = self.generic_callee(callee) else {
            // Anything else, a closure, a field holding a function, or a call to something with
            // no parameters of its own, has nothing to instantiate, so its type is already the
            // signature.
            return (self.ty_of(callee), None);
        };

        let sig = self.ty_of(def.owner_id());
        let args: Vec<Ty> = generics.iter().map(|_| self.tcx.next_ty_var()).collect();
        let subst: HashMap<HirId, Ty> = generics.into_iter().zip(args.iter().copied()).collect();
        let sig = self.subst_ty(sig, &subst);

        // Recorded rather than left to `ty_of`, so the type the table holds for this node is the
        // one the call was actually checked against.
        self.types.record(callee, sig);
        (sig, Some((def, args)))
    }

    fn generic_callee(&self, callee: HirId) -> Option<(DefId, Vec<HirId>)> {
        let ExprKind::Path(path) = &self.hir.expr(callee).kind else {
            return None;
        };
        let Res::Function(def) = path.res else {
            return None;
        };
        let OwnerNode::Function(function) = self.hir.def(def) else {
            return None;
        };
        (!function.generics.is_empty()).then(|| (def, function.generics.clone()))
    }

    /// Resolves every type in `tys` to what the unifier currently knows it stands for.
    pub(crate) fn resolve_all(&mut self, tys: &[Ty]) -> Vec<Ty> {
        tys.iter()
            .map(|&ty| self.unifier.find_deep(&mut self.tcx, ty))
            .collect()
    }

    // -----------------------------------------------------------------
    // Method calls
    // -----------------------------------------------------------------

    /// Checks a method call `receiver.member(args)`, where `id` names the call expression
    /// itself.
    pub(crate) fn check_method_call(
        &mut self,
        id: HirId,
        receiver: HirId,
        member: Ident,
        args: &[HirId],
    ) -> Ty {
        let owner = receiver.owner;
        let receiver_ty = self.ty_of(receiver);

        // Step 1: the receiver's type must already be known. Resolution picks a candidate from
        // what the receiver's type is, so unlike a trait bound it cannot defer to a later pass.
        if matches!(self.tcx.kind(receiver_ty), TyKind::Var(_)) {
            report_receiver_type_unknown(member, self.hir.expr(receiver).span);
            return self.check_unresolved_call_args(args);
        }
        if matches!(self.tcx.kind(receiver_ty), TyKind::Error) {
            return self.check_unresolved_call_args(args);
        }

        // Step 2: strip references and `any` indirections to reach the type methods are looked
        // up on.
        let (base, layers) = self.peel_receiver(receiver_ty);

        // Step 3: collect every candidate `member` could name on `base`.
        let candidates = self.method_candidates(base, member.text, owner);
        if candidates.is_empty() {
            report_no_method(self.display_cx(), member, base);
            return self.check_unresolved_call_args(args);
        }

        // Step 4: settle on exactly one candidate, or report why the call is ambiguous.
        let Some(chosen) = self.select_candidate(&candidates, member) else {
            return self.check_unresolved_call_args(args);
        };

        // Step 5: check the receiver and arguments against the chosen method's signature.
        self.check_chosen_method(id, &chosen, receiver, &layers, member, args)
    }

    /// Instantiates `chosen`'s signature at this call site, checks the receiver against its
    /// self mode, and checks the argument list against its parameters.
    fn check_chosen_method(
        &mut self,
        id: HirId,
        chosen: &Candidate,
        receiver: HirId,
        layers: &[Layer],
        member: Ident,
        args: &[HirId],
    ) -> Ty {
        let span = self.hir.expr(id).span;
        let mode = self.receiver_mode(chosen.method);
        let (params, ret, fresh) =
            self.instantiate_method(chosen.method, &chosen.subst, chosen.self_ty);

        match mode {
            Some(mode) => self.check_receiver(mode, receiver, layers, member, chosen.method),
            None => report_no_receiver(self.hir, member, chosen.method),
        }

        // A method's `self` counts as its first parameter (see
        // [`collect_function`](Typeck::collect_function)), and it was already checked above by
        // `check_receiver`, so the written arguments start at index one.
        let expected = &params[usize::from(mode.is_some())..];
        let name = format!("`{}`", Interner::resolve(member.text));
        self.check_args(expected, args, &name, span);

        // A method may declare its own generic parameters. Calling it instantiates them the same
        // way calling a free function does, after the arguments have been checked.
        let resolved = self.resolve_all(&fresh);
        self.register_bound_obligations(chosen.method, &resolved, span, receiver.owner);

        // An `extend` block's own bounds condition the methods it offers: `extend<T: Show>
        // Wrap<T>` only gives its methods to a `Wrap<T>` whose `T` implements `Show`. Only the
        // picked candidate's block raises this bound, deferred like any other bound.
        if let Some((block, args)) = chosen.extend_block_origin.clone() {
            self.register_bound_obligations(block, &args, span, receiver.owner);
        }

        self.types.record_call(id, chosen.method, resolved);

        ret.unwrap_or_else(|| self.tcx.unit())
    }

    pub(crate) fn method_candidates(
        &mut self,
        base: Ty,
        member: Symbol,
        owner: DefId,
    ) -> Vec<Candidate> {
        let mut candidates = Vec::new();

        // Inherent and trait `extend` blocks, both keyed on the head of the self type, so only
        // a struct or enum receiver can match one.
        if let TyKind::Adt { def, .. } = *self.tcx.kind(base) {
            for block in self.extends.for_type(def).to_vec() {
                if let Some(candidate) = self.candidate_from_extend_block(block, base, member) {
                    candidates.push(candidate);
                }
            }
        }

        // A `dyn Show` value implements exactly `Show`, so it offers exactly what `Show`
        // declares. There is no `extend` block behind it, since `extend` blocks are nominal, so
        // this is a rule here, the same way it is a rule in the query.
        if let TyKind::Dyn { trait_, args } = self.tcx.kind(base).clone()
            && let Some(method) = self.trait_method(trait_, member)
        {
            let subst = self.trait_subst(trait_, &args);
            candidates.push(Candidate {
                method,
                source: CandidateSource::Trait(trait_),
                self_ty: base,
                subst,
                extend_block_origin: None,
            });
        }

        // The bounds in scope are the only step that can answer for a receiver whose type is a
        // bare parameter, since a parameter is not in the index at all.
        for bound in self.bounds_env(owner).bounds {
            if bound.self_ty != base {
                continue;
            }
            let Some(method) = self.trait_method(bound.trait_.def, member) else {
                continue;
            };
            let subst = self.trait_subst(bound.trait_.def, &bound.trait_.args);
            candidates.push(Candidate {
                method,
                source: CandidateSource::Trait(bound.trait_.def),
                self_ty: base,
                subst,
                extend_block_origin: None,
            });
        }

        let mut seen = HashSet::new();
        candidates.retain(|candidate| seen.insert(candidate.method));
        candidates
    }

    fn candidate_from_extend_block(
        &mut self,
        block: DefId,
        base: Ty,
        member: Symbol,
    ) -> Option<Candidate> {
        let subst = self.header_applies(block, base)?;
        let provided = self.get_method_in_block(block, member);

        let generics = self.declared_generics(block);
        let block_origin = Some((block, self.instantiated_generics(generics, &subst)));

        let Some(trait_ref) = self.extends.trait_of(block).cloned() else {
            return provided.map(|method| Candidate {
                method,
                source: CandidateSource::Inherent,
                self_ty: base,
                subst,
                extend_block_origin: block_origin,
            });
        };

        let declared = self.trait_method(trait_ref.def, member)?;
        let source = CandidateSource::Trait(trait_ref.def);

        match provided {
            Some(method) => Some(Candidate {
                method,
                source,
                self_ty: base,
                subst,
                extend_block_origin: block_origin,
            }),
            None => {
                let args: Vec<Ty> = trait_ref
                    .args
                    .iter()
                    .map(|&arg| self.subst_ty(arg, &subst))
                    .collect();
                let subst = self.trait_subst(trait_ref.def, &args);
                Some(Candidate {
                    method: declared,
                    source,
                    self_ty: base,
                    subst,
                    extend_block_origin: block_origin,
                })
            }
        }
    }

    fn instantiated_generics(&mut self, generics: &[HirId], subst: &HashMap<HirId, Ty>) -> Vec<Ty> {
        generics
            .iter()
            .map(|&param| {
                subst
                    .get(&param)
                    .copied()
                    .unwrap_or_else(|| self.tcx.mk_generic(param))
            })
            .collect()
    }

    fn select_candidate(&self, candidates: &[Candidate], member: Ident) -> Option<Candidate> {
        if let Some(inherent) = candidates
            .iter()
            .find(|candidate| candidate.source == CandidateSource::Inherent)
        {
            return Some(inherent.clone());
        }

        match candidates {
            [only] => Some(only.clone()),
            [] => unreachable!("select_candidate is only asked about a non-empty candidate list"),
            ambiguous_candidates => {
                let candidates: Vec<(&str, SrcSpan)> = ambiguous_candidates
                    .iter()
                    .map(|candidate| {
                        let CandidateSource::Trait(def) = candidate.source else {
                            unreachable!(
                                "an inherent candidate wins outright, so it is never ambiguous"
                            );
                        };
                        (
                            get_name_of_trait(self.hir, def),
                            function_name_span(self.hir, candidate.method),
                        )
                    })
                    .collect();
                report_ambiguous_method(member, &candidates);
                None
            }
        }
    }

    // -----------------------------------------------------------------
    // Signatures
    // -----------------------------------------------------------------

    fn instantiate_method(
        &mut self,
        method: DefId,
        subst: &HashMap<HirId, Ty>,
        self_ty: Ty,
    ) -> (Vec<Ty>, Option<Ty>, Vec<Ty>) {
        let function = self.hir.function(method);
        let generics = function.generics.clone();

        let mut subst = subst.clone();
        let mut fresh = Vec::with_capacity(generics.len());
        for param in generics {
            let var = self.tcx.next_ty_var();
            subst.insert(param, var);
            fresh.push(var);
        }

        let (params, ret) = self
            .signature(method)
            .expect("collect_function records every method's own signature");

        let params = params
            .into_iter()
            .map(|ty| self.subst_sig_ty(ty, &subst, self_ty))
            .collect();
        let ret = ret.map(|ty| self.subst_sig_ty(ty, &subst, self_ty));
        (params, ret, fresh)
    }

    pub(crate) fn subst_sig_ty(&mut self, ty: Ty, subst: &HashMap<HirId, Ty>, self_ty: Ty) -> Ty {
        fold::fold_ty(&mut self.tcx, ty, &mut |tcx, ty| match *tcx.kind(ty) {
            TyKind::Generic(param) => Some(subst.get(&param).copied().unwrap_or(ty)),
            TyKind::SelfTy(_) => Some(self_ty),
            _ => None,
        })
    }

    pub(crate) fn trait_subst(&self, trait_def: DefId, args: &[Ty]) -> HashMap<HirId, Ty> {
        self.hir
            .trait_(trait_def)
            .generics
            .iter()
            .copied()
            .zip(args.iter().copied())
            .collect()
    }

    pub(crate) fn trait_method(&self, trait_def: DefId, name: Symbol) -> Option<DefId> {
        self.hir
            .trait_(trait_def)
            .functions
            .iter()
            .copied()
            .find(|&function| self.hir.function(function).name.text == name)
    }

    pub(crate) fn receiver_mode(&self, method: DefId) -> Option<SelfMode> {
        let function = self.hir.function(method);
        Some(self.hir.self_param(function.self_param?).mode)
    }

    // -----------------------------------------------------------------
    // Receivers
    // -----------------------------------------------------------------

    /// Strips the `&`, `&mut`, and `any` layers off a receiver
    pub(crate) fn peel_receiver(&self, ty: Ty) -> (Ty, Vec<Layer>) {
        let mut layers = Vec::new();
        let mut current = ty;
        loop {
            match *self.tcx.kind(current) {
                TyKind::Ref { base, mutability } => {
                    layers.push(Layer::Ref(mutability));
                    current = base;
                }
                TyKind::Any(base) => {
                    layers.push(Layer::Any);
                    current = base;
                }
                _ => return (current, layers),
            }
        }
    }

    fn check_receiver(
        &mut self,
        mode: SelfMode,
        receiver: HirId,
        layers: &[Layer],
        member: Ident,
        method: DefId,
    ) {
        let span = self.hir.expr(receiver).span;
        match mode {
            // `any self` accepts every receiver shape. That is its entire purpose as a mode.
            SelfMode::Any => {}
            SelfMode::Move => {
                if !layers.is_empty() {
                    report_receiver_mode(self.hir, member, mode, span, method);
                }
            }
            SelfMode::Immutable => {
                if layers.is_empty() && !self.is_place_expr(receiver) {
                    report_receiver_not_a_place(self.hir, member, mode, span, method);
                }
            }
            SelfMode::Mutable => match layers.first() {
                None => {
                    if !self.is_place_expr(receiver) {
                        report_receiver_not_a_place(self.hir, member, mode, span, method);
                    }
                }
                Some(Layer::Ref(Mutability::Immutable)) => {
                    report_receiver_mode(self.hir, member, mode, span, method);
                }
                Some(Layer::Ref(Mutability::Mutable) | Layer::Any) => {}
            },
        }
    }

    pub(crate) fn is_place_expr(&self, id: HirId) -> bool {
        matches!(
            &self.hir.expr(id).kind,
            ExprKind::Path(_)
                | ExprKind::Access {
                    args: AccessArgs::None,
                    ..
                }
                | ExprKind::Index { .. }
                | ExprKind::Unary {
                    op: UnaryOp::Deref,
                    ..
                }
        )
    }

    // -----------------------------------------------------------------
    // Fields
    // -----------------------------------------------------------------

    fn check_field(&mut self, base: HirId, member: Ident) -> Ty {
        let owner = base.owner;
        let base_ty = self.ty_of(base);

        if matches!(self.tcx.kind(base_ty), TyKind::Var(_)) {
            report_receiver_type_unknown(member, self.hir.expr(base).span);
            return self.tcx.error();
        }
        if matches!(self.tcx.kind(base_ty), TyKind::Error) {
            return self.tcx.error();
        }

        // A field is reached through references exactly as a method is.
        let (base_ty, _adjustment) = self.peel_receiver(base_ty);

        if let TyKind::Tuple(elems) = self.tcx.kind(base_ty).clone() {
            return self.check_tuple_field(&elems, member, base_ty);
        }

        if let Some(ty) = self.field_ty(base_ty, owner, member) {
            return ty;
        }

        if self
            .method_candidates(base_ty, member.text, owner)
            .is_empty()
        {
            report_no_field(self.display_cx(), member, base_ty);
        } else {
            report_field_is_a_method(self.display_cx(), member, base_ty);
        }
        self.tcx.error()
    }

    /// Checks a tuple index access, such as `t.0` on `t: (i32, bool)`. `member`'s text is the
    /// digits written after the `.`, parsed here rather than at the call site so an
    /// out-of-range or malformed index reports through the same diagnostic a named field would.
    fn check_tuple_field(&mut self, elems: &[Ty], member: Ident, base_ty: Ty) -> Ty {
        let index = Interner::resolve(member.text).parse::<usize>().ok();
        match index.and_then(|index| elems.get(index)) {
            Some(&ty) => ty,
            None => {
                report_no_field(self.display_cx(), member, base_ty);
                self.tcx.error()
            }
        }
    }

    fn field_ty(&mut self, base: Ty, owner: DefId, member: Ident) -> Option<Ty> {
        let (def, subst) = self.adt_and_generic_substs(base)?;
        let OwnerNode::Struct(struct_) = self.hir.def(def) else {
            return None;
        };

        let field = struct_
            .fields
            .clone()
            .into_iter()
            .find(|&id| self.hir.field(id).name.text == member.text)?;

        let visibility = self.hir.field(field).visibility;
        if !self.is_visible_from(self.hir.module_of(def), owner, visibility) {
            report_private_field(member);
        }

        let declared = self
            .types
            .ty(field)
            .expect("collect_fields records every field's declared type");
        Some(self.subst_ty(declared, &subst))
    }

    // -----------------------------------------------------------------
    // Argument lists
    // -----------------------------------------------------------------

    fn check_args(&mut self, expected: &[Ty], args: &[HirId], name: &str, span: SrcSpan) {
        let found: Vec<Ty> = args.iter().map(|&arg| self.ty_of(arg)).collect();

        if found.len() != expected.len() {
            report_call_arg_count(name, found.len(), expected.len(), span);
        }

        for (index, (&want, &got)) in expected.iter().zip(found.iter()).enumerate() {
            if let Err(err) = self.unify_allowing_any(want, got) {
                let span = self.hir.expr(args[index]).span;
                report_call_arg_mismatch(self.display_cx(), err, span);
            }
        }
    }

    fn check_unresolved_call_args(&mut self, args: &[HirId]) -> Ty {
        for &arg in args {
            self.ty_of(arg);
        }
        self.tcx.error()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::DiagCtx;
    use crate::hir::Hir;
    use crate::testing::{
        Stage, checker_through, find_return, first_extend_method, resolve_src, typeck_src as check,
    };

    /// Runs the checker through the point candidate collection reads, without checking function
    /// bodies.
    fn collected<'hir>(hir: &'hir Hir) -> Typeck<'hir> {
        let checker = checker_through(hir, Stage::Index);
        DiagCtx::clear();
        checker
    }

    /// The `DefId` of the top-level definition named `name`.
    fn named(checker: &Typeck<'_>, name: &str) -> DefId {
        crate::testing::named_def(checker.hir, name)
    }

    fn trait_of(candidate: &Candidate) -> DefId {
        match candidate.source {
            CandidateSource::Trait(def) => def,
            CandidateSource::Inherent => panic!("this candidate is inherent"),
        }
    }

    // -----------------------------------------------------------------
    // Candidate collection
    // -----------------------------------------------------------------

    #[test]
    fn an_inherent_block_offers_the_methods_it_defines() {
        let hir = resolve_src(
            "struct Foo {}
             extend Foo { fun show(&self) {} }",
        );
        let mut checker = collected(&hir);
        let (foo, root) = (named(&checker, "Foo"), hir.root_id());
        let foo_ty = checker.tcx.mk_adt(foo, vec![]);

        let candidates = checker.method_candidates(foo_ty, Interner::intern("show"), root);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].source, CandidateSource::Inherent);

        assert!(
            checker
                .method_candidates(foo_ty, Interner::intern("other"), root)
                .is_empty(),
            "a name the block does not define is not offered"
        );
    }

    #[test]
    fn a_trait_impl_offers_what_the_trait_declares() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             struct Foo {}
             extend Foo with Show { fun show(&self) {} }",
        );
        let mut checker = collected(&hir);
        let (foo, show, root) = (
            named(&checker, "Foo"),
            named(&checker, "Show"),
            hir.root_id(),
        );
        let foo_ty = checker.tcx.mk_adt(foo, vec![]);

        let candidates = checker.method_candidates(foo_ty, Interner::intern("show"), root);
        assert_eq!(candidates.len(), 1);
        assert_eq!(trait_of(&candidates[0]), show);
    }

    /// A defaulted method is available without appearing in the block, so the candidate is the
    /// trait's own declaration.
    #[test]
    fn a_defaulted_method_is_offered_by_an_impl_that_does_not_write_it() {
        let hir = resolve_src(
            "trait Show { fun show(&self) {} }
             struct Foo {}
             extend Foo with Show {}",
        );
        let mut checker = collected(&hir);
        let (foo, show, root) = (
            named(&checker, "Foo"),
            named(&checker, "Show"),
            hir.root_id(),
        );
        let foo_ty = checker.tcx.mk_adt(foo, vec![]);

        let candidates = checker.method_candidates(foo_ty, Interner::intern("show"), root);
        assert_eq!(candidates.len(), 1);
        let declared = hir.trait_(show);
        assert_eq!(candidates[0].method, declared.functions[0]);
    }

    /// The index has nothing to say about a bare parameter, so the environment is the only thing
    /// that can answer.
    #[test]
    fn a_bound_in_scope_offers_its_traits_methods_on_a_parameter() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             fun f<T: Show>(x: T) {}",
        );
        let mut checker = collected(&hir);
        let (f, show) = (named(&checker, "f"), named(&checker, "Show"));
        let function = hir.function(f);
        let t = checker.tcx.mk_generic(function.generics[0]);

        let candidates = checker.method_candidates(t, Interner::intern("show"), f);
        assert_eq!(candidates.len(), 1);
        assert_eq!(trait_of(&candidates[0]), show);
    }

    /// A bound on some *other* parameter says nothing about this one.
    #[test]
    fn a_bound_on_another_parameter_is_not_offered() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             fun f<T, U: Show>(x: T) {}",
        );
        let mut checker = collected(&hir);
        let f = named(&checker, "f");
        let function = hir.function(f);
        let t = checker.tcx.mk_generic(function.generics[0]);

        assert!(
            checker
                .method_candidates(t, Interner::intern("show"), f)
                .is_empty()
        );
    }

    #[test]
    fn a_dyn_receiver_offers_exactly_its_traits_methods() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             trait Other { fun other(&self); }",
        );
        let mut checker = collected(&hir);
        let (show, root) = (named(&checker, "Show"), hir.root_id());
        let dyn_show = checker.tcx.mk_dyn(show, vec![]);

        let candidates = checker.method_candidates(dyn_show, Interner::intern("show"), root);
        assert_eq!(candidates.len(), 1);
        assert_eq!(trait_of(&candidates[0]), show);

        assert!(
            checker
                .method_candidates(dyn_show, Interner::intern("other"), root)
                .is_empty(),
            "a `dyn` offers the methods of the trait it names and no others"
        );
    }

    /// An `extend` block whose header does not match this receiver is not a candidate, no
    /// matter how well the method name lines up.
    #[test]
    fn an_impl_whose_header_does_not_match_is_not_a_candidate() {
        let hir = resolve_src(
            "struct Wrap<T> { inner: T }
             struct Foo {}
             struct Bar {}
             extend Wrap<Foo> { fun show(&self) {} }",
        );
        let mut checker = collected(&hir);
        let (wrap, foo, bar, root) = (
            named(&checker, "Wrap"),
            named(&checker, "Foo"),
            named(&checker, "Bar"),
            hir.root_id(),
        );
        let (foo_ty, bar_ty) = (
            checker.tcx.mk_adt(foo, vec![]),
            checker.tcx.mk_adt(bar, vec![]),
        );
        let (wrap_foo, wrap_bar) = (
            checker.tcx.mk_adt(wrap, vec![foo_ty]),
            checker.tcx.mk_adt(wrap, vec![bar_ty]),
        );

        assert_eq!(
            checker
                .method_candidates(wrap_foo, Interner::intern("show"), root)
                .len(),
            1
        );
        assert!(
            checker
                .method_candidates(wrap_bar, Interner::intern("show"), root)
                .is_empty()
        );
    }

    /// Two bounds naming the same trait are one candidate, not an ambiguity between a candidate
    /// and itself.
    #[test]
    fn the_same_method_reached_twice_is_one_candidate() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             fun f<T: Show + Show>(x: T) {}",
        );
        let mut checker = collected(&hir);
        let f = named(&checker, "f");
        let function = hir.function(f);
        let t = checker.tcx.mk_generic(function.generics[0]);

        assert_eq!(
            checker
                .method_candidates(t, Interner::intern("show"), f)
                .len(),
            1
        );
    }

    // -----------------------------------------------------------------
    // Picking
    // -----------------------------------------------------------------

    /// The function named `name` that `owner` (a trait or an `extend` block) declares. A
    /// [`Candidate`]'s `method` is always a real function, since diagnostics point at where it
    /// was declared, so a fixture using the trait's own `DefId` would not fool the reporting path.
    fn method_of(checker: &Typeck<'_>, owner: DefId, name: &str) -> DefId {
        checker
            .hir
            .def_ids()
            .find(|&id| {
                checker.hir.parent(id) == Some(owner)
                    && matches!(
                        checker.hir.def(id),
                        OwnerNode::Function(f) if Interner::resolve(f.name.text) == name
                    )
            })
            .unwrap_or_else(|| panic!("no method named {name:?}"))
    }

    /// The function named `name` declared by the fixture's one `extend` block. An `extend` block
    /// has no name of its own to look it up by, and a fixture that needs this only ever writes
    /// one.
    fn extend_method(checker: &Typeck<'_>, name: &str) -> DefId {
        let block = crate::testing::first_extend(checker.hir);
        method_of(checker, block, name)
    }

    /// Builds a candidate for testing [`Typeck::select_candidate`] alone, which reads nothing
    /// but `source`. The block a real candidate came from matters only after one is picked.
    fn fixture_candidate(source: CandidateSource, method: DefId, self_ty: Ty) -> Candidate {
        Candidate {
            method,
            source,
            self_ty,
            subst: HashMap::new(),
            extend_block_origin: None,
        }
    }

    #[test]
    fn an_inherent_candidate_wins_over_a_trait_one() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             struct Foo {}
             extend Foo { fun show(&self) {} }",
        );
        let mut checker = collected(&hir);
        let (foo, show) = (named(&checker, "Foo"), named(&checker, "Show"));
        let inherent = extend_method(&checker, "show");
        let foo_ty = checker.tcx.mk_adt(foo, vec![]);
        let member = Ident {
            text: Interner::intern("show"),
            span: SrcSpan::new(0, 0),
        };

        // Trait first, so that "the inherent one" is not merely "the first one".
        let candidates = [
            fixture_candidate(
                CandidateSource::Trait(show),
                method_of(&checker, show, "show"),
                foo_ty,
            ),
            fixture_candidate(CandidateSource::Inherent, inherent, foo_ty),
        ];
        let picked = checker
            .select_candidate(&candidates, member)
            .expect("one candidate wins");
        assert_eq!(picked.source, CandidateSource::Inherent);
        assert!(DiagCtx::diagnostics().is_empty());
    }

    #[test]
    fn a_single_trait_candidate_is_picked() {
        let hir = resolve_src(
            "trait Show { fun show(&self); }
             struct Foo {}",
        );
        let mut checker = collected(&hir);
        let (foo, show) = (named(&checker, "Foo"), named(&checker, "Show"));
        let foo_ty = checker.tcx.mk_adt(foo, vec![]);
        let member = Ident {
            text: Interner::intern("show"),
            span: SrcSpan::new(0, 0),
        };

        let candidates = [fixture_candidate(
            CandidateSource::Trait(show),
            method_of(&checker, show, "show"),
            foo_ty,
        )];
        assert!(checker.select_candidate(&candidates, member).is_some());
        assert!(DiagCtx::diagnostics().is_empty());
    }

    #[test]
    fn two_trait_candidates_are_an_ambiguity_naming_both() {
        let hir = resolve_src(
            "trait A { fun size(&self); }
             trait B { fun size(&self); }
             struct Foo {}",
        );
        let mut checker = collected(&hir);
        let (foo, a, b) = (
            named(&checker, "Foo"),
            named(&checker, "A"),
            named(&checker, "B"),
        );
        let foo_ty = checker.tcx.mk_adt(foo, vec![]);
        let member = Ident {
            text: Interner::intern("size"),
            span: SrcSpan::new(0, 0),
        };

        let candidates = [
            fixture_candidate(
                CandidateSource::Trait(a),
                method_of(&checker, a, "size"),
                foo_ty,
            ),
            fixture_candidate(
                CandidateSource::Trait(b),
                method_of(&checker, b, "size"),
                foo_ty,
            ),
        ];
        assert!(checker.select_candidate(&candidates, member).is_none());
        assert_eq!(
            DiagCtx::diagnostics()
                .into_iter()
                .map(|diagnostic| diagnostic.message)
                .collect::<Vec<_>>(),
            ["ambiguous method call: `size` is declared by more than one trait in scope: `A`, `B`"]
        );
    }

    // -----------------------------------------------------------------
    // The block's own bounds
    //
    // A block's header only says which types it *matches*. What it promises them is conditional
    // on its own `<T: ..>` bounds, so picking a method out of one raises those bounds about the
    // receiver, exactly as instantiating any other declaration raises the bounds it writes.
    // -----------------------------------------------------------------

    /// A fixture with both kinds of conditional block: one implementing a trait, one inherent.
    const CONDITIONAL: &str = "trait Show { fun show(&self); }
         struct Foo {}
         struct Bare {}
         struct Wrap<T> { inner: T }
         extend Foo with Show { fun show(&self) {} }
         extend<T: Show> Wrap<T> with Show { fun show(&self) {} }
         extend<T: Show> Wrap<T> { fun get(&self) {} }";

    #[test]
    fn a_conditional_impls_method_is_available_when_the_bound_holds() {
        assert!(
            check(&format!(
                "{CONDITIONAL} fun f(x: Wrap<Foo>) {{ x.show(); }}"
            ))
            .is_empty()
        );
    }

    #[test]
    fn a_conditional_impls_method_is_not_available_when_the_bound_fails() {
        assert_eq!(
            check(&format!(
                "{CONDITIONAL} fun f(x: Wrap<Bare>) {{ x.show(); }}"
            )),
            ["the trait bound `Bare: Show` is not satisfied"]
        );
    }

    /// An inherent block is conditional in exactly the same way: `get` is offered to a `Wrap<T>`
    /// whose `T` implements `Show`, and there is no trait involved in saying so.
    #[test]
    fn a_conditional_inherent_blocks_method_is_not_available_when_the_bound_fails() {
        assert_eq!(
            check(&format!(
                "{CONDITIONAL} fun f(x: Wrap<Bare>) {{ x.get(); }}"
            )),
            ["the trait bound `Bare: Show` is not satisfied"]
        );
    }

    /// The bound is raised about the receiver and discharged wherever any other bound would be,
    /// which for a generic caller is its own environment.
    #[test]
    fn a_conditional_impls_bound_is_discharged_from_the_callers_own_bounds() {
        assert!(
            check(&format!(
                "{CONDITIONAL} fun f<U: Show>(x: Wrap<U>) {{ x.show(); }}"
            ))
            .is_empty()
        );
        assert_eq!(
            check(&format!(
                "{CONDITIONAL} fun f<U>(x: Wrap<U>) {{ x.show(); }}"
            )),
            ["the trait bound `U: Show` is not satisfied"]
        );
    }

    /// A method the block never wrote out is inherited from the trait's default body, and is as
    /// conditional as one it did: the block is still what makes the trait apply to this type.
    #[test]
    fn a_method_inherited_through_a_conditional_impl_needs_the_blocks_bound_too() {
        assert_eq!(
            check(
                "trait Show { fun show(&self) {} }
                 struct Bare {}
                 struct Wrap<T> { inner: T }
                 extend<T: Show> Wrap<T> with Show {}
                 fun f(x: Wrap<Bare>) { x.show(); }"
            ),
            ["the trait bound `Bare: Show` is not satisfied"]
        );
    }

    // -----------------------------------------------------------------
    // Source-level
    // -----------------------------------------------------------------

    #[test]
    fn an_inherent_method_call_checks() {
        assert!(
            check(
                "struct Foo {}
                 extend Foo { fun show(&self) {} }
                 fun f(x: Foo) { x.show(); }"
            )
            .is_empty()
        );
    }

    #[test]
    fn a_trait_method_call_checks() {
        assert!(
            check(
                "trait Show { fun show(&self); }
                 struct Foo {}
                 extend Foo with Show { fun show(&self) {} }
                 fun f(x: Foo) { x.show(); }"
            )
            .is_empty()
        );
    }

    /// The case the environment exists for: nothing is known about `T` but the bound.
    #[test]
    fn a_method_reached_through_a_bound_checks() {
        assert!(
            check(
                "trait Show { fun show(&self); }
                 fun f<T: Show>(x: T) { x.show(); }"
            )
            .is_empty()
        );
    }

    #[test]
    fn a_method_on_a_parameter_with_no_bound_is_not_found() {
        assert_eq!(
            check(
                "trait Show { fun show(&self); }
                 fun f<T>(x: T) { x.show(); }"
            ),
            ["no method `show` on `T`"]
        );
    }

    /// Inside a trait, `Self` implements that trait by definition, letting one default method
    /// body call another method of the same trait.
    #[test]
    fn a_default_body_may_call_another_method_of_its_own_trait() {
        assert!(
            check("trait Show { fun show(&self); fun show_twice(&self) { self.show(); } }")
                .is_empty()
        );
    }

    #[test]
    fn a_method_on_a_dyn_receiver_checks() {
        assert!(
            check(
                "trait Show { fun show(&self); }
                 fun f(x: &dyn Show) { x.show(); }"
            )
            .is_empty()
        );
    }

    /// An inherent `extend Foo` block always wins for `Foo` specifically. Coherence separately
    /// rejects two `extend` blocks that offer the same name for one type, so that diagnostic is
    /// expected here; picking the trait's method instead would additionally mismatch the `return`.
    #[test]
    fn an_inherent_method_beats_a_trait_method_of_the_same_name() {
        assert_eq!(
            check(
                "trait Show { fun show(&self) -> bool; }
                 struct Foo {}
                 extend Foo with Show { fun show(&self) -> bool { return true; } }
                 extend Foo { fun show(&self) -> i32 { return 0; } }
                 fun f(x: Foo) -> i32 { return x.show(); }"
            ),
            ["the method `show` is defined more than once for type `Foo`"]
        );
    }

    /// Coherence cannot see this one: neither trait is implemented for anything in particular
    /// here, so the collision only exists at the call site.
    #[test]
    fn a_method_declared_by_two_bounds_is_ambiguous() {
        assert_eq!(
            check(
                "trait A { fun size(&self); }
                 trait B { fun size(&self); }
                 fun f<T: A + B>(x: T) { x.size(); }"
            ),
            ["ambiguous method call: `size` is declared by more than one trait in scope: `A`, `B`"]
        );
    }

    #[test]
    fn an_unknown_method_is_reported() {
        assert_eq!(
            check(
                "struct Foo {}
                 extend Foo { fun show(&self) {} }
                 fun f(x: Foo) { x.nope(); }"
            ),
            ["no method `nope` on `Foo`"]
        );
    }

    #[test]
    fn a_field_access_checks_to_the_fields_type() {
        assert!(
            check(
                "struct Foo { count: i32 }
                 fun f(x: Foo) -> i32 { return x.count; }"
            )
            .is_empty()
        );
    }

    /// A field's declared type is read through the arguments the receiver applied.
    #[test]
    fn a_generic_structs_field_is_read_through_its_arguments() {
        assert_eq!(
            check(
                "struct Wrap<T> { inner: T }
                 fun f(x: Wrap<i32>) -> bool { return x.inner; }"
            ),
            ["mismatched types: expected `bool`, found `i32`"]
        );
    }

    #[test]
    fn a_tuple_index_access_checks_to_the_elements_type() {
        assert_eq!(
            check("fun f(t: (i32, bool)) -> bool { return t.0; }"),
            ["mismatched types: expected `bool`, found `i32`"]
        );
        assert!(check("fun f(t: (i32, bool)) -> i32 { return t.0; }").is_empty());
        assert!(check("fun f(t: (i32, bool)) -> bool { return t.1; }").is_empty());
    }

    #[test]
    fn a_chained_tuple_index_access_checks() {
        assert!(check("fun f(t: ((i32, bool), i32)) -> i32 { return t.0.0; }").is_empty());
        assert!(check("fun f(t: ((i32, bool), i32)) -> bool { return t.0.1; }").is_empty());
    }

    #[test]
    fn a_triple_chained_tuple_index_access_checks() {
        assert!(
            check("fun f(t: (i32, (bool, (i32, i32)))) -> i32 { return t.1.1.0; }").is_empty()
        );
        assert_eq!(
            check("fun f(t: (i32, (bool, (i32, i32)))) -> bool { return t.1.1.0; }"),
            ["mismatched types: expected `bool`, found `i32`"]
        );
    }

    #[test]
    fn an_out_of_range_tuple_index_is_reported() {
        assert_eq!(
            check("fun f(t: (i32, bool)) -> i32 { return t.2; }"),
            ["no field `2` on `(i32, bool)`"]
        );
    }

    #[test]
    fn an_unknown_field_is_reported() {
        assert_eq!(
            check(
                "struct Foo { count: i32 }
                 fun f(x: Foo) -> i32 { return x.nope; }"
            ),
            ["no field `nope` on `Foo`"]
        );
    }

    /// `x.show` without a call is an error, and the diagnostic says what to do about it rather
    /// than only that no field of that name exists.
    #[test]
    fn naming_a_method_without_calling_it_says_so() {
        assert_eq!(
            check(
                "struct Foo {}
                 extend Foo { fun show(&self) {} }
                 fun f(x: Foo) { x.show; }"
            ),
            ["no field `show` on `Foo`; there is a method `show`"]
        );
    }

    /// A field with no `public` is private by default, reachable only from its declaring module
    /// and that module's descendants, the same rule `SymbolTable::is_visible` enforces for a
    /// path lookup. This needs `typeck_src_files` to put the access in a different module.
    #[test]
    fn a_private_field_cannot_be_read_from_another_module() {
        assert_eq!(
            crate::testing::typeck_src_files(&[
                "module math; public struct Foo { count: i32 }",
                "module app;
                 import math::Foo;
                 fun f(x: Foo) -> i32 { return x.count; }",
            ]),
            ["field `count` is private"]
        );
    }

    #[test]
    fn a_public_field_can_be_read_from_another_module() {
        assert!(
            crate::testing::typeck_src_files(&[
                "module math; public struct Foo { public count: i32 }",
                "module app;
                 import math::Foo;
                 fun f(x: Foo) -> i32 { return x.count; }",
            ])
            .is_empty()
        );
    }

    /// Re-checks `SymbolTable::is_visible`'s full rule for a field instead of a path lookup:
    /// `private` reaches the declaring module and every one of its descendants, however deep,
    /// but neither a sibling module nor the declaring module's own parent.
    #[test]
    fn field_privacy_follows_the_declaring_modules_full_descendant_chain() {
        // A grandchild of the struct's own module, not just a direct child, can still see its
        // private field.
        assert!(
            crate::testing::typeck_src_files(&[
                "module math; public struct Foo { count: i32 }",
                "module math::inner::deeper;
                 import math::Foo;
                 fun f(x: Foo) -> i32 { return x.count; }",
            ])
            .is_empty(),
            "a descendant module, however deep, should see the private field"
        );

        // A sibling module, neither an ancestor nor a descendant, cannot.
        assert_eq!(
            crate::testing::typeck_src_files(&[
                "module math; public struct Foo { count: i32 }",
                "module other;
                 import math::Foo;
                 fun f(x: Foo) -> i32 { return x.count; }",
            ]),
            ["field `count` is private"],
            "an unrelated sibling module should not see the private field"
        );

        // Nor can the declaring module's own parent see into it: visibility only ever reaches
        // downward.
        assert_eq!(
            crate::testing::typeck_src_files(&[
                "module math::inner; public struct Foo { count: i32 }",
                "module math;
                 import math::inner::Foo;
                 fun f(x: Foo) -> i32 { return x.count; }",
            ]),
            ["field `count` is private"],
            "a parent module should not see a descendant's private field"
        );
    }

    /// Combines two things a plain privacy fixture would not: the field is reached through a
    /// reference, using the same `peel_receiver` step a method call goes through, and the
    /// struct is generic, so the field's type is substituted through the receiver's arguments.
    #[test]
    fn field_privacy_is_checked_through_autoderef_and_generic_substitution() {
        assert_eq!(
            crate::testing::typeck_src_files(&[
                "module lib; public struct Wrap<T> { inner: T, public tag: T }",
                "module app;
                 import lib::Wrap;
                 fun f(x: &Wrap<i32>) -> i32 { return x.inner; }",
            ]),
            ["field `inner` is private"]
        );
        // The public field, reached the very same way, is not.
        assert!(
            crate::testing::typeck_src_files(&[
                "module lib; public struct Wrap<T> { inner: T, public tag: T }",
                "module app;
                 import lib::Wrap;
                 fun f(x: &Wrap<i32>) -> i32 { return x.tag; }",
            ])
            .is_empty()
        );
    }

    /// `field_ty`'s `owner` is the definition an access sits inside, here an `extend` block's
    /// own method rather than a free function. Privacy is judged by where the block itself was
    /// written, not by where the type it extends was declared.
    #[test]
    fn an_extend_blocks_method_reads_a_private_field_only_from_the_declaring_module() {
        // An `extend` block in the struct's own module: allowed, same as any in-module access.
        assert!(
            crate::testing::typeck_src_files(&["module math;
                 public struct Foo { count: i32 }
                 extend Foo { fun get(&self) -> i32 { return self.count; } }",])
            .is_empty()
        );

        // An `extend` block for the same type, written in a *different* module: refused exactly
        // as a free function in that module would be.
        assert_eq!(
            crate::testing::typeck_src_files(&[
                "module math; public struct Foo { count: i32 }",
                "module app;
                 import math::Foo;
                 extend Foo { fun get(&self) -> i32 { return self.count; } }",
            ]),
            ["field `count` is private"]
        );
    }

    // -----------------------------------------------------------------
    // Receivers
    // -----------------------------------------------------------------

    #[test]
    fn a_reference_receiver_reaches_a_ref_self_method() {
        assert!(
            check(
                "struct Foo {}
                 extend Foo { fun show(&self) {} }
                 fun f(x: &Foo) { x.show(); }"
            )
            .is_empty()
        );
    }

    /// Autoref takes a reference automatically when the method wants one and the receiver is a
    /// place.
    #[test]
    fn a_value_receiver_is_autoreffed_for_a_ref_self_method() {
        assert!(
            check(
                "struct Foo {}
                 extend Foo { fun show(&self) {} }
                 fun f(x: Foo) { x.show(); }"
            )
            .is_empty()
        );
    }

    #[test]
    fn a_mut_ref_receiver_reaches_a_mut_self_method() {
        assert!(
            check(
                "struct Foo {}
                 extend Foo { fun bump(&mut self) {} }
                 fun f(x: &mut Foo) { x.bump(); }"
            )
            .is_empty()
        );
    }

    #[test]
    fn a_shared_reference_cannot_reach_a_mut_self_method() {
        assert_eq!(
            check(
                "struct Foo {}
                 extend Foo { fun bump(&mut self) {} }
                 fun f(x: &Foo) { x.bump(); }"
            ),
            ["`bump` takes `&mut self`, which this receiver cannot provide"]
        );
    }

    // Whether a `&mut self` call's autoreffed receiver may be written to is `mir::checks::constck`'s
    // question, exercised by that module's own tests. `mir::lower::call` materializes this
    // autoref as an `Rvalue::Ref`, checked the same way an explicit `&mut` borrow is.

    #[test]
    fn a_value_receiver_reaches_a_by_value_self_method() {
        assert!(
            check(
                "struct Foo {}
                 extend Foo { fun consume(self) {} }
                 fun f(x: Foo) { x.consume(); }"
            )
            .is_empty()
        );
    }

    #[test]
    fn a_reference_cannot_reach_a_by_value_self_method() {
        assert_eq!(
            check(
                "struct Foo {}
                 extend Foo { fun consume(self) {} }
                 fun f(x: &Foo) { x.consume(); }"
            ),
            ["`consume` takes `self`, which this receiver cannot provide"]
        );
    }

    /// `any self` accepts a receiver of any form: by value, by reference, or by `any`.
    #[test]
    fn an_any_self_method_accepts_every_receiver() {
        assert!(
            check(
                "struct Foo {}
                 extend Foo { fun peek(any self) {} }
                 fun f(a: Foo, b: &Foo, c: &mut Foo) { a.peek(); b.peek(); c.peek(); }"
            )
            .is_empty()
        );
    }

    /// A method reached on a value produced by another call has nothing to take a reference to.
    #[test]
    fn a_temporary_receiver_cannot_be_autoreffed() {
        assert_eq!(
            check(
                "struct Foo {}
                 extend Foo { fun show(&self) {} fun make() -> Foo { return Foo {}; } }
                 fun make() -> Foo { return Foo {}; }
                 fun f() { make().show(); }"
            ),
            ["`show` takes `&self`, and this receiver is a temporary"]
        );
    }

    // -----------------------------------------------------------------
    // Arguments
    // -----------------------------------------------------------------

    #[test]
    fn too_many_arguments_are_reported() {
        assert_eq!(
            check(
                "struct Foo {}
                 extend Foo { fun take(&self, n: i32) {} }
                 fun f(x: Foo) { x.take(1, 2); }"
            ),
            ["`take` takes 1 argument but 2 were supplied"]
        );
    }

    #[test]
    fn an_argument_of_the_wrong_type_is_reported() {
        assert_eq!(
            check(
                "struct Foo {}
                 extend Foo { fun take(&self, n: i32) {} }
                 fun f(x: Foo) { x.take(true); }"
            ),
            ["mismatched types: expected `i32`, found `bool`"]
        );
    }

    #[test]
    fn a_methods_return_type_is_read_through_the_impls_arguments() {
        assert_eq!(
            check(
                "struct Wrap<T> { inner: T }
                 extend<T> Wrap<T> { fun get(&self) -> T { return self.inner; } }
                 fun f(x: Wrap<i32>) -> bool { return x.get(); }"
            ),
            ["mismatched types: expected `bool`, found `i32`"]
        );
    }

    // -----------------------------------------------------------------
    // Calls
    // -----------------------------------------------------------------

    #[test]
    fn a_call_to_a_free_function_checks_its_arguments() {
        assert_eq!(
            check(
                "fun g(n: i32) {}
                 fun f() { g(true); }"
            ),
            ["mismatched types: expected `i32`, found `bool`"]
        );
    }

    #[test]
    fn a_call_with_the_wrong_number_of_arguments_is_reported() {
        assert_eq!(
            check(
                "fun g(n: i32) {}
                 fun f() { g(); }"
            ),
            ["this call takes 1 argument but 0 were supplied"]
        );
    }

    /// Instantiating `g`'s parameter at `Bare` raises the obligation `Bare: Show`, which the
    /// per-body drain then answers.
    #[test]
    fn a_call_to_a_generic_callee_checks_the_callees_bounds() {
        assert_eq!(
            check(
                "trait Show { fun show(&self); }
                 struct Bare {}
                 fun g<T: Show>(x: T) {}
                 fun f(y: Bare) { g(y); }"
            ),
            ["the trait bound `Bare: Show` is not satisfied"]
        );
    }

    #[test]
    fn a_call_to_a_generic_callee_whose_bound_holds_checks() {
        assert!(
            check(
                "trait Show { fun show(&self); }
                 struct Foo {}
                 extend Foo with Show { fun show(&self) {} }
                 fun g<T: Show>(x: T) {}
                 fun f(y: Foo) { g(y); }"
            )
            .is_empty()
        );
    }

    /// A receiver whose type nothing pins down cannot be resolved, and unlike a bound it is not
    /// something a later pass could answer.
    #[test]
    fn an_unresolved_receiver_is_reported() {
        assert_eq!(
            check(
                "struct Foo {}
                 extend Foo { fun show(&self) {} }
                 fun f() { let x = 1; x.show(); }"
            ),
            [
                "type annotations needed: the type of the value `show` is reached on is still \
                 unknown"
            ]
        );
    }

    // -----------------------------------------------------------------
    // `any self`, generic methods, and index-through-a-generic-trait
    // -----------------------------------------------------------------

    /// `any self` accepts every receiver shape: by value, by reference, and by `any`.
    #[test]
    fn any_self_accepts_every_receiver_shape() {
        assert!(
            check(
                "struct Foo {}
                 extend Foo { fun show(any self) {} }
                 fun f(a: Foo, b: &Foo, c: &mut Foo, d: any Foo) {
                     a.show();
                     b.show();
                     c.show();
                     d.show();
                 }"
            )
            .is_empty()
        );
    }

    /// A method's own generic parameters are instantiated fresh per call, independent of the
    /// receiver's type.
    #[test]
    fn a_methods_own_generic_parameter_is_inferred_from_its_argument() {
        assert!(
            check(
                "struct Box_ {}
                 extend Box_ { fun identity<T>(&self, x: T) -> T { return x; } }
                 fun f(b: Box_, n: i32) -> i32 { return b.identity(n); }"
            )
            .is_empty()
        );
        assert_eq!(
            check(
                "struct Box_ {}
                 extend Box_ { fun identity<T>(&self, x: T) -> T { return x; } }
                 fun f(b: Box_, n: i32) -> bool { return b.identity(n); }"
            ),
            ["mismatched types: expected `bool`, found `i32`"]
        );
    }

    /// `Index<K, V>` read through a generic `extend` block: `V` is recovered from the receiver's
    /// own type arguments, not left as the block's bare parameter.
    #[test]
    fn indexing_a_generic_type_reads_v_through_the_receivers_own_arguments() {
        assert!(
            check(
                "module core::ops;

                 public trait Index<K, V> { fun index(&self, key: K) -> &V; }

                 struct Map<V> { value: V }

                 extend<V> Map<V> with Index<i32, V> {
                     fun index(&self, key: i32) -> &V { return &self.value; }
                 }

                 fun f(m: Map<bool>) -> &bool { return m[0]; }"
            )
            .is_empty()
        );
        assert_eq!(
            check(
                "module core::ops;

                 public trait Index<K, V> { fun index(&self, key: K) -> &V; }

                 struct Map<V> { value: V }

                 extend<V> Map<V> with Index<i32, V> {
                     fun index(&self, key: i32) -> &V { return &self.value; }
                 }

                 fun f(m: Map<bool>) -> &i32 { return m[0]; }"
            ),
            ["mismatched types: expected `&i32`, found `&bool`"]
        );
    }

    // -----------------------------------------------------------------
    // Resolved-call recording
    // -----------------------------------------------------------------

    #[test]
    fn a_free_call_records_its_callee_and_instantiation() {
        let hir = resolve_src(
            "fun largest<T>(a: T, b: T) -> T { return a; }
             fun main() -> i32 { return largest(1, 2); }",
        );
        DiagCtx::clear();
        let checked = crate::typeck::check(&hir);

        let largest = named(&collected(&hir), "largest");
        let main = named(&collected(&hir), "main");
        let (_, call_id) = find_return(&hir, main);

        let resolved = checked
            .types
            .call(call_id)
            .expect("a call to a resolved free function is recorded");
        assert_eq!(resolved.def, largest);
        assert_eq!(resolved.args.len(), 1);
        assert!(matches!(
            checked.tcx.kind(resolved.args[0]),
            TyKind::Primitive(_)
        ));
    }

    #[test]
    fn a_method_call_records_its_callee() {
        let hir = resolve_src(
            "struct Foo {}
             extend Foo { fun show(&self) -> i32 { return 0; } }
             fun main() -> i32 { let f = Foo {}; return f.show(); }",
        );
        DiagCtx::clear();
        let checked = crate::typeck::check(&hir);

        let main = named(&collected(&hir), "main");
        let show = first_extend_method(&hir);
        let (_, call_id) = find_return(&hir, main);

        let resolved = checked
            .types
            .call(call_id)
            .expect("a method call is recorded");
        assert_eq!(resolved.def, show);
        assert!(resolved.args.is_empty());
    }

    // -----------------------------------------------------------------
    // `any`-coercion (README section 7)
    // -----------------------------------------------------------------

    #[test]
    fn an_any_parameter_accepts_a_plain_owned_argument() {
        use crate::testing::typeck_accepts;

        typeck_accepts(
            "fun min(x: any i32, y: any i32) -> any i32 {
                 return if x < y { x } else { y };
             }
             fun f() {
                 let a = 1;
                 let b = 2;
                 min(a, b);
             }",
        );
    }

    #[test]
    fn an_any_return_accepts_a_plain_owned_return_expression() {
        use crate::testing::typeck_accepts;

        typeck_accepts("fun make(x: any i32) -> any i32 { return x; }");
    }

    #[test]
    fn an_any_parameter_still_rejects_a_mismatched_base_type() {
        use crate::testing::typeck_rejects;

        typeck_rejects(
            "fun min(x: any i32, y: any i32) -> any i32 {
                 return if x < y { x } else { y };
             }
             fun f() {
                 let b = 2;
                 min(true, b);
             }",
            "mismatched types",
        );
    }
}
