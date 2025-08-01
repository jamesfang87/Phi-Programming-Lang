#include "CodeGen/CodeGen.hpp"
#include "AST/Expr.hpp"

llvm::Type* phi::CodeGen::gen_type(const phi::Type& type) {
    switch (type.primitive_type()) {
        case Type::Primitive::null: return llvm::Type::getVoidTy(context);
        case Type::Primitive::boolean: return llvm::Type::getInt1Ty(context);
        case Type::Primitive::i8: return llvm::Type::getInt8Ty(context);
        case Type::Primitive::i16: return llvm::Type::getInt16Ty(context);
        case Type::Primitive::i32: return llvm::Type::getInt32Ty(context);
        case Type::Primitive::i64: return llvm::Type::getInt64Ty(context);
        case Type::Primitive::f32: return llvm::Type::getFloatTy(context);
        case Type::Primitive::f64: return llvm::Type::getDoubleTy(context);
        default: throw std::runtime_error("Unsupported type");
    }
}

void phi::CodeGen::visit(phi::IntLiteral& expr) {
    llvm::ConstantFP::get(builder.getInt64Ty(), expr.get_value());
}
