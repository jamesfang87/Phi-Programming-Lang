#include "AST/Stmt.hpp"
#include "Diagnostics/DiagnosticBuilder.hpp"
#include "Lexer/TokenKind.hpp"
#include "Parser/Parser.hpp"

#include <cassert>
#include <memory>
#include <vector>

#include <llvm/Support/Casting.h>

#include "AST/Expr.hpp"

namespace phi {
std::unique_ptr<MatchExpr> Parser::parseMatchExpr() {
  assert(peekKind() == TokenKind::MatchKw);
  const auto Location = peekToken().getStart();

  // parse the Expr which we are matching on
  auto Value = parseExpr();

  if (!matchToken(TokenKind::OpenBrace)) {
    emitUnexpectedTokenError(peekToken());
  }

  std::vector<MatchExpr::Case> Cases;
  while (peekKind() != TokenKind::CloseBrace) {
    // TODO: Add more complex pattern matching
    std::vector<std::unique_ptr<Expr>> Patterns;
    Patterns.push_back(parseExpr());

    if (!matchToken(TokenKind::Colon)) {
      emitUnexpectedTokenError(peekToken());
    }

    std::unique_ptr<Block> Body = nullptr;
    std::unique_ptr<Expr> Return = nullptr;
    switch (peekKind()) {
    case TokenKind::OpenBrace:
      Body = parseBlock();
      if (Body->getStmts().empty())
        break;

      if (auto S = llvm::dyn_cast<ExprStmt>(Body->getStmts().back())) {
        Return = S->takeExpr();
      } else {
        error("Invalid expression as return value in match case")
            .with_primary_label(Body->getStmts().back()->getLocation(),
                                "Expected this to be a proper expression")
            .emit(*DiagnosticsMan);
      }
      break;
    default:
      Return = parseExpr();
    }

    Cases.push_back(MatchExpr::Case{.Patterns = std::move(Patterns),
                                    .Body = std::move(Body),
                                    .Return = std::move(Return)});
  }

  if (!matchToken(TokenKind::CloseBrace)) {
    emitUnexpectedTokenError(peekToken());
  }

  return std::make_unique<MatchExpr>(Location, std::move(Value),
                                     std::move(Cases));
}
} // namespace phi
