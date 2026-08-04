use crate::ast::interner::Interner;
use crate::ast::{Path, Symbol};
use crate::hir::{DefId, HirId, TyKind};
use crate::nameres::NameResolver;
use crate::nameres::results::{PrimTy, TypeRes};
use crate::nameres::symbol_table::SymbolTable;

impl<'hir> NameResolver<'hir> {
    pub fn resolve_ty(&mut self, ty_id: HirId) {
        let ty = self.hir.ty(ty_id);

        match &ty.kind {
            TyKind::Path { path, args } => {
                let res = self.resolve_ty_path(ty_id.owner, path);
                self.results.record_type(ty_id, res);
                for &arg in args {
                    self.resolve_ty(arg);
                }
            }
            TyKind::Ref { base, .. } | TyKind::Any(base) => {
                self.resolve_ty(*base);
            }
            TyKind::Tuple(elems) => {
                for &elem in elems {
                    self.resolve_ty(elem);
                }
            }
            TyKind::Array { elem, len } => {
                self.resolve_ty(*elem);
                if let Some(len) = len {
                    self.resolve_expr(*len);
                }
            }
            TyKind::Function { params, ret } => {
                for &param in params {
                    self.resolve_ty(param);
                }
                if let Some(ret) = ret {
                    self.resolve_ty(*ret);
                }
            }
            // `Self` resolves no path, so there is nothing to record against this node. What it
            // stands for is a property of the enclosing definition rather than of the annotation,
            // and is recorded once per definition by `resolve_struct`, `resolve_enums`,
            // `resolve_trait`, and `resolve_extend`. Type lowering reads it back the same way
            // this resolver would, by walking up from the owner -- see `Typeck::self_ty`.
            TyKind::SelfType => {}
            TyKind::Dyn(path) => {
                let res = self.resolve_ty_path(ty_id.owner, path);
                self.results.record_type(ty_id, res);
            }
            TyKind::Error => {}
        }
    }

    /// Resolves a path used as a type: a single-segment path can name a primitive (`i32`,
    /// `bool`, ...), which never gets a `DefId`, so those are checked before falling back to the
    /// type namespace (structs/enums/traits), searched relative to the module `owner_id` sits in.
    pub fn resolve_ty_path(&mut self, owner_id: DefId, path: &Path) -> TypeRes {
        if let [name] = path.segments.as_slice() {
            if let Some(prim) = Self::prim_ty(name.text) {
                return TypeRes::PrimTy(prim);
            }
            if let Some(res) = self.generic_ty(owner_id, name.text) {
                return res;
            }
        }

        if let Some(def_id) = self.symbol_tab.lookup_type_path(owner_id, path) {
            return TypeRes::Def(def_id);
        }

        let name = *path
            .segments
            .last()
            .expect("a path always has at least one segment");
        SymbolTable::report_not_found(name);
        TypeRes::Err
    }

    fn prim_ty(symbol: Symbol) -> Option<PrimTy> {
        match Interner::resolve(symbol).as_str() {
            "i8" => Some(PrimTy::I8),
            "i16" => Some(PrimTy::I16),
            "i32" => Some(PrimTy::I32),
            "i64" => Some(PrimTy::I64),
            "u8" => Some(PrimTy::U8),
            "u16" => Some(PrimTy::U16),
            "u32" => Some(PrimTy::U32),
            "u64" => Some(PrimTy::U64),
            "f32" => Some(PrimTy::F32),
            "f64" => Some(PrimTy::F64),
            "bool" => Some(PrimTy::Bool),
            "char" => Some(PrimTy::Char),
            _ => None,
        }
    }
}
