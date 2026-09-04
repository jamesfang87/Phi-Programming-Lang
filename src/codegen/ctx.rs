use std::cell::RefCell;
use std::collections::HashMap;

use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{FunctionValue, PointerValue};

use crate::ast::Symbol;
use crate::codegen::intrinsic::{self, Libc};
use crate::hir::DefId;
use crate::typeck::ty::Ty;

pub struct CodegenCtx<'ctx> {
    pub llvm: &'ctx Context,
    pub module: Module<'ctx>,
    pub builder: Builder<'ctx>,
    pub functions: HashMap<String, FunctionValue<'ctx>>,
    pub types: RefCell<HashMap<Ty, BasicTypeEnum<'ctx>>>,
    pub strings: RefCell<HashMap<Symbol, PointerValue<'ctx>>>,
    pub libc: Libc<'ctx>,
    pub vtables: RefCell<HashMap<(Ty, DefId), PointerValue<'ctx>>>,
}

impl<'ctx> CodegenCtx<'ctx> {
    pub fn new(llvm: &'ctx Context, module_name: &str) -> Self {
        let module = llvm.create_module(module_name);
        let libc = intrinsic::declare_libc(llvm, &module);
        CodegenCtx {
            llvm,
            module,
            builder: llvm.create_builder(),
            functions: HashMap::new(),
            types: RefCell::new(HashMap::new()),
            strings: RefCell::new(HashMap::new()),
            libc,
            vtables: RefCell::new(HashMap::new()),
        }
    }
}
