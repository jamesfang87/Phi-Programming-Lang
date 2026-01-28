#include "Parser/Parser.hpp"

#include <cassert>
#include <memory>

#include <llvm/Support/Casting.h>

#include "AST/Nodes/Expr.hpp"
#include "Lexer/TokenKind.hpp"

namespace phi {

std::unique_ptr<AdtInit> Parser::parseAdtInit(std::unique_ptr<Expr> InitExpr,
                                              std::vector<TypeRef> TypeArgs) {
  auto *const DeclRef = llvm::dyn_cast<DeclRefExpr>(InitExpr.get());
  std::string StructId = DeclRef->getId();

  auto Inits = parseList<MemberInit>(
      TokenKind::OpenBrace, TokenKind::CloseBrace, &Parser::parseMemberInit);

  return (!Inits) ? nullptr
                  : std::make_unique<AdtInit>(InitExpr->getLocation(), StructId,
                                              std::move(TypeArgs),
                                              std::move(Inits.value()));
}

std::unique_ptr<MemberInit> Parser::parseMemberInit() {
  if (!expectToken(TokenKind::Identifier, "Member Init", false)) {
    return nullptr;
  }
  SrcLocation Loc = peekToken().getStart();
  std::string FieldId = advanceToken().getLexeme();

  // MemberInitExprs can be for data-less enum variants, so an equal sign
  // is not required, as it would be if they were only representing fields
  if (peekKind() != TokenKind::Colon) {
    return std::make_unique<MemberInit>(Loc, std::move(FieldId), nullptr);
  }

  // Otherwise, we consume the equals sign, parse the init expr and return
  assert(advanceToken().getKind() == TokenKind::Colon);
  auto Init = parseExpr();
  if (!Init)
    return nullptr;
  return std::make_unique<MemberInit>(Loc, std::move(FieldId), std::move(Init));
}

} // namespace phi
