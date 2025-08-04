#include "Diagnostics/DiagnosticBuilder.hpp"
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
std::unique_ptr<FunDecl> Parser::parse_fun_decl() {
  Token tok = advanceToken(); // Eat 'fun'
  SrcLocation loc = tok.getStart();

  // Validate function name
  if (peekToken().getTy() != TokenType::tok_identifier) {
    error("invalid function name")
        .with_primary_label(spanFromToken(peekToken()),
                            "expected function name here")
        .with_secondary_label(spanFromToken(tok), "after `fun` keyword")
        .with_help("function names must be valid identifiers")
        .with_note("identifiers must start with a letter or underscore")
        .with_code("E0006")
        .emit(*diagnosticsManager);
    return nullptr;
  }
  std::string name = advanceToken().getLexeme();

  // Parse parameter list
  auto param_list = parse_list<ParamDecl>(TokenType::tok_open_paren,
                                          TokenType::tok_close_paren,
                                          &Parser::parse_param_decl);
  if (!param_list)
    return nullptr;

  // Handle optional return type
  auto return_type = Type(Type::Primitive::null);
  if (peekToken().getTy() == TokenType::tok_fun_return) {
    advanceToken(); // eat '->'
    auto res = parse_type();
    if (!res)
      return nullptr;
    return_type = res.value();
  }

  // Parse function body
  auto body = parse_block();
  if (!body)
    return nullptr;

  return std::make_unique<FunDecl>(loc, std::move(name), return_type,
                                   std::move(param_list.value()),
                                   std::move(body));
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
std::optional<Parser::TypedBinding> Parser::parse_typed_binding() {
  // Parse identifier
  if (peekToken().getTy() != TokenType::tok_identifier) {
    error("expected identifier")
        .with_primary_label(spanFromToken(peekToken()),
                            "expected identifier here")
        .emit(*diagnosticsManager);
    return std::nullopt;
  }
  SrcLocation start = peekToken().getStart();
  std::string name = advanceToken().getLexeme();

  // Parse colon separator
  if (peekToken().getTy() != TokenType::tok_colon) {
    error("expected colon")
        .with_primary_label(spanFromToken(peekToken()), "expected `:` here")
        .with_suggestion(spanFromToken(peekToken()), ":",
                         "add colon before type")
        .emit(*diagnosticsManager);
    return std::nullopt;
  }
  advanceToken();

  // Parse type
  auto type = parse_type();
  if (!type)
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
std::unique_ptr<ParamDecl> Parser::parse_param_decl() {
  auto binding = parse_typed_binding();
  if (!binding)
    return nullptr;

  auto [loc, name, type] = *binding;
  return std::make_unique<ParamDecl>(loc, name, type);
}

} // namespace phi
