#include "Lexer/TokenKind.hpp"
#include "Parser/Parser.hpp"

#include <memory>
#include <optional>

#include <llvm/Support/Casting.h>

#include "AST/Nodes/Expr.hpp"

namespace phi {

std::unique_ptr<Expr> Parser::parsePostfix(const Token &Op,
                                           std::unique_ptr<Expr> Lhs) {
  switch (Op.getKind()) {
  // Unary ops
  case TokenKind::DoublePlus:
  case TokenKind::DoubleMinus:
  case TokenKind::Try:
    advanceToken();
    return std::make_unique<UnaryOp>(std::move(Lhs), Op, false); // postfix
  case TokenKind::DoubleColon: {
    advanceToken();
    auto Res = parseTypeArgList(true);
    if (!Res) {
      return nullptr;
    }

    if (peekKind() == TokenKind::OpenParen) {
      return parseFunCall(std::move(Lhs), std::move(*Res));
    }

    if (peekKind() == TokenKind::OpenBrace && !NoAdtInit) {
      return parseAdtInit(std::move(Lhs), std::move(*Res));
    }
    break;
  }
  case TokenKind::OpenParen:
    return parseFunCall(std::move(Lhs), {});
  case TokenKind::OpenBrace:
    if (!NoAdtInit) {
      return parseAdtInit(std::move(Lhs), {});
    }
  case TokenKind::OpenBracket: {
    advanceToken();
    auto Index = parseExpr();
    advanceToken();
    return make_unique<IndexExpr>(Lhs->getLocation(), std::move(Lhs),
                                  std::move(Index));
  }
  default:
    return Lhs;
  }
  return Lhs;
}

std::unique_ptr<Expr> Parser::parseInfix(const Token &Op,
                                         std::unique_ptr<Expr> Lhs, int RBp) {
  std::vector<TokenKind::Kind> Terminators = {
      TokenKind::Eof, TokenKind::Semicolon, TokenKind::Comma,
      TokenKind::CloseParen, TokenKind::CloseBracket};
  if (NoAdtInit) {
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

    if (auto *Member = llvm::dyn_cast<DeclRefExpr>(Rhs.get())) {
      return std::make_unique<FieldAccessExpr>(Member->getLocation(),
                                               std::move(Lhs), Member->getId());
    }

    if (auto *FunCall = llvm::dyn_cast<FunCallExpr>(Rhs.get())) {
      return std::make_unique<MethodCallExpr>(std::move(*FunCall),
                                              std::move(Lhs));
    }

    if (auto *Int = llvm::dyn_cast<IntLiteral>(Rhs.get())) {
      return std::make_unique<IndexExpr>(Lhs->getLocation(), std::move(Lhs),
                                         std::move(Rhs));
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
