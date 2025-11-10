#include "Parser/Parser.hpp"

#include <llvm/Support/Casting.h>
#include <memory>

#include "AST/Expr.hpp"

namespace phi {

std::unique_ptr<CustomTypeCtor>
Parser::parseCustomInit(std::unique_ptr<Expr> InitExpr) {
  auto *const DeclRef = llvm::dyn_cast<DeclRefExpr>(InitExpr.get());
  std::string StructId = DeclRef->getId();

  auto Inits = parseList<MemberInitExpr>(
      TokenKind::OpenBrace, TokenKind::CloseBrace, &Parser::parseFieldInit);

  return (!Inits) ? nullptr
                  : std::make_unique<CustomTypeCtor>(InitExpr->getLocation(),
                                                     StructId,
                                                     std::move(Inits.value()));
}

std::unique_ptr<MemberInitExpr> Parser::parseFieldInit() {
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

  return std::make_unique<MemberInitExpr>(Loc, std::move(FieldId),
                                          std::move(Init));
}

} // namespace phi
