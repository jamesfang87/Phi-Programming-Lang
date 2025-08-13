#include "Diagnostics/DiagnosticBuilder.hpp"
#include "Lexer/TokenKind.hpp"
#include "Parser/Parser.hpp"

#include <expected>
#include <memory>
#include <print>
#include <utility>

#include "AST/Decl.hpp"
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
  if (peekToken().getKind() != TokenKind::IdentifierKind) {
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
  auto Params =
      parseList<ParamDecl>(TokenKind::OpenParenKind, TokenKind::CloseParenKind,
                           &Parser::parseParamDecl);
  if (!Params)
    return nullptr;

  // Handle optional return type
  auto ReturnType = Type(Type::PrimitiveKind::NullKind);
  if (peekToken().getKind() == TokenKind::ArrowKind) {
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

  return std::make_unique<FunDecl>(Decl::Kind::FunDecl, Loc, std::move(Id),
                                   ReturnType, std::move(Params.value()),
                                   std::move(Body));
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
  if (peekToken().getKind() != TokenKind::IdentifierKind) {
    error("expected identifier")
        .with_primary_label(spanFromToken(peekToken()),
                            "expected identifier here")
        .emit(*DiagnosticsMan);
    return std::nullopt;
  }
  SrcLocation start = peekToken().getStart();
  std::string name = advanceToken().getLexeme();

  // Parse colon separator
  if (peekToken().getKind() != TokenKind::ColonKind) {
    error("expected colon")
        .with_primary_label(spanFromToken(peekToken()), "expected `:` here")
        .with_suggestion(spanFromToken(peekToken()), ":",
                         "add colon before type")
        .emit(*DiagnosticsMan);
    return std::nullopt;
  }
  advanceToken();

  // Parse type
  auto type = parseType();
  if (!type.has_value())
    return std::nullopt;

  return TypedBinding{
      .loc = start, .name = name, .type = std::move(type.value())};
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
  if (peekToken().getKind() == TokenKind::ConstKwKind) {
    IsConst = true;
    advanceToken();
  } else if (peekToken().getKind() == TokenKind::VarKwKind) {
    IsConst = false;
    advanceToken();
  } else {
    emitUnexpectedTokenError(peekToken(), {"const", "var"});
    return nullptr;
  }

  auto binding = parseTypedBinding();
  if (!binding)
    return nullptr;

  auto [loc, id, type] = *binding;
  return std::make_unique<ParamDecl>(loc, id, type, IsConst);
}

} // namespace phi
