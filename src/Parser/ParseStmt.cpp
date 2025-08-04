#include "Parser/Parser.hpp"

#include <memory>
#include <print>
#include <vector>

#include "AST/Stmt.hpp"
#include "Diagnostics/Diagnostic.hpp"
#include "Lexer/TokenType.hpp"

namespace phi {

/**
 * Dispatches to specific statement parsers based on current token.
 *
 * @return std::unique_ptr<Stmt> Statement AST or nullptr on error.
 *         Errors are emitted to DiagnosticManager.
 *
 * Handles:
 * - Return statements
 * - While loops
 * - If statements
 * - For loops
 * - Variable declarations (let)
 */
std::unique_ptr<Stmt> Parser::parse_stmt() {
  switch (peekToken().getTy()) {
  case TokenType::tok_return:
    return parse_return_stmt();
  case TokenType::tok_if:
    return parse_if_stmt();
  case TokenType::tok_while:
    return parse_while_stmt();
  case TokenType::tok_for:
    return parse_for_stmt();
  case TokenType::tok_let:
    return parse_let_stmt();
  default:
    auto res = parse_expr();
    advanceToken();
    return res;
  }
}

/**
 * Parses a return statement.
 *
 * @return std::unique_ptr<ReturnStmt> Return AST or nullptr on error.
 *         Errors are emitted to DiagnosticManager.
 *
 * Formats:
 * - return;       (implicit null)
 * - return expr;  (explicit value)
 *
 * Validates semicolon terminator and expression validity.
 */
std::unique_ptr<ReturnStmt> Parser::parse_return_stmt() {
  SrcLocation loc = peekToken().getStart();
  advanceToken(); // eat 'return'

  // Null return: return;
  if (peekToken().getTy() == TokenType::tok_semicolon) {
    advanceToken(); // eat ';'
    return std::make_unique<ReturnStmt>(loc, nullptr);
  }

  // Value return: return expr;
  auto expr = parse_expr();
  if (!expr) {
    return nullptr;
  }

  // Validate semicolon terminator
  if (peekToken().getTy() != TokenType::tok_semicolon) {
    error("missing semicolon after return statement")
        .with_primary_label(spanFromToken(peekToken()), "expected `;` here")
        .with_help("return statements must end with a semicolon")
        .with_suggestion(spanFromToken(peekToken()), ";", "add semicolon")
        .with_code("E0012")
        .emit(*diagnosticsManager);
    return nullptr;
  }
  advanceToken();

  return std::make_unique<ReturnStmt>(loc, std::move(expr));
}

/**
 * Parses an if statement with optional else clause.
 *
 * @return std::unique_ptr<IfStmt> If statement AST or nullptr on error.
 *         Errors are emitted to DiagnosticManager.
 *
 * Handles:
 * - if (cond) { ... }
 * - if (cond) { ... } else { ... }
 * - if (cond) { ... } else if { ... } (chained)
 */
std::unique_ptr<IfStmt> Parser::parse_if_stmt() {
  SrcLocation loc = peekToken().getStart();
  advanceToken(); // eat 'if'

  auto condition = parse_expr();
  if (!condition)
    return nullptr;

  auto body = parse_block();
  if (!body)
    return nullptr;

  // No else clause
  if (peekToken().getTy() != TokenType::tok_else) {
    return std::make_unique<IfStmt>(loc, std::move(condition), std::move(body),
                                    nullptr);
  }

  // Else clause
  advanceToken(); // eat 'else'

  // Else block: else { ... }
  if (peekToken().getTy() == TokenType::tok_open_brace) {
    auto else_body = parse_block();
    if (!else_body)
      return nullptr;

    return std::make_unique<IfStmt>(loc, std::move(condition), std::move(body),
                                    std::move(else_body));
  }
  // Else if: else if ...
  if (peekToken().getTy() == TokenType::tok_if) {
    std::vector<std::unique_ptr<Stmt>> elif_stmt;

    auto res = parse_if_stmt();
    if (!res)
      return nullptr;

    elif_stmt.emplace_back(std::move(res));
    auto elif_body = std::make_unique<Block>(std::move(elif_stmt));
    return std::make_unique<IfStmt>(loc, std::move(condition), std::move(body),
                                    std::move(elif_body));
  }

  // Invalid else clause
  error("invalid else clause")
      .with_primary_label(spanFromToken(peekToken()), "unexpected token here")
      .with_help(
          "else must be followed by a block `{` or another `if` statement")
      .with_code("E0040")
      .emit(*diagnosticsManager);
  return nullptr;
}

/**
 * Parses a while loop statement.
 *
 * @return std::unique_ptr<WhileStmt> While loop AST or nullptr on error.
 *         Errors are emitted to DiagnosticManager.
 *
 * Format: while (condition) { body }
 */
std::unique_ptr<WhileStmt> Parser::parse_while_stmt() {
  SrcLocation loc = peekToken().getStart();
  advanceToken(); // eat 'while'

  auto condition = parse_expr();
  if (!condition)
    return nullptr;

  auto body = parse_block();
  if (!body)
    return nullptr;

  return std::make_unique<WhileStmt>(loc, std::move(condition),
                                     std::move(body));
}

/**
 * Parses a for loop statement.
 *
 * @return std::unique_ptr<ForStmt> For loop AST or nullptr on error.
 *         Errors are emitted to DiagnosticManager.
 *
 * Format: for variable in range { body }
 *
 * Creates implicit loop variable declaration (i64 type).
 * Validates loop variable and 'in' keyword syntax.
 */
std::unique_ptr<ForStmt> Parser::parse_for_stmt() {
  SrcLocation loc = peekToken().getStart();
  advanceToken(); // eat 'for'

  // Parse loop variable
  Token looping_var = advanceToken();
  if (looping_var.getTy() != TokenType::tok_identifier) {
    error("for loop must have a loop variable")
        .with_primary_label(spanFromToken(looping_var),
                            "expected identifier here")
        .with_help("for loops have the form: `for variable in iterable`")
        .with_note(
            "the loop variable will be assigned each value from the iterable")
        .with_code("E0014")
        .emit(*diagnosticsManager);
    return nullptr;
  }

  // Validate 'in' keyword
  Token in_keyword = advanceToken();
  if (in_keyword.getTy() != TokenType::tok_in) {
    error("missing `in` keyword in for loop")
        .with_primary_label(spanFromToken(looping_var), "loop variable")
        .with_secondary_label(spanFromToken(in_keyword), "expected `in` here")
        .with_help("for loops have the form: `for variable in iterable`")
        .with_suggestion(spanFromToken(in_keyword), "in", "add `in` keyword")
        .with_code("E0015")
        .emit(*diagnosticsManager);
    return nullptr;
  }

  // Parse range expression
  auto range = parse_expr();
  if (!range)
    return nullptr;

  // Parse loop body
  auto body = parse_block();
  if (!body)
    return nullptr;

  // Create loop variable declaration (implicit i64 type)
  auto loop_var =
      std::make_unique<VarDecl>(looping_var.getStart(), looping_var.getLexeme(),
                                Type(Type::Primitive::i64), false, nullptr);

  return std::make_unique<ForStmt>(loc, std::move(loop_var), std::move(range),
                                   std::move(body));
}

/**
 * Parses a variable declaration (let statement).
 *
 * @return std::unique_ptr<LetStmt> Variable declaration AST or nullptr on
 * error. Errors are emitted to DiagnosticManager.
 *
 * Format: let name: type = value;
 *
 * Validates:
 * - Colon after identifier
 * - Valid type annotation
 * - Assignment operator
 * - Initializer expression
 * - Semicolon terminator
 */
std::unique_ptr<LetStmt> Parser::parse_let_stmt() {
  SrcLocation let_loc = peekToken().getStart();
  if (advanceToken().getTy() != TokenType::tok_let) {
    emit_unexpected_token_error(peekToken(), {"let"});
    return nullptr;
  }

  auto binding = parse_typed_binding();
  if (!binding)
    return nullptr;
  auto [var_loc, name, type] = *binding;

  // Validate assignment operator
  if (advanceToken().getTy() != TokenType::tok_assign) {
    error("missing assignment in variable declaration")
        .with_primary_label(spanFromToken(peekToken()), "expected `=` here")
        .with_help("variables must be initialized with a value")
        .with_note("variable syntax: `let name: type = value;`")
        .with_code("E0023")
        .emit(*diagnosticsManager);
    return nullptr;
  }

  // Parse initializer expression
  auto expr = parse_expr();
  if (!expr)
    return nullptr;

  // Validate semicolon terminator
  if (advanceToken().getTy() != TokenType::tok_semicolon) {
    error("missing semicolon after variable declaration")
        .with_primary_label(spanFromToken(peekToken()), "expected `;` here")
        .with_help("variable declarations must end with a semicolon")
        .with_suggestion(spanFromToken(peekToken()), ";", "add semicolon")
        .with_code("E0025")
        .emit(*diagnosticsManager);
    return nullptr;
  }

  return std::make_unique<LetStmt>(
      let_loc,
      std::make_unique<VarDecl>(var_loc, name, type, true, std::move(expr)));
}

} // namespace phi
