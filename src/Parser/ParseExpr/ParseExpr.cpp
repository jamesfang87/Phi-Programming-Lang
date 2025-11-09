#include "Parser/Parser.hpp"

#include <algorithm>
#include <cassert>
#include <initializer_list>
#include <memory>
#include <print>
#include <vector>

#include <llvm/Support/Casting.h>

#include "AST/Expr.hpp"
#include "Lexer/Token.hpp"
#include "Lexer/TokenKind.hpp"
#include "Parser/PrecedenceTable.hpp"

namespace phi {

/**
 * Entry point for expression parsing.
 *
 * @return std::unique_ptr<Expr> Expression AST or nullptr on error.
 *         Errors are emitted to DiagnosticManager.
 *
 * Uses Pratt parsing with operator precedence handling.
 */
std::unique_ptr<Expr> Parser::parseExpr() {
  std::vector<TokenKind> Terminators = {
      TokenKind::Eof,        TokenKind::Semicolon,    TokenKind::Comma,
      TokenKind::CloseParen, TokenKind::CloseBracket, TokenKind::CloseBrace,
      TokenKind::Colon};
  if (NoStructInit) {
    Terminators.push_back(TokenKind::OpenBrace);
  }

  auto Res = pratt(0, Terminators);
  if (!Res) {
    std::println("Error parsing expression");
    return nullptr;
  }
  return Res;
}

/**
 * Pratt parser implementation for expressions.
 *
 * @param min_bp Minimum binding power for current expression
 * @return std::unique_ptr<Expr> Expression AST or nullptr on error
 *         Errors are emitted to DiagnosticManager.
 *
 * Recursive descent parser that handles:
 * - Prefix operators
 * - Literal values
 * - Identifiers
 * - Parenthesized expressions
 * - Postfix operations (function calls, member access)
 */
std::unique_ptr<Expr> Parser::pratt(int MinBp,
                                    const std::vector<TokenKind> &Terminators) {
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
