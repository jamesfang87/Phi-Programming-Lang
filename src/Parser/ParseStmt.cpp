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
std::unique_ptr<Stmt> Parser::parseStmt() {
  switch (peekToken().getType()) {
  case TokenKind::tokReturn:
    return parseReturn();
  case TokenKind::tokIf:
    return parseIf();
  case TokenKind::tokWhile:
    return parseWhile();
  case TokenKind::tokFor:
    return parseFor();
  case TokenKind::tokLet:
    return parseLet();
  case TokenKind::tokBreak:
    return parseBreak();
  case TokenKind::tokContinue:
    return parseContinue();
  default:
    auto res = parseExpr();
    advanceToken(); // this is for the semicolon
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
std::unique_ptr<ReturnStmt> Parser::parseReturn() {
  SrcLocation loc = peekToken().getStart();
  advanceToken(); // eat 'return'

  // Null return: return;
  if (peekToken().getType() == TokenKind::tokSemicolon) {
    advanceToken(); // eat ';'
    return std::make_unique<ReturnStmt>(loc, nullptr);
  }

  // Value return: return expr;
  auto expr = parseExpr();
  if (!expr) {
    return nullptr;
  }

  // Validate semicolon terminator
  if (peekToken().getType() != TokenKind::tokSemicolon) {
    error("missing semicolon after return statement")
        .with_primary_label(spanFromToken(peekToken()), "expected `;` here")
        .with_help("return statements must end with a semicolon")
        .with_suggestion(spanFromToken(peekToken()), ";", "add semicolon")
        .with_code("E0012")
        .emit(*diagnosticManager);
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
std::unique_ptr<IfStmt> Parser::parseIf() {
  SrcLocation loc = peekToken().getStart();
  advanceToken(); // eat 'if'

  auto cond = parseExpr();
  if (!cond)
    return nullptr;

  auto body = parseBlock();
  if (!body)
    return nullptr;

  // No else clause
  if (peekToken().getType() != TokenKind::tokElse) {
    return std::make_unique<IfStmt>(loc, std::move(cond), std::move(body),
                                    nullptr);
  }

  // Else clause
  advanceToken(); // eat 'else'

  // Else block: else { ... }
  if (peekToken().getType() == TokenKind::tokLeftBrace) {
    auto elseBody = parseBlock();
    if (!elseBody)
      return nullptr;

    return std::make_unique<IfStmt>(loc, std::move(cond), std::move(body),
                                    std::move(elseBody));
  }
  // Else if: else if ...
  if (peekToken().getType() == TokenKind::tokIf) {
    std::vector<std::unique_ptr<Stmt>> elifStmt;

    auto res = parseIf();
    if (!res)
      return nullptr;

    elifStmt.emplace_back(std::move(res));
    auto elifBody = std::make_unique<Block>(std::move(elifStmt));
    return std::make_unique<IfStmt>(loc, std::move(cond), std::move(body),
                                    std::move(elifBody));
  }

  // Invalid else clause
  error("invalid else clause")
      .with_primary_label(spanFromToken(peekToken()), "unexpected token here")
      .with_help(
          "else must be followed by a block `{` or another `if` statement")
      .with_code("E0040")
      .emit(*diagnosticManager);
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
std::unique_ptr<WhileStmt> Parser::parseWhile() {
  SrcLocation loc = peekToken().getStart();
  advanceToken(); // eat 'while'

  auto cond = parseExpr();
  if (!cond)
    return nullptr;

  auto body = parseBlock();
  if (!body)
    return nullptr;

  return std::make_unique<WhileStmt>(loc, std::move(cond), std::move(body));
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
std::unique_ptr<ForStmt> Parser::parseFor() {
  SrcLocation loc = peekToken().getStart();
  advanceToken(); // eat 'for'

  // Parse loop variable
  Token loopVar = advanceToken();
  if (loopVar.getType() != TokenKind::tokIdentifier) {
    error("for loop must have a loop variable")
        .with_primary_label(spanFromToken(loopVar), "expected identifier here")
        .with_help("for loops have the form: `for variable in iterable`")
        .with_note(
            "the loop variable will be assigned each value from the iterable")
        .with_code("E0014")
        .emit(*diagnosticManager);
    return nullptr;
  }

  // Validate 'in' keyword
  Token inKw = advanceToken();
  if (inKw.getType() != TokenKind::tokIn) {
    error("missing `in` keyword in for loop")
        .with_primary_label(spanFromToken(loopVar), "loop variable")
        .with_secondary_label(spanFromToken(inKw), "expected `in` here")
        .with_help("for loops have the form: `for variable in iterable`")
        .with_suggestion(spanFromToken(inKw), "in", "add `in` keyword")
        .with_code("E0015")
        .emit(*diagnosticManager);
    return nullptr;
  }

  // Parse range expression
  auto range = parseExpr();
  if (!range)
    return nullptr;

  // Parse loop body
  auto body = parseBlock();
  if (!body)
    return nullptr;

  // Create loop variable declaration (implicit i64 type)
  auto loop_var =
      std::make_unique<VarDecl>(loopVar.getStart(), loopVar.getLexeme(),
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
std::unique_ptr<LetStmt> Parser::parseLet() {
  SrcLocation letLoc = peekToken().getStart();
  if (advanceToken().getType() != TokenKind::tokLet) {
    emitUnexpectedTokenError(peekToken(), {"let"});
    return nullptr;
  }

  // Check if they specified that this should be mutable
  bool isMut = false;
  if (peekToken().getType() == TokenKind::TokMut) {
    advanceToken();
    isMut = true;
  }

  auto binding = parseTypedBinding();
  if (!binding)
    return nullptr;
  auto [VarLoc, Id, type] = *binding;

  // Validate assignment operator
  if (advanceToken().getType() != TokenKind::tokEquals) {
    error("missing assignment in variable declaration")
        .with_primary_label(spanFromToken(peekToken()), "expected `=` here")
        .with_help("variables must be initialized with a value")
        .with_note("variable syntax: `let name: type = value;`")
        .with_code("E0023")
        .emit(*diagnosticManager);
    return nullptr;
  }

  // Parse initializer expression
  auto Init = parseExpr();
  if (!Init)
    return nullptr;

  // Validate semicolon terminator
  if (advanceToken().getType() != TokenKind::tokSemicolon) {
    error("missing semicolon after variable declaration")
        .with_primary_label(spanFromToken(peekToken()), "expected `;` here")
        .with_help("variable declarations must end with a semicolon")
        .with_suggestion(spanFromToken(peekToken()), ";", "add semicolon")
        .with_code("E0025")
        .emit(*diagnosticManager);
    return nullptr;
  }

  return std::make_unique<LetStmt>(
      letLoc,
      std::make_unique<VarDecl>(VarLoc, Id, type, isMut, std::move(Init)));
}

std::unique_ptr<BreakStmt> Parser::parseBreak() {
  SrcLocation Loc = peekToken().getStart();
  if (advanceToken().getType() != TokenKind::tokBreak) {
    error("missing break keyword")
        .with_primary_label(spanFromToken(peekToken()), "expected `break` here")
        .with_help("break statements must be preceded by a loop")
        .with_code("E0026")
        .emit(*diagnosticManager);
    return nullptr;
  }

  // Validate semicolon terminator
  if (advanceToken().getType() != TokenKind::tokSemicolon) {
    error("missing semicolon after break statement")
        .with_primary_label(spanFromToken(peekToken()), "expected `;` here")
        .with_help("break statements must end with a semicolon")
        .with_suggestion(spanFromToken(peekToken()), ";", "add semicolon")
        .with_code("E0027")
        .emit(*diagnosticManager);
    return nullptr;
  }

  return std::make_unique<BreakStmt>(Loc);
}

std::unique_ptr<ContinueStmt> Parser::parseContinue() {
  SrcLocation Loc = peekToken().getStart();
  if (advanceToken().getType() != TokenKind::tokContinue) {
    error("missing continue keyword")
        .with_primary_label(spanFromToken(peekToken()),
                            "expected `continue` here")
        .with_help("continue statements must be preceded by a loop")
        .with_code("E0028")
        .emit(*diagnosticManager);
    return nullptr;
  }

  // Validate semicolon terminator
  if (advanceToken().getType() != TokenKind::tokSemicolon) {
    error("missing semicolon after continue statement")
        .with_primary_label(spanFromToken(peekToken()), "expected `;` here")
        .with_help("continue statements must end with a semicolon")
        .with_suggestion(spanFromToken(peekToken()), ";", "add semicolon")
        .with_code("E0029")
        .emit(*diagnosticManager);
    return nullptr;
  }

  return std::make_unique<ContinueStmt>(Loc);
}

} // namespace phi
