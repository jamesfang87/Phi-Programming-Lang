#include "Parser/Parser.hpp"

#include <cassert>
#include <memory>
#include <vector>

#include <llvm/Support/Casting.h>

#include "AST/Expr.hpp"
#include "AST/Stmt.hpp"
#include "Diagnostics/DiagnosticBuilder.hpp"
#include "Lexer/TokenKind.hpp"

namespace phi {
std::unique_ptr<MatchExpr> Parser::parseMatchExpr() {
  const auto Location = peekToken(-1).getStart(); // def safe

  // parse the scrutinee
  NoStructInit = true;
  auto Scrutinee = parseExpr();
  NoStructInit = false;

  if (!matchToken(TokenKind::OpenBrace)) {
    emitUnexpectedTokenError(peekToken());
  }

  std::vector<MatchExpr::Arm> Arms;
  while (peekKind() != TokenKind::CloseBrace) {
    auto Pattern = parsePattern();

    if (!matchToken(TokenKind::FatArrow)) {
      emitUnexpectedTokenError(peekToken());
    }

    std::unique_ptr<Block> Body = nullptr;
    Expr *Return = nullptr;
    switch (peekKind()) {
    case TokenKind::OpenBrace:
      Body = parseBlock();
      if (Body->getStmts().empty())
        break;

      if (auto *S = llvm::dyn_cast<ExprStmt>(Body->getStmts().back().get())) {
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
      auto Temp =
          std::make_unique<ExprStmt>(Res->getLocation(), std::move(Res));
      Return = &Temp->getExpr();
      TempBlock.push_back(std::move(Temp));
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

    Arms.push_back(MatchExpr::Arm{.Pattern = std::move(*Pattern),
                                  .Body = std::move(Body),
                                  .Return = Return});
  }

  if (!matchToken(TokenKind::CloseBrace)) {
    emitUnexpectedTokenError(peekToken());
  }

  return std::make_unique<MatchExpr>(Location, std::move(Scrutinee),
                                     std::move(Arms));
}
} // namespace phi
