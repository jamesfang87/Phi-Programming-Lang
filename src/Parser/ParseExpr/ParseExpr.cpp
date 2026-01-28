#include "Parser/Parser.hpp"

#include <algorithm>
#include <cassert>
#include <initializer_list>
#include <memory>
#include <vector>

#include <llvm/Support/Casting.h>

#include "AST/Nodes/Expr.hpp"
#include "Lexer/Token.hpp"
#include "Lexer/TokenKind.hpp"
#include "Parser/PrecedenceTable.hpp"

namespace phi {

std::unique_ptr<Expr> Parser::parseExpr() {
  std::vector<TokenKind::Kind> Terminators = {
      TokenKind::Eof,        TokenKind::Semicolon,    TokenKind::Comma,
      TokenKind::CloseParen, TokenKind::CloseBracket, TokenKind::CloseBrace,
      TokenKind::Colon};
  if (NoAdtInit) {
    Terminators.push_back(TokenKind::OpenBrace);
  }

  auto Res = pratt(0, Terminators);
  if (!Res) {
    return nullptr;
  }
  return Res;
}

std::unique_ptr<Expr>
Parser::pratt(int MinBp, const std::vector<TokenKind::Kind> &Terminators) {
  std::unique_ptr<Expr> Lhs = parseNud(peekToken());
  if (!Lhs) {
    return nullptr;
  }

  // Process right-hand side and operators
  while (true) {
    Token Op = peekToken();

    // Terminate if we reach a terminator token
    if (std::ranges::find(Terminators, Op.getKind()) != Terminators.end()) {
      break;
    }

    // Handle postfix operators
    if (postfixBP(Op.getKind())) {
      const auto [L, R] = postfixBP(Op.getKind()).value();
      if (L < MinBp)
        break;

      Lhs = parsePostfix(Op, std::move(Lhs));
      if (!Lhs)
        return nullptr;

      continue;
    }

    // Handle infix operators
    if (infixBP(Op.getKind())) {
      auto [L, R] = infixBP(Op.getKind()).value();
      if (L < MinBp)
        break;

      Lhs = parseInfix(Op, std::move(Lhs), R);
      if (!Lhs)
        return nullptr;

      continue;
    }

    break; // No more operators to process
  }

  return Lhs;
}

} // namespace phi
