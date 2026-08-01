use crate::ast::interner::Interner;
use crate::ast::{Path, Symbol};
use crate::hir::{DefId, HirId, LocalId, Node, TyKind};
use crate::nameres::resolve_results::{PrimTy, Res};
use crate::nameres::symbol_table::SymbolTable;
use crate::nameres::NameResolver;

impl<'hir> NameResolver<'hir> {
    pub fn resolve_ty(&mut self, owner_id: DefId, ty_id: LocalId) {
        let hir = self.hir;

        let hir_id = HirId {
            owner: owner_id,
            local_id: ty_id,
        };
        let Node::Ty(ty) = hir.node(hir_id) else {
            unreachable!("Expected a ty's local id to name a ty");
        };

        match &ty.kind {
            TyKind::Base { path, args } => {
                let res = self.resolve_ty_path(owner_id, path);
                self.results.add(hir_id, res);
                for &arg in args {
                    self.resolve_ty(owner_id, arg);
                }
            }
            TyKind::Ref { base, .. } | TyKind::Any(base) => {
                self.resolve_ty(owner_id, *base);
            }
            TyKind::Tuple(elems) => {
                for &elem in elems {
                    self.resolve_ty(owner_id, elem);
                }
            }
            TyKind::Array { elem, len } => {
                self.resolve_ty(owner_id, *elem);
                if let Some(len) = len {
                    self.resolve_expr(owner_id, *len);
                }
            }
            TyKind::Function { params, ret } => {
                for &param in params {
                    self.resolve_ty(owner_id, param);
                }
                if let Some(ret) = ret {
                    self.resolve_ty(owner_id, *ret);
                }
            }
            TyKind::SelfType => {
                let res = self.self_ty(owner_id).unwrap_or(Res::Err);
                self.results.add(hir_id, res);
            }
            TyKind::Dyn(path) => {
                let res = self.resolve_ty_path(owner_id, path);
                self.results.add(hir_id, res);
            }
            TyKind::Error => {}
        }
    }

    /// Resolves a path used as a type: a single-segment path can name a primitive (`i32`,
    /// `bool`, ...), which never gets a `DefId`, so those are checked before falling back to the
    /// type namespace (structs/enums/traits), searched relative to the module `owner_id` sits in.
    pub fn resolve_ty_path(&mut self, owner_id: DefId, path: &Path) -> Res {
        if let [name] = path.segments.as_slice() {
            if let Some(prim) = Self::prim_ty(name.text) {
                return Res::PrimTy(prim);
            }
        }

        if let Some(def_id) = self.symbol_tab.lookup_type_path(owner_id, path) {
            return Res::Def(def_id);
        }

        let name = *path
            .segments
            .last()
            .expect("a path always has at least one segment");
        SymbolTable::report_not_found(name);
        Res::Err
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
