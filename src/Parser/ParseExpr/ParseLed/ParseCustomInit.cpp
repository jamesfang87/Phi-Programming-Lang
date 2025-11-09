#include "Parser/Parser.hpp"

#include <llvm/Support/Casting.h>
#include <memory>

#include "AST/Expr.hpp"

namespace phi {

std::unique_ptr<CustomTypeCtor>
Parser::parseCustomInit(std::unique_ptr<Expr> InitExpr) {
  const auto DeclRef = llvm::dyn_cast<DeclRefExpr>(InitExpr.get());
  std::string StructId = DeclRef->getId();

  auto Inits = parseList<FieldInitExpr>(
      TokenKind::OpenBrace, TokenKind::CloseBrace, &Parser::parseFieldInit);

  return (!Inits) ? nullptr
                  : std::make_unique<CustomTypeCtor>(InitExpr->getLocation(),
                                                     StructId,
                                                     std::move(Inits.value()));
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
