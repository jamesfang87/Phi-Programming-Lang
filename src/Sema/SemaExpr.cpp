#include "AST/Decl.hpp"
#include "Sema/Sema.hpp"

#include <cassert>
#include <cstddef>
#include <print>

namespace phi {

// Literal expression resolution
bool Sema::visit(IntLiteral &expr) {
  expr.setTy(Type(Type::Primitive::i64));
  return true;
}

bool Sema::visit(FloatLiteral &expr) {
  expr.setTy(Type(Type::Primitive::f64));
  return true;
}

bool Sema::visit(StrLiteral &expr) {
  expr.setTy(Type(Type::Primitive::str));
  return true;
}

bool Sema::visit(CharLiteral &expr) {
  expr.setTy(Type(Type::Primitive::character));
  return true;
}

bool Sema::visit(BoolLiteral &expr) {
  expr.setTy(Type(Type::Primitive::boolean));
  return true;
}

/**
 * Resolves a variable reference expression.
 *
 * Validates:
 * - Identifier exists in symbol table
 * - Not attempting to use function as value
 */
bool Sema::visit(DeclRefExpr &expr) {
  Decl *decl = symbolTable.lookup(expr);

  if (!decl) {
    std::println("error: undeclared identifier '{}'", expr.getID());
    return false;
  }

  expr.setDecl(decl);
  expr.setTy(decl->getTy());

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
bool Sema::visit(FunCallExpr &expr) {
  FunDecl *fun = symbolTable.lookup(expr);

  // Validate argument count
  if (expr.getArgs().size() != fun->getParams().size()) {
    std::println("error: parameter list length mismatch");
    return false;
  }

  // Resolve arguments and validate types
  for (size_t i = 0; i < expr.getArgs().size(); i++) {
    Expr *arg = expr.getArgs()[i].get();
    if (!arg->accept(*this)) {
      std::println("error: could not resolve argument");
      return false;
    }

    ParamDecl *param = fun->getParams().at(i).get();
    if (arg->getTy() != param->getTy()) {
      std::println("error: argument type mismatch");
      return false;
    }
  }

  expr.setTy(fun->getReturnTy());
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
bool Sema::visit(RangeLiteral &expr) {
  // Resolve start and end expressions
  if (!expr.getStart().accept(*this) || !expr.getEnd().accept(*this)) {
    return false;
  }

  const Type startTy = expr.getStart().getTy();
  const Type endTy = expr.getEnd().getTy();

  // Validate integer types
  if (!startTy.isPrimitive() || !isIntTy(startTy)) {
    std::println("Range start must be an integer type (i8, i16, i32, i64, u8, "
                 "u16, u32, u64)");
    return false;
  }

  if (!endTy.isPrimitive() || !isIntTy(endTy)) {
    std::println("Range end must be an integer type (i8, i16, i32, i64, u8, "
                 "u16, u32, u64)");
    return false;
  }

  // Set types
  expr.getStart().setTy(Type(Type::Primitive::i64));
  expr.getEnd().setTy(Type(Type::Primitive::i64));
  expr.setTy(Type(Type::Primitive::range));
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
bool Sema::visit(BinaryOp &expr) {
  // Resolve operands
  if (!expr.getLhs().accept(*this) || !expr.getRhs().accept(*this)) {
    return false;
  }

  const Type lhs_type = expr.getLhs().getTy();
  const Type rhs_type = expr.getRhs().getTy();
  const TokenKind op = expr.getOp();

  // Only primitive types supported
  if (!lhs_type.isPrimitive() || !rhs_type.isPrimitive()) {
    std::println("Binary operations not supported on custom types");
    return false;
  }

  // Operation-specific validation
  switch (op) {
  // Arithmetic operations
  case TokenKind::tokPlus:
  case TokenKind::tokMinus:
  case TokenKind::tokStar:
  case TokenKind::tokSlash:
  case TokenKind::tokPercent: {
    if (!isNumTy(lhs_type) || !isNumTy(rhs_type)) {
      std::println("Arithmetic operations require numeric types");
      return false;
    }
    if (op == TokenKind::tokPercent &&
        (!isIntTy(lhs_type) || !isIntTy(rhs_type))) {
      std::println("Modulo operation requires integer types");
      return false;
    }
    // Apply type promotion
    expr.setTy(promoteNumTys(lhs_type, rhs_type));
    break;
  }

  case TokenKind::tokEquals: {
    if (!expr.getLhs().isAssignable()) {
      std::println("Left-hand side of assignment must be assignable");
      return false;
    }

    if (lhs_type != rhs_type) {
      std::println("Assignment requires same types");
      return false;
    }

    expr.setTy(lhs_type);
    break;
  }

  // Comparison operations
  case TokenKind::tokDoubleEquals:
  case TokenKind::tokBangEquals:
    if (lhs_type != rhs_type) {
      std::println("Equality comparison requires same types");
      return false;
    }
    expr.setTy(Type(Type::Primitive::boolean));
    break;

  case TokenKind::tokLeftCaret:
  case TokenKind::tokLessEqual:
  case TokenKind::tokRightCaret:
  case TokenKind::tokGreaterEqual:
    if (!isNumTy(lhs_type) || !isNumTy(rhs_type) || lhs_type != rhs_type) {
      std::println("Ordering comparisons require same numeric types");
      return false;
    }
    expr.setTy(Type(Type::Primitive::boolean));
    break;

  // Logical operations
  case TokenKind::tokDoubleAmp:
  case TokenKind::tokDoublePipe:
    if (lhs_type.primitive_type() != Type::Primitive::boolean ||
        rhs_type.primitive_type() != Type::Primitive::boolean) {
      std::println("Logical operations require boolean types");
      return false;
    }
    expr.setTy(Type(Type::Primitive::boolean));
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
Type Sema::promoteNumTys(const Type &lhs, const Type &rhs) {
  if (lhs == rhs)
    return lhs;

  if (isIntTy(lhs) && isFloat(rhs)) {
    return rhs;
  }
  if (isIntTy(rhs) && isFloat(lhs)) {
    return lhs;
  }

  return lhs; // Default to left type
}
/**
 * Resolves a unary operation expression.
 *
 * Validates:
 * - Operand resolves successfully
 * - Operation is supported for operand type
 */
bool Sema::visit(UnaryOp &expr) {
  // Resolve operand
  if (!expr.getOperand().accept(*this)) {
    return false;
  }

  const Type operand_type = expr.getOperand().getTy();
  const TokenKind op = expr.getOp();

  // Only primitive types supported
  if (!operand_type.isPrimitive()) {
    std::println("Unary operations not supported on custom types");
    return false;
  }

  // Operation-specific validation
  switch (op) {
  // Arithmetic negation
  case TokenKind::tokMinus:
    if (!isNumTy(operand_type)) {
      std::println("Arithmetic negation requires numeric type");
      return false;
    }
    expr.setTy(operand_type);
    break;

  // Logical NOT
  case TokenKind::tokBang:
    if (operand_type.primitive_type() != Type::Primitive::boolean) {
      std::println("Logical NOT requires boolean type");
      return false;
    }
    expr.setTy(Type(Type::Primitive::boolean));
    break;

  // Increment/Decrement
  case TokenKind::tokDoublePlus:
  case TokenKind::tokDoubleMinus:
    if (!isIntTy(operand_type)) {
      std::println("Increment/decrement requires integer type");
      return false;
    }
    expr.setTy(operand_type);
    break;

  default:
    std::println("Unsupported unary operation");
    return false;
  }

  return true;
}

} // namespace phi
