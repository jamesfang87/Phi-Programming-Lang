#include "Lexer/TokenKind.hpp"
#include "Parser/Parser.hpp"

#include <cassert>
#include <llvm/Support/Casting.h>
#include <memory>

#include "AST/Expr.hpp"

namespace phi {

std::unique_ptr<CustomTypeCtor>
Parser::parseCustomInit(std::unique_ptr<Expr> InitExpr) {
  auto *const DeclRef = llvm::dyn_cast<DeclRefExpr>(InitExpr.get());
  std::string StructId = DeclRef->getId();

  auto Inits = parseList<MemberInitExpr>(
      TokenKind::OpenBrace, TokenKind::CloseBrace, &Parser::parseMemberInit);

  return (!Inits) ? nullptr
                  : std::make_unique<CustomTypeCtor>(InitExpr->getLocation(),
                                                     StructId,
                                                     std::move(Inits.value()));
}

std::unique_ptr<MemberInitExpr> Parser::parseMemberInit() {
  if (peekKind() != TokenKind::Identifier) {
    emitUnexpectedTokenError(peekToken(), {"Identifier"});
    return nullptr;
  }
  SrcLocation Loc = peekToken().getStart();
  std::string FieldId = advanceToken().getLexeme();

  // MemberInitExprs can be for data-less enum variants, so an equal sign
  // is not required, as it would be if they were only representing fields
  if (peekKind() != TokenKind::Equals) {
    return std::make_unique<MemberInitExpr>(Loc, std::move(FieldId), nullptr);
  }

  // Otherwise, we consume the equals sign, parse the init expr and return
  assert(peekKind() == TokenKind::Equals);
  advanceToken();
  auto Init = parseExpr();
  return std::make_unique<MemberInitExpr>(Loc, std::move(FieldId),
                                          std::move(Init));
}

} // namespace phi
