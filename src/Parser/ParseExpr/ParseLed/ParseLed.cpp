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
      return std::make_unique<MemberAccessExpr>(
          Member->getLocation(), std::move(Lhs), Member->getId());
    }

    if (auto FunCall = llvm::dyn_cast<FunCallExpr>(Rhs.get())) {
      // Transfer ownership of the FunCallExpr from rhs to the MemberFunCallExpr
      auto Temp = std::unique_ptr<FunCallExpr>(
          static_cast<FunCallExpr *>(Rhs.release()));
      return std::make_unique<MemberFunCallExpr>(
          FunCall->getLocation(), std::move(Lhs), std::move(Temp));
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
