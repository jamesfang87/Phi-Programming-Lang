use crate::ast::interner::Interner;
use crate::ast::{Path, Symbol};
use crate::hir::DefId;
use crate::nameres::NameResolver;
use crate::nameres::results::{PrimTy, TypeRes};
use crate::nameres::symbol_table::report_not_found;

impl<'hir> NameResolver<'hir> {
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
        report_not_found(name);
        TypeRes::Err
    }

    fn prim_ty(symbol: Symbol) -> Option<PrimTy> {
        match Interner::resolve(symbol) {
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
