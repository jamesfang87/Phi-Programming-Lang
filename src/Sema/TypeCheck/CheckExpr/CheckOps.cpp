#include "Sema/TypeChecker.hpp"

#include <cassert>
#include <format>

#include <llvm/Support/ErrorHandling.h>

#include "AST/Type.hpp"
#include "Diagnostics/DiagnosticBuilder.hpp"
#include "Lexer/TokenKind.hpp"

using namespace phi;

bool TypeChecker::visit(BinaryOp &E) {
  assert(E.getLhs().hasType());
  assert(E.getRhs().hasType());

  bool Success = visit(E.getLhs());
  Success = visit(E.getRhs()) && Success;

  Type Lhs = E.getLhs().getType();
  Type Rhs = E.getRhs().getType();

  switch (E.getOp()) {
  // Comparison operators and numeric operators
  case TokenKind::Equals: // Assignment operator
  case TokenKind::PlusEquals:
  case TokenKind::SubEquals:
  case TokenKind::MulEqual:
  case TokenKind::DivEquals:
  case TokenKind::ModEquals:
  case TokenKind::DoubleEquals:
  case TokenKind::BangEquals:
  case TokenKind::OpenCaret:
  case TokenKind::LessEqual:
  case TokenKind::CloseCaret:
  case TokenKind::GreaterEqual:
  case TokenKind::Plus:
  case TokenKind::Minus:
  case TokenKind::Star:
  case TokenKind::Slash:
  case TokenKind::Percent: {
    bool isAssignmentOp =
        (E.getOp() == TokenKind::Equals || E.getOp() == TokenKind::PlusEquals ||
         E.getOp() == TokenKind::SubEquals ||
         E.getOp() == TokenKind::MulEqual ||
         E.getOp() == TokenKind::DivEquals ||
         E.getOp() == TokenKind::ModEquals);

    // Check LHS assignability for assignment ops
    if (isAssignmentOp && !E.getLhs().isAssignable()) {
      error("Left-hand side of assignment is not assignable")
          .with_primary_label(E.getLhs().getLocation(), "Cannot assign here")
          .emit(*Diag);
      Success = false;
    }

    // Numeric checks for arithmetic ops
    bool isNumericOp =
        (E.getOp() == TokenKind::OpenCaret ||
         E.getOp() == TokenKind::LessEqual ||
         E.getOp() == TokenKind::CloseCaret ||
         E.getOp() == TokenKind::GreaterEqual || E.getOp() == TokenKind::Plus ||
         E.getOp() == TokenKind::Minus || E.getOp() == TokenKind::Star ||
         E.getOp() == TokenKind::Slash || E.getOp() == TokenKind::Percent ||
         E.getOp() == TokenKind::PlusEquals ||
         E.getOp() == TokenKind::SubEquals ||
         E.getOp() == TokenKind::MulEqual ||
         E.getOp() == TokenKind::DivEquals ||
         E.getOp() == TokenKind::ModEquals);

    if (isNumericOp) {
      if (!Lhs.isInteger() && !Lhs.isFloat()) {
        error(std::format("Operation `{}` not defined for non-numeric types",
                          tyToStr(E.getOp())))
            .with_primary_label(E.getLhs().getLocation(),
                                "Expected numeric type")
            .emit(*Diag);
        Success = false;
      }

      if (!Rhs.isInteger() && !Rhs.isFloat()) {
        error(std::format("Operation `{}` not defined for non-numeric types",
                          tyToStr(E.getOp())))
            .with_primary_label(E.getRhs().getLocation(),
                                "Expected numeric type")
            .emit(*Diag);
        Success = false;
      }
    }

    // All these ops require LHS and RHS to have the same type
    if (Lhs != Rhs) {
      error("Operation must be between expressions of the same type")
          .with_primary_label(E.getLhs().getLocation(), Lhs.toString())
          .with_secondary_label(E.getRhs().getLocation(), Rhs.toString())
          .emit(*Diag);
      Success = false;
    }

    return Success;
  }

  // Logical boolean operators
  case TokenKind::DoublePipe:
  case TokenKind::DoubleAmp: {
    if (!Lhs.isPrimitive() || Lhs.asPrimitive() != PrimitiveKind::Bool) {
      error(std::format("Operation `{}` can only be applied to bool type",
                        tyToStr(E.getOp())))
          .with_primary_label(E.getLhs().getLocation(), "Expected type `bool`")
          .emit(*Diag);
      Success = false;
    }

    if (!Rhs.isPrimitive() || Rhs.asPrimitive() != PrimitiveKind::Bool) {
      error(std::format("Operation `{}` can only be applied to bool type",
                        tyToStr(E.getOp())))
          .with_primary_label(E.getRhs().getLocation(), "Expected type `bool`")
          .emit(*Diag);
      Success = false;
    }

    return Success;
  }

  default:
    break;
  }

  return Success;
}

bool TypeChecker::visit(UnaryOp &E) {
  assert(E.getOperand().hasType());
  bool Success = visit(E.getOperand());

  switch (E.getOp()) {
  case TokenKind::Bang: {
    phi::Type Type = E.getOperand().getType();
    if (!Type.isPrimitive() ||
        E.getOperand().getType().asPrimitive() != PrimitiveKind::Bool) {
      error("Logical NOT can only be applied to bool type")
          .with_primary_label(E.getOperand().getLocation(),
                              "Expected this to be of type `bool`")
          .emit(*Diag);
      return false;
    }

    return Success;
  }
  case TokenKind::Minus: {
    phi::Type Type = E.getOperand().getType();
    if (!Type.isPrimitive() || E.getOperand().getType().isUnsignedInteger()) {
      error("Negation can only be applied to signed intgers or floats")
          .with_primary_label(E.getOperand().getLocation(),
                              "Expected this to be of type `i8`, `i16`, `i32`, "
                              "`i64`, `f32`, or `f64`")
          .emit(*Diag);
      return false;
    }

    return Success;
  }
  default:
    llvm_unreachable("This is unreachable");
    return true;
  }
}
