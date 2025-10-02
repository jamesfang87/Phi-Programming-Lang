#include "Parser/Parser.hpp"

#include <cassert>
#include <memory>
#include <optional>
#include <string>
#include <vector>

#include "AST/Stmt.hpp"
#include "Diagnostics/Diagnostic.hpp"
#include "Lexer/TokenKind.hpp"

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
  switch (peekToken().getKind()) {
  case TokenKind::ReturnKw:
    return parseReturnStmt();
  case TokenKind::DeferKw:
    return parseDeferStmt();
  case TokenKind::IfKw:
    return parseIfStmt();
  case TokenKind::WhileKw:
    return parseWhileStmt();
  case TokenKind::ForKw:
    return parseForStmt();
  case TokenKind::VarKw:
  case TokenKind::ConstKw:
    return parseDeclStmt();
  case TokenKind::BreakKw:
    return parseBreakStmt();
  case TokenKind::ContinueKw:
    return parseContinueStmt();
  default:
    auto Res = parseExpr();
    if (!Res)
      return nullptr;
    SrcLocation Loc = Res->getLocation();
    advanceToken(); // this is for the semicolon
    return std::make_unique<ExprStmt>(Loc, std::move(Res));
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
std::unique_ptr<ReturnStmt> Parser::parseReturnStmt() {
  SrcLocation Loc = peekToken().getStart();
  advanceToken(); // eat 'return'

  // Null return: return;
  if (peekToken().getKind() == TokenKind::Semicolon) {
    advanceToken(); // eat ';'
    return std::make_unique<ReturnStmt>(Loc, nullptr);
  }

  // Value return: return expr;
  auto ReturnExpr = parseExpr();
  if (!ReturnExpr) {
    return nullptr;
  }

  // Validate semicolon terminator
  if (peekToken().getKind() != TokenKind::Semicolon) {
    error("missing semicolon after return statement")
        .with_primary_label(spanFromToken(peekToken()), "expected `;` here")
        .with_help("return statements must end with a semicolon")
        .with_suggestion(spanFromToken(peekToken()), ";", "add semicolon")
        .emit(*DiagnosticsMan);
    return nullptr;
  }
  advanceToken();

  return std::make_unique<ReturnStmt>(Loc, std::move(ReturnExpr));
}

std::unique_ptr<DeferStmt> Parser::parseDeferStmt() {
  SrcLocation Loc = peekToken().getStart();
  advanceToken(); // eat 'return'

  // Value return: return expr;
  auto DeferredExpr = parseExpr();
  if (!DeferredExpr) {
    return nullptr;
  }

  // Validate semicolon terminator
  if (peekToken().getKind() != TokenKind::Semicolon) {
    error("missing semicolon after defer statement")
        .with_primary_label(spanFromToken(peekToken()), "expected `;` here")
        .with_help("defer statements must end with a semicolon")
        .with_suggestion(spanFromToken(peekToken()), ";", "add semicolon")
        .emit(*DiagnosticsMan);
    return nullptr;
  }
  advanceToken();

  return std::make_unique<DeferStmt>(Loc, std::move(DeferredExpr));
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
std::unique_ptr<IfStmt> Parser::parseIfStmt() {
  NoStructInit = true;
  SrcLocation Loc = peekToken().getStart();
  advanceToken(); // eat 'if'

  auto Cond = parseExpr();
  if (!Cond) {
    NoStructInit = false;
    return nullptr;
  }

  auto Body = parseBlock();
  if (!Body) {
    NoStructInit = false;
    return nullptr;
  }

  // No else clause
  if (peekToken().getKind() != TokenKind::ElseKwKind) {
    NoStructInit = false;
    return std::make_unique<IfStmt>(Loc, std::move(Cond), std::move(Body),
                                    nullptr);
  }

  // Else clause
  advanceToken(); // eat 'else'

  // Else block: else { ... }
  if (peekToken().getKind() == TokenKind::OpenBrace) {
    auto ElseBody = parseBlock();
    if (!ElseBody) {
      NoStructInit = false;
      return nullptr;
    }

    NoStructInit = false;
    return std::make_unique<IfStmt>(Loc, std::move(Cond), std::move(Body),
                                    std::move(ElseBody));
  }
  // Else if: else if ...
  if (peekToken().getKind() == TokenKind::IfKw) {
    std::vector<std::unique_ptr<Stmt>> ElifStmt;

    auto Res = parseIfStmt();
    if (!Res) {
      NoStructInit = false;
      return nullptr;
    }

    ElifStmt.emplace_back(std::move(Res));
    auto ElifBody = std::make_unique<Block>(std::move(ElifStmt));
    NoStructInit = false;
    return std::make_unique<IfStmt>(Loc, std::move(Cond), std::move(Body),
                                    std::move(ElifBody));
  }

  // Invalid else clause
  error("invalid else clause")
      .with_primary_label(spanFromToken(peekToken()), "unexpected token here")
      .with_help(
          "`else` must be followed by a block `{` or another `if` statement")
      .emit(*DiagnosticsMan);
  NoStructInit = false;
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
std::unique_ptr<WhileStmt> Parser::parseWhileStmt() {
  NoStructInit = true;
  SrcLocation loc = peekToken().getStart();
  advanceToken(); // eat 'while'

  auto Cond = parseExpr();
  if (!Cond) {
    NoStructInit = false;
    return nullptr;
  }

  auto Body = parseBlock();
  if (!Body) {
    NoStructInit = false;
    return nullptr;
  }

  NoStructInit = false;
  return std::make_unique<WhileStmt>(loc, std::move(Cond), std::move(Body));
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
std::unique_ptr<ForStmt> Parser::parseForStmt() {
  NoStructInit = true;
  SrcLocation Loc = peekToken().getStart();
  advanceToken(); // eat 'for'

  // Parse loop variable
  Token LoopVar = advanceToken();
  if (LoopVar.getKind() != TokenKind::Identifier) {
    error("for loop must have a loop variable")
        .with_primary_label(spanFromToken(LoopVar), "expected identifier here")
        .with_help("for loops have the form: `for variable in iterable`")
        .with_note(
            "the loop variable will be assigned each value from the iterable")
        .emit(*DiagnosticsMan);
    NoStructInit = false;
    return nullptr;
  }

  // Validate 'in' keyword
  Token InKw = advanceToken();
  if (InKw.getKind() != TokenKind::InKwKind) {
    error("missing `in` keyword in for loop")
        .with_primary_label(spanFromToken(LoopVar), "loop variable")
        .with_secondary_label(spanFromToken(InKw), "expected `in` here")
        .with_help("for loops have the form: `for variable in iterable`")
        .with_suggestion(spanFromToken(InKw), "in", "add `in` keyword")
        .emit(*DiagnosticsMan);
    NoStructInit = false;
    return nullptr;
  }

  // Parse range expression
  auto Range = parseExpr();
  if (!Range) {
    NoStructInit = false;
    return nullptr;
  }

  // Parse loop body
  auto Body = parseBlock();
  if (!Body) {
    NoStructInit = false;
    return nullptr;
  }

  // Create loop variable declaration (no type until inference)
  auto LoopVarDecl = std::make_unique<VarDecl>(
      LoopVar.getStart(), LoopVar.getLexeme(), std::nullopt, false, nullptr);

  NoStructInit = false;
  return std::make_unique<ForStmt>(Loc, std::move(LoopVarDecl),
                                   std::move(Range), std::move(Body));
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
std::unique_ptr<DeclStmt> Parser::parseDeclStmt() {
  SrcLocation letLoc = peekToken().getStart();
  bool IsConst;
  if (peekToken().getKind() == TokenKind::ConstKw) {
    IsConst = true;
    advanceToken();
  } else if (peekToken().getKind() == TokenKind::VarKw) {
    IsConst = false;
    advanceToken();
  } else {
    emitUnexpectedTokenError(peekToken(), {"var", "const"});
    return nullptr;
  }

  SrcLocation VarLoc;
  std::string Id;
  std::optional<Type> DeclType = std::nullopt; // initialize to nullptr

  if (peekToken(1).getKind() != TokenKind::Colon) {
    // just parse the name and leave DeclType as nullptr
    if (peekToken().getKind() != TokenKind::Identifier) {
      error("expected identifier")
          .with_primary_label(spanFromToken(peekToken()),
                              "expected identifier here")
          .emit(*DiagnosticsMan);
      return nullptr;
    }
    VarLoc = peekToken().getStart();
    Id = advanceToken().getLexeme();
  } else {
    auto Binding = parseTypedBinding();
    if (!Binding)
      return nullptr;

    // Assign to outer variables instead of redeclaring
    VarLoc = Binding->Loc;
    Id = Binding->Name;
    DeclType = Binding->Type;
  }

  // Validate assignment operator
  if (peekToken().getKind() != TokenKind::Equals) {
    error("missing assignment in variable declaration")
        .with_primary_label(spanFromToken(peekToken()), "expected `=` here")
        .with_help("variables must be initialized with a value")
        .with_note("variable syntax: `let name: type = value;`")
        .emit(*DiagnosticsMan);
    return nullptr;
  }
  advanceToken();

  // Parse initializer expression
  auto Init = parseExpr();
  if (!Init)
    return nullptr;

  // Validate semicolon terminator
  if (peekToken().getKind() != TokenKind::Semicolon) {
    error("missing semicolon after variable declaration")
        .with_primary_label(spanFromToken(peekToken()), "expected `;` here")
        .with_help("variable declarations must end with a semicolon")
        .with_suggestion(spanFromToken(peekToken()), ";", "add semicolon")
        .emit(*DiagnosticsMan);
    return nullptr;
  }
  advanceToken();

  return std::make_unique<DeclStmt>(
      letLoc, std::make_unique<VarDecl>(VarLoc, Id, DeclType, IsConst,
                                        std::move(Init)));
}

std::unique_ptr<BreakStmt> Parser::parseBreakStmt() {
  SrcLocation Loc = peekToken().getStart();
  if (advanceToken().getKind() != TokenKind::BreakKw) {
    error("missing break keyword")
        .with_primary_label(spanFromToken(peekToken()), "expected `break` here")
        .with_help("break statements must be preceded by a loop")
        .emit(*DiagnosticsMan);
    return nullptr;
  }

  // Validate semicolon terminator
  if (advanceToken().getKind() != TokenKind::Semicolon) {
    error("missing semicolon after break statement")
        .with_primary_label(spanFromToken(peekToken()), "expected `;` here")
        .with_help("break statements must end with a semicolon")
        .with_suggestion(spanFromToken(peekToken()), ";", "add semicolon")
        .emit(*DiagnosticsMan);
    return nullptr;
  }

  return std::make_unique<BreakStmt>(Loc);
}

std::unique_ptr<ContinueStmt> Parser::parseContinueStmt() {
  SrcLocation Loc = peekToken().getStart();
  if (advanceToken().getKind() != TokenKind::ContinueKw) {
    error("missing continue keyword")
        .with_primary_label(spanFromToken(peekToken()),
                            "expected `continue` here")
        .with_help("continue statements must be preceded by a loop")
        .with_code("E0028")
        .emit(*DiagnosticsMan);
    return nullptr;
  }

  // Validate semicolon terminator
  if (advanceToken().getKind() != TokenKind::Semicolon) {
    error("missing semicolon after continue statement")
        .with_primary_label(spanFromToken(peekToken()), "expected `;` here")
        .with_help("continue statements must end with a semicolon")
        .with_suggestion(spanFromToken(peekToken()), ";", "add semicolon")
        .emit(*DiagnosticsMan);
    return nullptr;
  }

  return std::make_unique<ContinueStmt>(Loc);
}

} // namespace phi
