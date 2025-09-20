#include "Parser/Parser.hpp"

#include <llvm/Support/Casting.h>

#include "AST/Expr.hpp"

namespace phi {

std::unique_ptr<StructLiteral>
Parser::parseStructInit(std::unique_ptr<Expr> InitExpr) {
  const auto DeclRef = llvm::dyn_cast<DeclRefExpr>(InitExpr.get());
  std::string StructId = DeclRef->getId();

  auto FieldInits = parseList<FieldInitExpr>(
      TokenKind::OpenBrace, TokenKind::CloseBrace, &Parser::parseFieldInit);
  if (!FieldInits) {
    return nullptr;
  }

  return std::make_unique<StructLiteral>(InitExpr->getLocation(), StructId,
                                         std::move(FieldInits.value()));
}

std::unique_ptr<FieldInitExpr> Parser::parseFieldInit() {
  if (peekToken().getKind() != TokenKind::Identifier) {
    emitUnexpectedTokenError(peekToken(), {"Identifier"});
    return nullptr;
  }
  SrcLocation Loc = peekToken().getStart();
  std::string FieldId = advanceToken().getLexeme();

  if (peekToken().getKind() != TokenKind::Equals) {
    emitUnexpectedTokenError(peekToken(), {"="});
    return nullptr;
  }
  advanceToken();

  auto Init = parseExpr();

  return std::make_unique<FieldInitExpr>(Loc, std::move(FieldId),
                                         std::move(Init));
}

} // namespace phi
