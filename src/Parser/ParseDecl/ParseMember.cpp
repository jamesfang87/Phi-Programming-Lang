#include "Parser/Parser.hpp"

#include <llvm/ADT/ScopeExit.h>

#include <memory>
#include <optional>
#include <print>

#include "AST/Nodes/Decl.hpp"
#include "AST/Nodes/Stmt.hpp"
#include "Lexer/TokenKind.hpp"

namespace phi {

std::optional<Visibility> Parser::parseAdtMemberVisibility() {
  switch (peekKind()) {
  case TokenKind::PublicKw:
    advanceToken();
    return Visibility::Public;
  case TokenKind::FunKw:
  case TokenKind::Identifier:
    return Visibility::Private;
  default:
    emitUnexpectedTokenError(peekToken());
    syncTo({TokenKind::PublicKw, TokenKind::FunKw, TokenKind::Identifier,
            TokenKind::CloseBrace});
    return std::nullopt;
  }
}

std::unique_ptr<ParamDecl> Parser::parseMethodParam(std::string ParentName) {
  if (peekToken(1).getKind() != TokenKind::ThisKw) {
    return parseParamDecl();
  }

  auto Constness = parseMutability();
  auto Base = TypeCtx::getAdt(ParentName, nullptr, peekToken().getSpan());
  auto Ty = TypeCtx::getRef(Base, peekToken().getSpan());
  return std::make_unique<ParamDecl>(advanceToken().getSpan(), *Constness,
                                     "this", Ty);
}

std::unique_ptr<MethodDecl> Parser::parseMethodDecl(std::string ParentName,
                                                    Visibility Vis) {
  assert(advanceToken().getKind() == TokenKind::FunKw);

  // Validate function name
  if (peekKind() != TokenKind::Identifier) {
    error("invalid function name")
        .with_primary_label(peekToken().getSpan(),
                            "expected function name here")
        .with_secondary_label(peekToken().getSpan(), "after `fun` keyword")
        .with_help("function names must be valid identifiers")
        .with_note("identifiers must start with a letter or underscore")
        .emit(*Diags);
    return nullptr;
  }
  SrcSpan Span = peekToken().getSpan();
  std::string Id = advanceToken().getLexeme();

  auto TypeArgs = parseTypeArgDecls();
  if (!TypeArgs) {
    return nullptr;
  }

  // Schedule cleanup at the end of this function
  size_t NumTypeArgs = TypeArgs->size();
  auto Cleanup = llvm::make_scope_exit([&] {
    for (size_t i = 0; i < NumTypeArgs; ++i)
      ValidGenerics.pop_back();
  });

  auto Params =
      parseList<ParamDecl>(TokenKind::OpenParen, TokenKind::CloseParen,
                           [&] { return parseMethodParam(ParentName); });
  if (!Params)
    return nullptr;

  auto ReturnTy = parseReturnTy(Span);
  if (!ReturnTy)
    return nullptr;

  auto Body = parseBlock();
  if (!Body)
    return nullptr;

  return std::make_unique<MethodDecl>(Span, Vis, Id, std::move(*TypeArgs),
                                      std::move(*Params), std::move(*ReturnTy),
                                      std::move(Body));
}

std::unique_ptr<FieldDecl> Parser::parseFieldDecl(uint32_t FieldIndex,
                                                  Visibility Vis) {
  auto Field = parseBinding({.Type = Policy::Required});
  if (!Field)
    return nullptr;

  auto &[Span, Id, Type, Init] = *Field;
  if (matchToken(TokenKind::Comma) || peekKind() == TokenKind::CloseBrace) {
    return std::make_unique<FieldDecl>(Span, FieldIndex, Vis, Id, *Type,
                                       std::move(Init));
  }

  error("missing comma after field declaration")
      .with_primary_label(peekToken().getSpan(), "expected `,` here")
      .with_help("field declarations must end with a comma")
      .with_suggestion(peekToken().getSpan(), ";", "add comma")
      .emit(*Diags);
  return nullptr;
}

std::unique_ptr<StructDecl> Parser::parseAnonymousStruct() {
  SrcLocation Start = peekToken().getStart();
  uint32_t FieldIndex = 0;
  auto Fields = parseList<FieldDecl>(
      TokenKind::OpenBrace, TokenKind::CloseBrace,
      [&] -> std::unique_ptr<FieldDecl> {
        auto Field = parseBinding({.Type = Required, .Init = Forbidden});
        if (!Field)
          return nullptr;

        auto &[Span, Name, Type, _] = *Field;
        return std::make_unique<FieldDecl>(
            Span, FieldIndex++, Visibility::Public, Name, *Type, nullptr);
      });
  SrcLocation End = peekToken(-1).getEnd();

  // Generate a unique name for the desugared anonymous struct
  static uint32_t AnonymousStructCounter = 0;
  std::string StructId = std::format("@struct_{}", ++AnonymousStructCounter);
  return std::make_unique<StructDecl>(
      SrcSpan(Start, End), Visibility::Public, StructId,
      std::vector<std::unique_ptr<TypeArgDecl>>{}, std::move(*Fields),
      std::vector<std::unique_ptr<MethodDecl>>{});
}

std::unique_ptr<VariantDecl> Parser::parseVariantDecl() {
  assert(peekToken().getKind() == TokenKind::Identifier);
  SrcSpan Span = peekToken().getSpan();
  std::string Id = advanceToken().getLexeme();

  // Comma or close brace indicates a variant with no payload;
  // parsing is trivial so we return early
  if (matchToken(TokenKind::Comma) || peekKind() == TokenKind::CloseBrace) {
    return std::make_unique<VariantDecl>(Span, std::move(Id), std::nullopt);
  }

  // anything else is unexpected: variants must be followed by ':' or ','
  if (!expectToken(TokenKind::Colon)) {
    return nullptr;
  }

  // Now the current token must be a Colon
  std::optional<TypeRef> PayloadType = std::nullopt;
  if (peekKind() != TokenKind::OpenBrace) {
    PayloadType = parseType(false);
  } else {
    auto Res = parseAnonymousStruct();
    if (!Res)
      return nullptr;

    PayloadType = TypeCtx::getAdt(Res->getId(), Res.get(), Res->getSpan());
    Ast.push_back(std::move(Res));
  }

  if (matchToken(TokenKind::Comma) || peekKind() == TokenKind::CloseBrace) {
    return (PayloadType) ? std::make_unique<VariantDecl>(
                               VariantDecl(Span, std::move(Id), PayloadType))
                         : nullptr;
  }

  // missing comma/brace — emit diagnostic and try to recover
  error("missing comma after enum variant declaration")
      .with_primary_label(peekToken().getSpan(), "expected `,` here")
      .with_help("enum variant declarations must end with a comma")
      .with_suggestion(peekToken().getSpan(), ",", "add comma")
      .emit(*Diags);

  // consume the unexpected token to avoid infinite loop and recover
  advanceToken();
  return nullptr;
}

// Helper to recursively replace type declarations in types
static TypeRef
replaceTypeDecl(TypeRef T,
                const std::vector<std::unique_ptr<TypeArgDecl>> &OldDecls,
                const std::vector<std::unique_ptr<TypeArgDecl>> &NewDecls) {
  if (T.isGeneric()) {
    auto *GenTy = (GenericTy *)T.getPtr();
    for (size_t i = 0; i < OldDecls.size(); ++i) {
      if (GenTy->getDecl() == OldDecls[i].get()) {
        return TypeCtx::getGeneric(GenTy->getId(), NewDecls[i].get(),
                                   T.getSpan());
      }
    }
    return T;
  }

  if (T.isApplied()) {
    auto *AppTy = (AppliedTy *)T.getPtr();
    auto Base = replaceTypeDecl(AppTy->getBase(), OldDecls, NewDecls);
    std::vector<TypeRef> Args;
    Args.reserve(AppTy->getArgs().size());
    for (const auto &Arg : AppTy->getArgs()) {
      Args.push_back(replaceTypeDecl(Arg, OldDecls, NewDecls));
    }
    return TypeCtx::getApplied(Base, Args, T.getSpan());
  }

  if (T.isTuple()) {
    auto *TupTy = (TupleTy *)T.getPtr();
    std::vector<TypeRef> ElemTys;
    ElemTys.reserve(TupTy->getElementTys().size());
    for (const auto &Elem : TupTy->getElementTys()) {
      ElemTys.push_back(replaceTypeDecl(Elem, OldDecls, NewDecls));
    }
    return TypeCtx::getTuple(ElemTys, T.getSpan());
  }

  if (T.isFun()) {
    auto *FuncTy = (FunTy *)T.getPtr();
    std::vector<TypeRef> Params;
    Params.reserve(FuncTy->getParamTys().size());
    for (const auto &P : FuncTy->getParamTys()) {
      Params.push_back(replaceTypeDecl(P, OldDecls, NewDecls));
    }
    auto Ret = replaceTypeDecl(FuncTy->getReturnTy(), OldDecls, NewDecls);
    return TypeCtx::getFun(Params, Ret, T.getSpan());
  }

  if (T.isPtr()) {
    auto *PointerTy = (PtrTy *)T.getPtr();
    return TypeCtx::getPtr(
        replaceTypeDecl(PointerTy->getPointee(), OldDecls, NewDecls),
        T.getSpan());
  }

  if (T.isRef()) {
    auto *ReferenceTy = (RefTy *)T.getPtr();
    return TypeCtx::getRef(
        replaceTypeDecl(ReferenceTy->getPointee(), OldDecls, NewDecls),
        T.getSpan());
  }

  if (T.isArray()) {
    auto *ArrTy = (ArrayTy *)T.getPtr();
    return TypeCtx::getArray(
        replaceTypeDecl(ArrTy->getContainedTy(), OldDecls, NewDecls),
        T.getSpan());
  }

  return T;
}

void Parser::desugarStaticMethod(
    std::string ParentName, std::string MethodName,
    const std::vector<std::unique_ptr<TypeArgDecl>> &ParentTypeArgs,
    std::vector<std::unique_ptr<TypeArgDecl>> MethodTypeArgs,
    std::vector<std::unique_ptr<ParamDecl>> Params, TypeRef ReturnTy,
    std::unique_ptr<Block> Body, SrcSpan Span, Visibility Vis) {

  // Struct::method
  std::string NewId = ParentName + "::" + MethodName;

  std::vector<std::unique_ptr<TypeArgDecl>> CombinedTypeArgs;
  // Clone parent type args
  for (const auto &Arg : ParentTypeArgs) {
    CombinedTypeArgs.push_back(
        std::make_unique<TypeArgDecl>(Arg->getSpan(), Arg->getId()));
  }

  // Move method type args
  for (auto &Arg : MethodTypeArgs) {
    CombinedTypeArgs.push_back(std::move(Arg));
  }

  // Let's adjust updatedParams and updatedReturnTy
  std::vector<std::unique_ptr<ParamDecl>> UpdatedParams;
  for (auto &Param : Params) {
    auto NewType =
        replaceTypeDecl(Param->getType(), ParentTypeArgs, CombinedTypeArgs);
    UpdatedParams.push_back(std::make_unique<ParamDecl>(
        Param->getSpan(), Param->getMutability(), Param->getId(), NewType));
  }

  auto UpdatedReturnTy =
      replaceTypeDecl(ReturnTy, ParentTypeArgs, CombinedTypeArgs);

  auto Fun = std::make_unique<FunDecl>(
      Span, Vis, std::move(NewId), std::move(CombinedTypeArgs),
      std::move(UpdatedParams), UpdatedReturnTy, std::move(Body));
  Ast.push_back(std::move(Fun));
}

} // namespace phi
