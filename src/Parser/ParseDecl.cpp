#include "Parser/Parser.hpp"

#include <cassert>
#include <memory>
#include <optional>
#include <utility>

#include "Diagnostics/DiagnosticBuilder.hpp"
#include "Lexer/TokenKind.hpp"
#include "SrcManager/SrcLocation.hpp"

namespace phi {

/**
 * Parses a function declaration from the token stream.
 *
 * @return std::unique_ptr<FunDecl> Function AST node or nullptr on error.
 *         Errors are emitted to DiagnosticManager.
 *
 * Parsing sequence:
 * 1. 'fun' keyword
 * 2. Function name identifier
 * 3. Parameter list in parentheses
 * 4. Optional return type (-> type)
 * 5. Function body block
 *
 * Handles comprehensive error recovery and validation at each step.
 */
std::unique_ptr<FunDecl> Parser::parseFunDecl() {
  Token FunKw = advanceToken(); // Eat 'fun'
  SrcSpan FunKwSpan = FunKw.getSpan();

  // Validate function name
  if (peekToken().getKind() != TokenKind::Identifier) {
    error("invalid function name")
        .with_primary_label(peekToken().getSpan(),
                            "expected function name here")
        .with_secondary_label(FunKwSpan, "after `fun` keyword")
        .with_help("function names must be valid identifiers")
        .with_note("identifiers must start with a letter or underscore")
        .with_code("E0006")
        .emit(*DiagnosticsMan);
    return nullptr;
  }
  std::string Id = peekToken().getLexeme();
  SrcSpan IdSpan = advanceToken().getSpan();

  // Parse parameter list
  auto Params = parseList<ParamDecl>(TokenKind(TokenKind::OpenParen),
                                     TokenKind(TokenKind::CloseParen),
                                     &Parser::parseParamDecl);
  if (!Params)
    return nullptr;

  for (auto &Param : *Params) {
    assert(Param->hasType());
  }

  // Handle optional return type
  std::optional<TypeRef> ReturnTy = std::nullopt;
  if (peekToken().getKind() == TokenKind::Arrow) {
    advanceToken(); // eat '->'
    ReturnTy.emplace(std::move(*parseType()));
    if (!ReturnTy)
      return nullptr;
  } else {
    ReturnTy.emplace(TypeCtx::getBuiltin(BuiltinTy::Null, IdSpan));
  }

  auto Body = parseBlock();
  if (!Body)
    return nullptr;

  return std::make_unique<FunDecl>(FunKwSpan.Start, std::move(Id), *ReturnTy,
                                   std::move(Params.value()), std::move(Body));
}

/**
 * Parses a typed binding (name: type) used in declarations.
 *
 * @return std::optional<std::pair<std::string, Type>> Name-type pair or
 * nullopt on error. Errors are emitted to DiagnosticManager.
 *
 * Format: identifier ':' type
 * Used in variable declarations, function parameters, etc.
 */
std::optional<Parser::TypedBinding> Parser::parseTypedBinding() {
  // Parse identifier
  if (peekKind() != TokenKind::Identifier) {
    error("expected identifier")
        .with_primary_label(peekToken().getSpan(),
                            std::format("expected identifier here (got {})",
                                        peekKind().toString()))
        .emit(*DiagnosticsMan);
    return std::nullopt;
  }
  SrcLocation Start = peekToken().getStart();
  std::string Name = advanceToken().getLexeme();

  // Parse colon separator
  if (peekToken().getKind() != TokenKind::Colon) {
    error("expected colon")
        .with_primary_label(peekToken().getSpan(), "expected `:` here")
        .with_suggestion(peekToken().getSpan(), ":", "add colon before type")
        .emit(*DiagnosticsMan);
    return std::nullopt;
  }
  advanceToken();

  // Parse type
  auto DeclType = parseType();
  if (!DeclType)
    return std::nullopt;

  return TypedBinding{.Loc = Start, .Name = Name, .Type = DeclType};
}

/**
 * Parses a function parameter declaration.
 *
 * @return std::unique_ptr<ParamDecl> Parameter AST or nullptr on error.
 *         Errors are emitted to DiagnosticManager.
 *
 */
std::unique_ptr<ParamDecl> Parser::parseParamDecl() {
  bool IsConst = true;
  if (peekToken().getKind() == TokenKind::ConstKw) {
    IsConst = true;
    advanceToken();
  } else if (peekToken().getKind() == TokenKind::VarKw) {
    IsConst = false;
    advanceToken();
  } else {
    emitUnexpectedTokenError(peekToken(), {"const", "var"});
    return nullptr;
  }

  auto Binding = parseTypedBinding();
  if (!Binding)
    return nullptr;

  auto [Loc, Id, DeclType] = *Binding;
  assert(DeclType);
  return std::make_unique<ParamDecl>(Loc, Id, *DeclType, IsConst);
}

} // namespace phi
