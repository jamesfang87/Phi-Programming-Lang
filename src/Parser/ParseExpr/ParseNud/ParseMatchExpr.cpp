#include "AST/Stmt.hpp"
#include "Diagnostics/DiagnosticBuilder.hpp"
#include "Lexer/TokenKind.hpp"
#include "Parser/Parser.hpp"

#include <cassert>
#include <memory>
#include <print>
#include <vector>

#include <llvm/Support/Casting.h>

#include "AST/Expr.hpp"

namespace phi {
std::unique_ptr<MatchExpr> Parser::parseMatchExpr() {
  const auto Location = peekToken(-1).getStart(); // def safe

  // parse the Expr which we are matching on
  NoStructInit = true;
  auto Value = parseExpr();
  NoStructInit = false;

  if (!matchToken(TokenKind::OpenBrace)) {
    emitUnexpectedTokenError(peekToken());
  }

  std::vector<MatchExpr::Case> Cases;
  while (peekKind() != TokenKind::CloseBrace) {
    // TODO: Add more complex pattern matching
    std::vector<std::unique_ptr<Expr>> Patterns;
    auto Res = parseExpr();
    Patterns.push_back(std::move(Res));

    if (!matchToken(TokenKind::FatArrow)) {
      emitUnexpectedTokenError(peekToken());
    }

    std::unique_ptr<Block> Body = nullptr;
    Expr *Return = nullptr;
    switch (peekKind()) {
    case TokenKind::OpenBrace:
      Body = parseBlock();
      Body->emit(0);
      if (Body->getStmts().empty())
        break;

      if (auto S = llvm::dyn_cast<ExprStmt>(Body->getStmts().back().get())) {
        Return = &S->getExpr();
      } else {
        error("Invalid expression as return value in match case")
            .with_primary_label(Body->getStmts().back()->getLocation(),
                                "Expected this to be a proper expression")
            .emit(*DiagnosticsMan);
      }
      break;
    default:
      auto Res = parseExpr();
      std::vector<std::unique_ptr<Stmt>> TempBlock;
      auto TempStmt =
          std::make_unique<ExprStmt>(Res->getLocation(), std::move(Res));
      Return = &TempStmt->getExpr();
      TempBlock.push_back(std::move(TempStmt));
      Body = std::make_unique<Block>(std::move(TempBlock));

      // now we must check whether the next token is
      // a comma or a }
      // we use peekKind for closebrace since we don't want to advance
      // as we consume it later
      if (!matchToken(TokenKind::Comma) &&
          peekKind() != TokenKind::CloseBrace) {
        emitUnexpectedTokenError(peekToken(), {",", "}"});
        syncTo({TokenKind::Identifier});
        // TODO: Improve sync for patterns
      }
    }

    Cases.push_back(MatchExpr::Case{.Patterns = std::move(Patterns),
                                    .Body = std::move(Body),
                                    .Return = Return});
  }

  if (!matchToken(TokenKind::CloseBrace)) {
    emitUnexpectedTokenError(peekToken());
  }

  return std::make_unique<MatchExpr>(Location, std::move(Value),
                                     std::move(Cases));
}
} // namespace phi
