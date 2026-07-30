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
    return make_unique<ArrayIndex>(Lhs->getLocation(), std::move(Lhs),
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

  if (Op.getKind() == TokenKind::ExclRange ||
      Op.getKind() == TokenKind::InclRange) {

    bool Inclusive = Op.getKind() == TokenKind::InclRange;
    auto Rhs = pratt(RBp, Terminators); // this is the end of the range
    if (!Rhs)
      return nullptr;

    return std::make_unique<RangeLiteral>(Op.getStart(), std::move(Lhs),
                                          std::move(Rhs), Inclusive);
  }

  // A period can mean several things
  if (Op.getKind() == TokenKind::Period) {
    auto Rhs = pratt(RBp, Terminators);
    if (!Rhs)
      return nullptr;

    // field access
    if (auto *Field = llvm::dyn_cast<DeclRefExpr>(Rhs.get())) {
      return std::make_unique<FieldAccessExpr>(Field->getLocation(),
                                               std::move(Lhs), Field->getId());
    }

    // method call
    if (auto *FunCall = llvm::dyn_cast<FunCallExpr>(Rhs.get())) {
      return std::make_unique<MethodCallExpr>(std::move(*FunCall),
                                              std::move(Lhs));
    }

    // tuple access
    if (auto *_ = llvm::dyn_cast<IntLiteral>(Rhs.get())) {
      auto IntPtr = llvm::unique_dyn_cast<IntLiteral>(std::move(Rhs));
      return std::make_unique<TupleIndex>(Lhs->getLocation(), std::move(Lhs),
                                          std::move(IntPtr));
    }

    // TODO: Change this to error and see if it breaks anything
    return std::make_unique<BinaryOp>(std::move(Lhs), std::move(Rhs), Op);
  }

  // casting
  if (Op.getKind() == TokenKind::AsKw) {
    auto Rhs = parseType(false); // Type to cast to
    if (!Rhs)
      return nullptr;

    return std::make_unique<CastExpr>(Lhs->getLocation(), std::move(Lhs),
                                      std::move(*Rhs));
  }

  // Regular binary operators
  auto Rhs = pratt(RBp, Terminators);
  if (!Rhs)
    return nullptr;

  return std::make_unique<BinaryOp>(std::move(Lhs), std::move(Rhs), Op);
}

} // namespace phi
