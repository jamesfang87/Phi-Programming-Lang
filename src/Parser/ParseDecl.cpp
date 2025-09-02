#include "Parser/Parser.hpp"

#include <cassert>
#include <memory>
#include <optional>
#include <utility>

#include "AST/Decl.hpp"
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
  Token Tok = advanceToken(); // Eat 'fun'
  SrcLocation Loc = Tok.getStart();

  // Validate function name
  if (peekToken().getKind() != TokenKind::Identifier) {
    error("invalid function name")
        .with_primary_label(spanFromToken(peekToken()),
                            "expected function name here")
        .with_secondary_label(spanFromToken(Tok), "after `fun` keyword")
        .with_help("function names must be valid identifiers")
        .with_note("identifiers must start with a letter or underscore")
        .with_code("E0006")
        .emit(*DiagnosticsMan);
    return nullptr;
  }
  std::string Id = advanceToken().getLexeme();

  // Parse parameter list
  auto Params = parseList<ParamDecl>(
      TokenKind::OpenParen, TokenKind::CloseParen, &Parser::parseParamDecl);
  if (!Params)
    return nullptr;

  for (auto &Param : *Params) {
    assert(Param->hasType());
  }

  // Handle optional return type
  auto ReturnType = Type::makePrimitive(PrimitiveKind::Null, SrcLocation{});
  if (peekToken().getKind() == TokenKind::Arrow) {
    advanceToken(); // eat '->'
    auto Res = parseType();
    if (!Res.has_value())
      return nullptr;
    ReturnType = Res.value();
  }

  // Parse function body
  auto Body = parseBlock();
  if (!Body)
    return nullptr;

  return std::make_unique<FunDecl>(Loc, std::move(Id), ReturnType,
                                   std::move(Params.value()), std::move(Body));
}

/**
 * Parses a typed binding (name: type) used in declarations.
 *
 * @return std::optional<std::pair<std::string, Type>> Name-type pair or nullopt
 * on error. Errors are emitted to DiagnosticManager.
 *
 * Format: identifier ':' type
 * Used in variable declarations, function parameters, etc.
 */
std::optional<Parser::TypedBinding> Parser::parseTypedBinding() {
  // Parse identifier
  if (peekToken().getKind() != TokenKind::Identifier) {
    error("expected identifier")
        .with_primary_label(spanFromToken(peekToken()),
                            "expected identifier here")
        .emit(*DiagnosticsMan);
    return std::nullopt;
  }
  SrcLocation Start = peekToken().getStart();
  std::string Name = advanceToken().getLexeme();

  // Parse colon separator
  if (peekToken().getKind() != TokenKind::Colon) {
    error("expected colon")
        .with_primary_label(spanFromToken(peekToken()), "expected `:` here")
        .with_suggestion(spanFromToken(peekToken()), ":",
                         "add colon before type")
        .emit(*DiagnosticsMan);
    return std::nullopt;
  }
  advanceToken();

  // Parse type
  auto DeclType = parseType();
  if (!DeclType.has_value())
    return std::nullopt;

  return TypedBinding{
      .Loc = Start, .Name = Name, .Type = std::move(DeclType.value())};
}

/**
 * Parses a function parameter declaration.
 *
 * @return std::unique_ptr<ParamDecl> Parameter AST or nullptr on error.
 *         Errors are emitted to DiagnosticManager.
 *
 * Wrapper around parse_typed_binding() that creates a ParamDecl node.
 */
std::unique_ptr<ParamDecl> Parser::parseParamDecl() {
  bool IsConst;
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

  auto binding = parseTypedBinding();
  if (!binding)
    return nullptr;

  auto [Loc, Id, DeclType] = *binding;
  return std::make_unique<ParamDecl>(Loc, Id, DeclType, IsConst);
}

} // namespace phi
