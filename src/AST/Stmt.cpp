#include "AST/Stmt.hpp"

#include <memory>
#include <print>
#include <string>

#include "AST/Decl.hpp"
#include "AST/Expr.hpp"

namespace {
/// Generates indentation string for AST dumping
/// @param level Current indentation level
/// @return String of spaces for indentation
std::string indent(int level) { return std::string(level * 2, ' '); }
} // namespace

namespace phi {

//======================== ReturnStmt Implementation =========================//

/**
 * @brief Constructs a return statement
 *
 * @param location Source location of return
 * @param expr Return value expression (optional)
 */
ReturnStmt::ReturnStmt(SrcLocation location, std::unique_ptr<Expr> expr)
    : Stmt(Kind::ReturnStmtKind, std::move(location)), expr(std::move(expr)) {}

ReturnStmt::~ReturnStmt() = default;

/**
 * @brief Dumps return statement information
 *
 * Output format:
 *   [indent]ReturnStmt
 *     [child expression dump] (if exists)
 *
 * @param level Current indentation level
 */
void ReturnStmt::emit(int level) const {
  std::println("{}ReturnStmt", indent(level));
  if (expr) {
    expr->emit(level + 1);
  }
}

//======================== IfStmt Implementation =========================//

/**
 * @brief Constructs an if statement
 *
 * @param location Source location of if
 * @param cond condal expression
 * @param then_block Then clause block
 * @param else_block Else clause block (optional)
 */
IfStmt::IfStmt(SrcLocation location, std::unique_ptr<Expr> cond,
               std::unique_ptr<Block> then_block,
               std::unique_ptr<Block> else_block)
    : Stmt(Stmt::Kind::IfStmtKind, std::move(location)), cond(std::move(cond)),
      thenBlock(std::move(then_block)), elseBlock(std::move(else_block)) {}

IfStmt::~IfStmt() = default;

/**
 * @brief Dumps if statement information
 *
 * Output format:
 *   [indent]IfStmt
 *     [cond dump]
 *     [then block dump]
 *     [else block dump] (if exists)
 *
 * @param level Current indentation level
 */
void IfStmt::emit(int level) const {
  std::println("{}IfStmt", indent(level));
  if (cond) {
    cond->emit(level + 1);
  }
  if (thenBlock) {
    thenBlock->emit(level + 1);
  }
  if (elseBlock) {
    elseBlock->emit(level + 1);
  }
}

//======================== WhileStmt Implementation =========================//

/**
 * @brief Constructs a while loop statement
 *
 * @param location Source location of while
 * @param cond Loop cond expression
 * @param body Loop body block
 */
WhileStmt::WhileStmt(SrcLocation location, std::unique_ptr<Expr> cond,
                     std::unique_ptr<Block> body)
    : Stmt(Stmt::Kind::WhileStmtKind, std::move(location)),
      cond(std::move(cond)), body(std::move(body)) {}

WhileStmt::~WhileStmt() = default;

/**
 * @brief Dumps while loop information
 *
 * Output format:
 *   [indent]WhileStmt
 *     [cond dump]
 *     [body dump]
 *
 * @param level Current indentation level
 */
void WhileStmt::emit(int level) const {
  std::println("{}WhileStmt", indent(level));
  if (cond) {
    cond->emit(level + 1);
  }
  if (body) {
    body->emit(level + 1);
  }
}

//======================== ForStmt Implementation =========================//

/**
 * @brief Constructs a for loop statement
 *
 * @param location Source location of for
 * @param loop_var Loop variable declaration
 * @param range Range expression
 * @param body Loop body block
 */
ForStmt::ForStmt(SrcLocation location, std::unique_ptr<VarDecl> loop_var,
                 std::unique_ptr<Expr> range, std::unique_ptr<Block> body)
    : Stmt(Stmt::Kind::ForStmtKind, std::move(location)),
      loopVar(std::move(loop_var)), range(std::move(range)),
      body(std::move(body)) {}

ForStmt::~ForStmt() = default;

/**
 * @brief Dumps for loop information
 *
 * Output format:
 *   [indent]ForStmt
 *     [loop variable dump]
 *     [range expression dump]
 *     [body dump]
 *
 * @param level Current indentation level
 */
void ForStmt::emit(int level) const {
  std::println("{}ForStmt", indent(level));
  if (loopVar) {
    loopVar->emit(level + 1);
  }
  if (range) {
    range->emit(level + 1);
  }
  if (body) {
    body->emit(level + 1);
  }
}

//======================== LetStmt Implementation =========================//

/**
 * @brief Constructs a variable declaration statement
 *
 * @param location Source location of declaration
 * @param decl Variable declaration
 */
DeclStmt::DeclStmt(SrcLocation location, std::unique_ptr<VarDecl> decl)
    : Stmt(Stmt::Kind::DeclStmtKind, std::move(location)),
      decl(std::move(decl)) {}

DeclStmt::~DeclStmt() = default;

/**
 * @brief Dumps variable declaration statement information
 *
 * Output format:
 *   [indent]VarDeclStmt
 *     [variable declaration dump]
 *
 * @param level Current indentation level
 */
void DeclStmt::emit(int level) const {
  std::println("{}VarDeclStmt", indent(level));
  if (decl) {
    decl->emit(level + 1);
  }
}

//======================== BreakStmt Implementation =========================//

/**
 * @brief Constructs a break statement
 *
 * @param location Source location of break statement
 */
BreakStmt::BreakStmt(SrcLocation location)
    : Stmt(Stmt::Kind::BreakStmtKind, std::move(location)) {}

BreakStmt::~BreakStmt() = default;

/**
 * @brief Dumps break statement information
 *
 * Output format:
 *   [indent]BreakStmt
 *
 * @param level Current indentation level
 */
void BreakStmt::emit(int level) const {
  std::println("{}BreakStmt", indent(level));
}

//======================= ContinueStmt Implementation ========================//

/**
 * @brief Constructs a continue statement
 *
 * @param location Source location of continue statement
 */
ContinueStmt::ContinueStmt(SrcLocation location)
    : Stmt(Stmt::Kind::ContinueStmtKind, std::move(location)) {}

ContinueStmt::~ContinueStmt() = default;

/**
 * @brief Dumps continue statement information
 *
 * Output format:
 *   [indent]ContinueStmt
 *
 * @param level Current indentation level
 */
void ContinueStmt::emit(int level) const {
  std::println("{}ContinueStmt", indent(level));
}

//======================== Block Implementation =========================//

/**
 * @brief Dumps block statement information
 *
 * Output format:
 *   [indent]Block
 *     [statement dumps]
 *
 * @param level Current indentation level
 */
void Block::emit(int level) const {
  std::println("{}Block", indent(level));
  for (auto &s : this->stmts) {
    s->emit(level + 1);
  }
}

} // namespace phi
