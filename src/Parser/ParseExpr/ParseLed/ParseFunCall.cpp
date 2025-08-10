#include "Parser/Parser.hpp"

namespace phi {

/**
 * Parses a function call expression.
 *
 * @param callee The function being called
 * @return std::unique_ptr<FunCallExpr> Call AST or nullptr on error
 *         Errors are emitted to DiagnosticManager.
 */
std::unique_ptr<FunCallExpr>
Parser::parseFunCall(std::unique_ptr<Expr> Callee) {
  auto Args = parseList<Expr>(TokenKind::OpenParenKind,
                              TokenKind::CloseParenKind, &Parser::parseExpr);

  // Check if there were errors during argument parsing
  if (!Args)
    return nullptr;

  return std::make_unique<FunCallExpr>(Callee->getLocation(), std::move(Callee),
                                       std::move(Args.value()));
}

} // namespace phi
