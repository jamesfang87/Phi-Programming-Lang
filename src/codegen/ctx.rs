use std::cell::RefCell;
use std::collections::HashMap;

use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{FunctionValue, PointerValue};

use crate::ast::Symbol;
use crate::codegen::intrinsic::{self, Libc};
use crate::hir::{DefId, Hir};
use crate::mir::Instance;
use crate::session::Session;
use crate::typeck::ty::Ty;
use crate::typeck::ty::ctx::TyCtx;

use super::mangle::mangle;

pub struct CodegenCtx<'ctx> {
    pub session: &'ctx Session,
    /// The HIR, the sole source of definition names for symbol mangling and of the core
    /// library's lang items. Codegen only reaches back into it for those, never for MIR-level
    /// facts, which lowering has already recorded.
    pub hir: &'ctx Hir,
    pub llvm: &'ctx Context,
    pub module: Module<'ctx>,
    pub builder: Builder<'ctx>,
    pub functions: HashMap<String, FunctionValue<'ctx>>,
    pub types: RefCell<HashMap<Ty, BasicTypeEnum<'ctx>>>,
    pub strings: RefCell<HashMap<Symbol, PointerValue<'ctx>>>,
    pub libc: Libc<'ctx>,
    pub vtables: RefCell<HashMap<(Ty, DefId), PointerValue<'ctx>>>,
    pub drop_glues: RefCell<HashMap<Ty, FunctionValue<'ctx>>>,
    pub fun_thunks: RefCell<HashMap<String, FunctionValue<'ctx>>>,
}

impl<'ctx> CodegenCtx<'ctx> {
    pub fn new(
        session: &'ctx Session,
        hir: &'ctx Hir,
        llvm: &'ctx Context,
        module_name: &str,
    ) -> Self {
        let module = llvm.create_module(module_name);
        let libc = intrinsic::declare_libc(llvm, &module);
        CodegenCtx {
            session,
            hir,
            llvm,
            module,
            builder: llvm.create_builder(),
            functions: HashMap::new(),
            types: RefCell::new(HashMap::new()),
            strings: RefCell::new(HashMap::new()),
            libc,
            vtables: RefCell::new(HashMap::new()),
            drop_glues: RefCell::new(HashMap::new()),
            fun_thunks: RefCell::new(HashMap::new()),
        }
    }

    /// The symbol name of `instance`, for the function it will be emitted as.
    pub fn mangle(&self, tcx: &TyCtx, instance: &Instance) -> String {
        mangle(self.hir, self.session, tcx, instance)
    }
}
