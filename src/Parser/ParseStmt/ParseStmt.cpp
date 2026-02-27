#include "AST/Nodes/Stmt.hpp"
#include "Diagnostics/DiagnosticBuilder.hpp"
#include "Parser/Parser.hpp"

#include <cassert>
#include <experimental/scope>
#include <format>
#include <memory>
#include <optional>
#include <string>
#include <vector>

#include "AST/Nodes/Decl.hpp"
#include "AST/TypeSystem/Type.hpp"
#include "Lexer/TokenKind.hpp"
#include "SrcManager/SrcSpan.hpp"

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
  case TokenKind::UseKw:
    return parseUseStmt();

  default:
    auto Res = parseExpr();
    if (!Res)
      return nullptr;
    if (!matchToken(TokenKind::Semicolon)) {
      error("missing semicolon after statement")
          .with_primary_label(SrcSpan(peekToken(-1).getEnd()),
                              "expected `;` here")
          .with_help("statements must end with a semicolon")
          .with_suggestion(SrcSpan(peekToken(-1).getEnd()), ";",
                           "add semicolon")
          .emit(*Diags);
    }
    return std::make_unique<ExprStmt>(Res->getLocation(), std::move(Res));
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

  if (matchToken(TokenKind::Semicolon)) {
    return std::make_unique<ReturnStmt>(Loc, nullptr);
  }

  // Value return: return expr;
  auto ReturnExpr = parseExpr();
  if (!ReturnExpr) {
    return nullptr;
  }

  // Validate semicolon terminator
  if (!matchToken(TokenKind::Semicolon)) {
    error("missing semicolon after return statement")
        .with_primary_label(SrcSpan(peekToken(-1).getEnd()),
                            "expected `;` here")
        .with_help("return statements must end with a semicolon")
        .with_suggestion(SrcSpan(peekToken(-1).getEnd()), ";", "add semicolon")
        .emit(*Diags);
    return nullptr;
  }

  return std::make_unique<ReturnStmt>(Loc, std::move(ReturnExpr));
}

std::unique_ptr<DeferStmt> Parser::parseDeferStmt() {
  SrcLocation Loc = advanceToken().getStart();

  // Value return: return expr;
  auto DeferredExpr = parseExpr();
  if (!DeferredExpr) {
    return nullptr;
  }

  // Validate semicolon terminator
  if (!matchToken(TokenKind::Semicolon)) {
    error("missing semicolon after defer statement")
        .with_primary_label(SrcSpan(peekToken(-1).getEnd()),
                            "expected `;` here")
        .with_help("defer statements must end with a semicolon")
        .with_suggestion(SrcSpan(peekToken(-1).getEnd()), ";", "add semicolon")
        .emit(*Diags);
    return nullptr;
  }

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
  SrcLocation Loc = advanceToken().getStart();

  std::unique_ptr<Expr> Cond;

  {
    NoAdtInit = true;
    auto _ = std::experimental::scope_exit([&] { NoAdtInit = false; });

    Cond = parseExpr();
    if (!Cond) {
      return nullptr;
    }
  } // NoAdtInit set back to false here

  auto Body = parseBlock();
  if (!Body) {
    return nullptr;
  }

  // No else clause
  if (!matchToken(TokenKind::ElseKw)) {
    return std::make_unique<IfStmt>(Loc, std::move(Cond), std::move(Body),
                                    nullptr);
  }

  // Else block: else { ... }
  if (peekKind() == TokenKind::OpenBrace) {
    auto ElseBody = parseBlock();
    if (!ElseBody) {
      return nullptr;
    }
    return std::make_unique<IfStmt>(Loc, std::move(Cond), std::move(Body),
                                    std::move(ElseBody));
  }

  // Else if: else if ...
  if (peekKind() == TokenKind::IfKw) {
    std::vector<std::unique_ptr<Stmt>> ElifStmt;
    auto Res = parseIfStmt(); // Recursive call handles its own NoAdtInit
    if (!Res) {
      return nullptr;
    }
    ElifStmt.emplace_back(std::move(Res));
    auto ElifBody = std::make_unique<Block>(std::move(ElifStmt));
    return std::make_unique<IfStmt>(Loc, std::move(Cond), std::move(Body),
                                    std::move(ElifBody));
  }

  // Invalid else clause
  error("invalid else clause")
      .with_primary_label(peekToken().getSpan(), "unexpected token here")
      .with_help(
          "`else` must be followed by a block `{` or another `if` statement")
      .emit(*Diags);
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
  SrcLocation Loc = peekToken().getStart();
  advanceToken(); // eat 'while'

  std::unique_ptr<Expr> Cond;

  {
    NoAdtInit = true;
    auto _ = std::experimental::scope_exit([&] { NoAdtInit = false; });

    Cond = parseExpr();
    if (!Cond) {
      return nullptr;
    }
  } // NoAdtInit set back to false here

  auto Body = parseBlock();
  if (!Body) {
    return nullptr;
  }

  return std::make_unique<WhileStmt>(Loc, std::move(Cond), std::move(Body));
}

std::unique_ptr<ForStmt> Parser::parseForStmt() {
  SrcLocation Loc = advanceToken().getStart();

  std::unique_ptr<VarDecl> LoopVarDecl;
  std::unique_ptr<Expr> Range;

  {
    NoAdtInit = true;
    auto _ = std::experimental::scope_exit([&] { NoAdtInit = false; });

    // Parse loop variable
    Token LoopVar = advanceToken();
    if (LoopVar.getKind() != TokenKind::Identifier) {
      error("for loop must have a loop variable")
          .with_primary_label(LoopVar.getSpan(), "expected identifier here")
          .with_help("for loops have the form: `for variable in iterable`")
          .with_note(
              "the loop variable will be assigned each value from the iterable")
          .emit(*Diags);
      return nullptr;
    }

    // Validate 'in' keyword
    Token InKw = advanceToken();
    if (InKw.getKind() != TokenKind::InKw) {
      error("missing `in` keyword in for loop")
          .with_primary_label(LoopVar.getSpan(), "loop variable")
          .with_secondary_label(InKw.getSpan(), "expected `in` here")
          .with_help("for loops have the form: `for variable in iterable`")
          .with_suggestion(InKw.getSpan(), "in", "add `in` keyword")
          .emit(*Diags);
      return nullptr;
    }

    // Parse range expression
    Range = parseExpr();
    if (!Range) {
      return nullptr;
    }

    // Create loop variable declaration
    LoopVarDecl = std::make_unique<VarDecl>(
        LoopVar.getSpan(), Mutability::Var, LoopVar.getLexeme(),
        TypeCtx::getVar(VarTy::Domain::Int, LoopVar.getSpan()), nullptr);
  } // scope_exit destructs here, setting NoAdtInit = false

  // Parse loop body (NoAdtInit is now false)
  auto Body = parseBlock();
  if (!Body) {
    return nullptr;
  }

  return std::make_unique<ForStmt>(Loc, std::move(LoopVarDecl),
                                   std::move(Range), std::move(Body));
}

/**
 * Parses a variable declaration (let statement).
 *
 * @return std::unique_ptr<LetStmt> Variable declaration AST or nullptr on
 * error. Errors are emitted to DiagnosticManager.
 *
 * Validates:
 * - Colon after identifier
 * - Valid type annotation
 * - Assignment operator
 * - Initializer expression
 * - Semicolon terminator
 */
std::unique_ptr<DeclStmt> Parser::parseDeclStmt() {
  SrcLocation StartLoc = peekToken().getStart();

  auto Mutability = parseMutability();
  if (!Mutability)
    return nullptr;

  auto Var = parseBinding(
      {.Type = Optional, .Init = Optional, .AllowPlaceholderForType = true});
  if (!Var)
    return nullptr;

  expectToken(TokenKind::Semicolon);

  auto &[Span, Id, Type, Init] = *Var;
  return std::make_unique<DeclStmt>(
      StartLoc,
      std::make_unique<VarDecl>(Span, *Mutability, Id, Type, std::move(Init)));
}

std::unique_ptr<BreakStmt> Parser::parseBreakStmt() {
  SrcLocation Loc = peekToken().getStart();
  assert(advanceToken().getKind() == TokenKind::BreakKw);

  // Validate semicolon terminator
  if (!matchToken(TokenKind::Semicolon)) {
    error("missing semicolon after break statement")
        .with_primary_label(SrcSpan(peekToken(-1).getEnd()),
                            "expected `;` here")
        .with_help("break statements must end with a semicolon")
        .with_suggestion(SrcSpan(peekToken(-1).getEnd()), ";", "add semicolon")
        .emit(*Diags);
    return nullptr;
  }

  return std::make_unique<BreakStmt>(Loc);
}

std::unique_ptr<ContinueStmt> Parser::parseContinueStmt() {
  SrcLocation Loc = peekToken().getStart();
  assert(advanceToken().getKind() == TokenKind::ContinueKw);

  // Validate semicolon terminator
  if (!matchToken(TokenKind::Semicolon)) {
    error("missing semicolon after continue statement")
        .with_primary_label(SrcSpan(peekToken(-1).getEnd()),
                            "expected `;` here")
        .with_help("continue statements must end with a semicolon")
        .with_suggestion(SrcSpan(peekToken(-1).getEnd()), ";", "add semicolon")
        .emit(*Diags);
    return nullptr;
  }

  return std::make_unique<ContinueStmt>(Loc);
}

std::unique_ptr<ImportStmt> Parser::parseImportStmt() {
  SrcLocation Loc = peekToken().getStart();
  assert(advanceToken().getKind() == TokenKind::ImportKw);
  auto Res = parseModulePath();
  if (!Res)
    return nullptr;

  std::optional<std::string> Alias = std::nullopt;
  if (matchToken(TokenKind::AsKw)) {
    if (expectToken(TokenKind::Identifier)) {
      Alias = peekToken(-1).getLexeme();
    }
  }

  if (!matchToken(TokenKind::Semicolon)) {
    error("missing semicolon after import statement")
        .with_primary_label(SrcSpan(peekToken(-1).getEnd()),
                            "expected `;` here")
        .with_help("import statements must end with a semicolon")
        .with_suggestion(SrcSpan(peekToken(-1).getEnd()), ";", "add semicolon")
        .emit(*Diags);
    return nullptr;
  }

  auto &[PathStr, Path, Span] = *Res;
  return std::make_unique<ImportStmt>(Loc, PathStr, Path, Alias);
}

std::unique_ptr<UseStmt> Parser::parseUseStmt() {
  SrcLocation Loc = peekToken().getStart();
  assert(advanceToken().getKind() == TokenKind::UseKw);

  Type *T = nullptr;

  switch (peekKind()) {
  case TokenKind::I8:
    T = TypeCtx::getBuiltin(BuiltinTy::i8, peekToken().getSpan()).getPtr();
    break;
  case TokenKind::I16:
    T = TypeCtx::getBuiltin(BuiltinTy::i16, peekToken().getSpan()).getPtr();
    break;
  case TokenKind::I32:
    T = TypeCtx::getBuiltin(BuiltinTy::i32, peekToken().getSpan()).getPtr();
    break;
  case TokenKind::I64:
    T = TypeCtx::getBuiltin(BuiltinTy::i64, peekToken().getSpan()).getPtr();
    break;
  case TokenKind::U8:
    T = TypeCtx::getBuiltin(BuiltinTy::u8, peekToken().getSpan()).getPtr();
    break;
  case TokenKind::U16:
    T = TypeCtx::getBuiltin(BuiltinTy::u16, peekToken().getSpan()).getPtr();
    break;
  case TokenKind::U32:
    T = TypeCtx::getBuiltin(BuiltinTy::u32, peekToken().getSpan()).getPtr();
    break;
  case TokenKind::U64:
    T = TypeCtx::getBuiltin(BuiltinTy::u64, peekToken().getSpan()).getPtr();
    break;
  case TokenKind::F32:
    T = TypeCtx::getBuiltin(BuiltinTy::f32, peekToken().getSpan()).getPtr();
    break;
  case TokenKind::F64:
    T = TypeCtx::getBuiltin(BuiltinTy::f64, peekToken().getSpan()).getPtr();
    break;
  case TokenKind::String:
    T = TypeCtx::getBuiltin(BuiltinTy::String, peekToken().getSpan()).getPtr();
    break;
  case TokenKind::Char:
    T = TypeCtx::getBuiltin(BuiltinTy::Char, peekToken().getSpan()).getPtr();
    break;
  case TokenKind::BoolKw:
    T = TypeCtx::getBuiltin(BuiltinTy::Bool, peekToken().getSpan()).getPtr();
    break;
  default:
    break;
  }

  if (T) {
    auto TypeTok = advanceToken(); // consume the type token
    std::string Alias;
    if (peekKind() == TokenKind::AsKw) {
      advanceToken(); // consume 'as'
      if (expectToken(TokenKind::Identifier)) {
        Alias = peekToken(-1).getLexeme();
      } else {
        return nullptr;
      }
    }

    if (!Alias.empty() && BuiltinTyAliases.contains(Alias)) {
      error(std::format("Redefinition of type alias `{}`", Alias))
          .with_primary_label(peekToken(-1).getSpan(), "redefinition here")
          .emit(*Diags);
      return nullptr;
    }

    if (!matchToken(TokenKind::Semicolon)) {
      error("missing semicolon after use statement")
          .with_primary_label(SrcSpan(peekToken(-1).getEnd()),
                              "expected `;` here")
          .with_help("use statements must end with a semicolon")
          .with_suggestion(SrcSpan(peekToken(-1).getEnd()), ";",
                           "add semicolon")
          .emit(*Diags);
      return nullptr;
    }

    if (!Alias.empty())
      BuiltinTyAliases[Alias] = T;

    return std::make_unique<UseStmt>(
        Loc, TypeTok.getLexeme(), std::vector<std::string>{TypeTok.getLexeme()},
        Alias);
  }

  // Module path import
  auto Res = parseModulePath();
  if (!Res)
    return nullptr;

  std::string Alias;
  if (matchToken(TokenKind::AsKw)) {
    if (expectToken(TokenKind::Identifier)) {
      Alias = peekToken(-1).getLexeme();
    }
  }

  if (!matchToken(TokenKind::Semicolon)) {
    error("missing semicolon after use statement")
        .with_primary_label(SrcSpan(peekToken(-1).getEnd()),
                            "expected `;` here")
        .with_help("use statements must end with a semicolon")
        .with_suggestion(SrcSpan(peekToken(-1).getEnd()), ";", "add semicolon")
        .emit(*Diags);
    return nullptr;
  }

  auto &[PathStr, Path, Span] = *Res;
  return std::make_unique<UseStmt>(Loc, PathStr, Path, Alias);
}

} // namespace phi
