#include "Parser/Parser.hpp"

#include <memory>

#include <llvm/Support/Casting.h>

#include "AST/Expr.hpp"

namespace phi {

/**
 * Handles postfix expressions after primary expression is parsed.
 *
 * @param expr The left-hand side expression
 * @return std::unique_ptr<Expr> Resulting expression or nullptr on error
 *         Errors are emitted to DiagnosticManager.
 *
 * Currently handles:
 * - Function calls: expr(...)
 * - Member access: expr.member (stubbed)
 */
std::unique_ptr<Expr> Parser::parsePostfix(const Token &Op,
                                           std::unique_ptr<Expr> Lhs) {
  switch (Op.getKind()) {
  // Unary ops
  case TokenKind::DoublePlus:
  case TokenKind::DoubleMinus:
    advanceToken();
    return std::make_unique<UnaryOp>(std::move(Lhs), Op, false); // postfix

  // Function call
  case TokenKind::OpenParen:
    return parseFunCall(std::move(Lhs));

  // Struct init
  case TokenKind::OpenBrace:
    if (!NoStructInit)
      return parseStructInit(std::move(Lhs));

  default:
    return Lhs;
  }
}

/**
 * Handles infix expressions after primary expression is parsed.
 *
 * @param expr The left-hand side expression
 * @return std::unique_ptr<Expr> Resulting expression or nullptr on error
 *         Errors are emitted to DiagnosticManager.
 */
std::unique_ptr<Expr> Parser::parseInfix(const Token &Op,
                                         std::unique_ptr<Expr> Lhs, int RBp) {
  std::vector<TokenKind> Terminators = {TokenKind::Eof, TokenKind::Semicolon,
                                        TokenKind::Comma, TokenKind::CloseParen,
                                        TokenKind::CloseBracket};
  if (NoStructInit) {
    Terminators.push_back(TokenKind::OpenBrace);
  }

  advanceToken(); // consume operator

  // Special handling for range operators
  if (Op.getKind() == TokenKind::ExclRange ||
      Op.getKind() == TokenKind::InclRange) {

    bool Inclusive = Op.getKind() == TokenKind::InclRange;
    auto Rhs = pratt(RBp, Terminators); // this is the end of the range
    if (!Rhs)
      return nullptr;

    return std::make_unique<RangeLiteral>(Op.getStart(), std::move(Lhs),
                                          std::move(Rhs), Inclusive);
  }

  if (Op.getKind() == TokenKind::Period) {
    auto Rhs = pratt(RBp, Terminators);
    if (!Rhs)
      return nullptr;

    if (auto Member = llvm::dyn_cast<DeclRefExpr>(Rhs.get())) {
      return std::make_unique<FieldAccessExpr>(Member->getLocation(),
                                               std::move(Lhs), Member->getId());
    }

    if (auto FunCall = llvm::dyn_cast<FunCallExpr>(Rhs.get())) {
      // Extract components from the FunCallExpr
      auto Location = FunCall->getLocation();

      // Release the FunCallExpr and extract its components
      auto *RawFunCall = static_cast<FunCallExpr *>(Rhs.release());

      // Move the args out of the FunCallExpr
      auto Args = std::move(RawFunCall->getArgs());

      // Create a new DeclRefExpr for the callee (method name)
      // The callee should be a DeclRefExpr for the method name
      std::unique_ptr<Expr> CalleePtr;
      if (auto *DeclRef =
              llvm::dyn_cast<DeclRefExpr>(&RawFunCall->getCallee())) {
        CalleePtr = std::make_unique<DeclRefExpr>(DeclRef->getLocation(),
                                                  DeclRef->getId());
      } else {
        // If callee is not a simple DeclRefExpr, we need to handle it
        // differently For now, create a copy - this might need refinement based
        // on actual usage
        delete RawFunCall;
        return nullptr;
      }

      delete RawFunCall;

      return std::make_unique<MethodCallExpr>(
          Location, std::move(Lhs), std::move(CalleePtr), std::move(Args));
    }

    return std::make_unique<BinaryOp>(std::move(Lhs), std::move(Rhs), Op);
  }

  // Regular binary operators
  auto Rhs = pratt(RBp, Terminators);
  if (!Rhs)
    return nullptr;

  return std::make_unique<BinaryOp>(std::move(Lhs), std::move(Rhs), Op);
}

} // namespace phi
