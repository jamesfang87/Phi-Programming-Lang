#include "AST/Decl.hpp"
#include "AST/Expr.hpp"
#include "Sema/Sema.hpp"

#include <llvm/Support/Casting.h>

#include <cassert>
#include <cstddef>
#include <print>

namespace phi {

// Literal expression resolution
bool Sema::visit(IntLiteral &Expression) {
  Expression.setType(Type(Type::Primitive::i64));
  return true;
}

bool Sema::visit(FloatLiteral &Expression) {
  Expression.setType(Type(Type::Primitive::f64));
  return true;
}

bool Sema::visit(StrLiteral &Expression) {
  Expression.setType(Type(Type::Primitive::str));
  return true;
}

bool Sema::visit(CharLiteral &Expression) {
  Expression.setType(Type(Type::Primitive::character));
  return true;
}

bool Sema::visit(BoolLiteral &Expression) {
  Expression.setType(Type(Type::Primitive::boolean));
  return true;
}

/**
 * Resolves a variable reference expression.
 *
 * Validates:
 * - Identifier exists in symbol table
 * - Not attempting to use function as value
 */
bool Sema::visit(DeclRefExpr &Expression) {
  Decl *DeclPtr = SymbolTab.lookup(Expression);

  if (!DeclPtr) {
    std::println("error: undeclared identifier '{}'", Expression.getId());
    return false;
  }

  Expression.setDecl(DeclPtr);
  Expression.setType(DeclPtr->getType());

  return true;
}

/**
 * Resolves a function call expression.
 *
 * Validates:
 * - Callee is a function
 * - Argument count matches parameter count
 * - Argument types match parameter types
 */
bool Sema::visit(FunCallExpr &Expression) {
  FunDecl *FunPtr = SymbolTab.lookup(Expression);

  // Validate argument count
  if (Expression.getArgs().size() != FunPtr->getParams().size()) {
    std::println("error: parameter list length mismatch");
    return false;
  }

  // Resolve arguments and validate types
  for (size_t i = 0; i < Expression.getArgs().size(); i++) {
    Expr *Arg = Expression.getArgs()[i].get();
    if (!Arg->accept(*this)) {
      std::println("error: could not resolve argument");
      return false;
    }

    ParamDecl *Param = FunPtr->getParams().at(i).get();
    if (Arg->getType() != Param->getType()) {
      std::println("error: argument type mismatch");
      return false;
    }
  }

  Expression.setType(FunPtr->getReturnTy());
  return true;
}

/**
 * Resolves a range literal expression.
 *
 * Validates:
 * - Start and end expressions resolve
 * - Both are integer types
 * - Both have same integer type
 */
bool Sema::visit(RangeLiteral &Expression) {
  // Resolve start and end expressions
  if (!Expression.getStart().accept(*this) ||
      !Expression.getEnd().accept(*this)) {
    return false;
  }

  const Type StartTy = Expression.getStart().getType();
  const Type EndTy = Expression.getEnd().getType();

  // Validate integer types
  if (!StartTy.isPrimitive() || !isIntTy(StartTy)) {
    std::println("Range start must be an integer type (i8, i16, i32, i64, u8, "
                 "u16, u32, u64)");
    return false;
  }

  if (!EndTy.isPrimitive() || !isIntTy(EndTy)) {
    std::println("Range end must be an integer type (i8, i16, i32, i64, u8, "
                 "u16, u32, u64)");
    return false;
  }

  // Set types
  Expression.getStart().setType(Type(Type::Primitive::i64));
  Expression.getEnd().setType(Type(Type::Primitive::i64));
  Expression.setType(Type(Type::Primitive::range));
  return true;
}

/**
 * Resolves a binary operation expression.
 *
 * Validates:
 * - Both operands resolve successfully
 * - Operation is supported for operand types
 * - Type promotion rules are followed
 */
bool Sema::visit(BinaryOp &Expression) {
  // Resolve operands
  if (!Expression.getLhs().accept(*this) ||
      !Expression.getRhs().accept(*this)) {
    return false;
  }

  const Type LhsTy = Expression.getLhs().getType();
  const Type RhsTy = Expression.getRhs().getType();
  const TokenKind Op = Expression.getOp();

  // Only primitive types supported
  if (!LhsTy.isPrimitive() || !RhsTy.isPrimitive()) {
    std::println("Binary operations not supported on custom types");
    return false;
  }

  // Operation-specific validation
  switch (Op) {
  // Arithmetic operations
  case TokenKind::PlusKind:
  case TokenKind::MinusKind:
  case TokenKind::StarKind:
  case TokenKind::SlashKind:
  case TokenKind::PercentKind: {
    if (!isNumTy(LhsTy) || !isNumTy(RhsTy)) {
      std::println("Arithmetic operations require numeric types");
      return false;
    }
    if (Op == TokenKind::PercentKind && (!isIntTy(LhsTy) || !isIntTy(RhsTy))) {
      std::println("Modulo operation requires integer types");
      return false;
    }
    // Apply type promotion
    Expression.setType(promoteNumTys(LhsTy, RhsTy));
    break;
  }

  case TokenKind::EqualsKind: {
    if (!Expression.getLhs().isAssignable()) {
      std::println("Left-hand side of assignment must be assignable");
      return false;
    }

    if (LhsTy != RhsTy) {
      std::println("Assignment requires same types");
      return false;
    }

    auto DeclRef = llvm::dyn_cast<DeclRefExpr>(&Expression.getLhs());
    if (DeclRef->getDecl()->isConst()) {
      std::println("Error: attempt to reassign value of constant variable");
      return false;
    }

    Expression.setType(LhsTy);
    break;
  }

  // Comparison operations
  case TokenKind::DoubleEqualsKind:
  case TokenKind::BangEqualsKind:
    if (LhsTy != RhsTy) {
      std::println("Equality comparison requires same types");
      return false;
    }
    Expression.setType(Type(Type::Primitive::boolean));
    break;

  case TokenKind::OpenCaretKind:
  case TokenKind::LessEqualKind:
  case TokenKind::CloseCaretKind:
  case TokenKind::GreaterEqualKind:
    if (!isNumTy(LhsTy) || !isNumTy(RhsTy) || LhsTy != RhsTy) {
      std::println("Ordering comparisons require same numeric types");
      return false;
    }
    Expression.setType(Type(Type::Primitive::boolean));
    break;

  // Logical operations
  case TokenKind::DoubleAmpKind:
  case TokenKind::DoublePipeKind:
    if (LhsTy.getPrimitiveType() != Type::Primitive::boolean ||
        RhsTy.getPrimitiveType() != Type::Primitive::boolean) {
      std::println("Logical operations require boolean types");
      return false;
    }
    Expression.setType(Type(Type::Primitive::boolean));
    break;

  default:
    std::println("Unsupported binary operation");
    return false;
  }

  return true;
}

/**
 * Promotes numeric types for binary operations.
 *
 * Current rules:
 * - Same type: return type
 * - Integer + float: promote to float
 * - Mixed integers: promote to larger type (placeholder)
 */
Type Sema::promoteNumTys(const Type &Lhs, const Type &Rhs) {
  if (Lhs == Rhs)
    return Lhs;

  if (isIntTy(Lhs) && isFloat(Rhs)) {
    return Rhs;
  }
  if (isIntTy(Rhs) && isFloat(Lhs)) {
    return Lhs;
  }

  return Lhs; // Default to left type
}
/**
 * Resolves a unary operation expression.
 *
 * Validates:
 * - Operand resolves successfully
 * - Operation is supported for operand type
 */
bool Sema::visit(UnaryOp &Expression) {
  // Resolve operand
  if (!Expression.getOperand().accept(*this)) {
    return false;
  }

  const Type OperandTy = Expression.getOperand().getType();
  const TokenKind Op = Expression.getOp();

  // Only primitive types supported
  if (!OperandTy.isPrimitive()) {
    std::println("Unary operations not supported on custom types");
    return false;
  }

  // Operation-specific validation
  switch (Op) {
  // Arithmetic negation
  case TokenKind::MinusKind:
    if (!isNumTy(OperandTy)) {
      std::println("Arithmetic negation requires numeric type");
      return false;
    }
    Expression.setType(OperandTy);
    break;

  // Logical NOT
  case TokenKind::BangKind:
    if (OperandTy.getPrimitiveType() != Type::Primitive::boolean) {
      std::println("Logical NOT requires boolean type");
      return false;
    }
    Expression.setType(Type(Type::Primitive::boolean));
    break;

  // Increment/Decrement
  case TokenKind::DoublePlusKind:
  case TokenKind::DoubleMinusKind:
    if (!isIntTy(OperandTy)) {
      std::println("Increment/decrement requires integer type");
      return false;
    }
    Expression.setType(OperandTy);
    break;

  default:
    std::println("Unsupported unary operation");
    return false;
  }

  return true;
}

} // namespace phi
