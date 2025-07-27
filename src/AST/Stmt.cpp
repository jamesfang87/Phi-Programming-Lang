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

//======================== ReturnStmt Implementation =========================//

/**
 * @brief Constructs a return statement
 *
 * @param location Source location of return
 * @param expr Return value expression (optional)
 */
ReturnStmt::ReturnStmt(SrcLocation location, std::unique_ptr<Expr> expr)
    : Stmt(std::move(location)),
      expr(std::move(expr)) {}

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
void ReturnStmt::info_dump(int level) const {
    std::println("{}ReturnStmt", indent(level));
    if (expr) {
        expr->info_dump(level + 1);
    }
}

//======================== IfStmt Implementation =========================//

/**
 * @brief Constructs an if statement
 *
 * @param location Source location of if
 * @param condition Conditional expression
 * @param then_block Then clause block
 * @param else_block Else clause block (optional)
 */
IfStmt::IfStmt(SrcLocation location,
               std::unique_ptr<Expr> condition,
               std::unique_ptr<Block> then_block,
               std::unique_ptr<Block> else_block)
    : Stmt(std::move(location)),
      condition(std::move(condition)),
      then_block(std::move(then_block)),
      else_block(std::move(else_block)) {}

IfStmt::~IfStmt() = default;

/**
 * @brief Dumps if statement information
 *
 * Output format:
 *   [indent]IfStmt
 *     [condition dump]
 *     [then block dump]
 *     [else block dump] (if exists)
 *
 * @param level Current indentation level
 */
void IfStmt::info_dump(int level) const {
    std::println("{}IfStmt", indent(level));
    if (condition) {
        condition->info_dump(level + 1);
    }
    if (then_block) {
        then_block->info_dump(level + 1);
    }
    if (else_block) {
        else_block->info_dump(level + 1);
    }
}

//======================== WhileStmt Implementation =========================//

/**
 * @brief Constructs a while loop statement
 *
 * @param location Source location of while
 * @param condition Loop condition expression
 * @param body Loop body block
 */
WhileStmt::WhileStmt(SrcLocation location,
                     std::unique_ptr<Expr> condition,
                     std::unique_ptr<Block> body)
    : Stmt(std::move(location)),
      condition(std::move(condition)),
      body(std::move(body)) {}

WhileStmt::~WhileStmt() = default;

/**
 * @brief Dumps while loop information
 *
 * Output format:
 *   [indent]WhileStmt
 *     [condition dump]
 *     [body dump]
 *
 * @param level Current indentation level
 */
void WhileStmt::info_dump(int level) const {
    std::println("{}WhileStmt", indent(level));
    if (condition) {
        condition->info_dump(level + 1);
    }
    if (body) {
        body->info_dump(level + 1);
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
ForStmt::ForStmt(SrcLocation location,
                 std::unique_ptr<VarDecl> loop_var,
                 std::unique_ptr<Expr> range,
                 std::unique_ptr<Block> body)
    : Stmt(std::move(location)),
      loop_var(std::move(loop_var)),
      range(std::move(range)),
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
void ForStmt::info_dump(int level) const {
    std::println("{}ForStmt", indent(level));
    if (loop_var) {
        loop_var->info_dump(level + 1);
    }
    if (range) {
        range->info_dump(level + 1);
    }
    if (body) {
        body->info_dump(level + 1);
    }
}

//======================== LetStmt Implementation =========================//

/**
 * @brief Constructs a variable declaration statement
 *
 * @param location Source location of declaration
 * @param decl Variable declaration
 */
LetStmt::LetStmt(SrcLocation location, std::unique_ptr<VarDecl> decl)
    : Stmt(std::move(location)),
      var_decl(std::move(decl)) {}

LetStmt::~LetStmt() = default;

/**
 * @brief Dumps variable declaration statement information
 *
 * Output format:
 *   [indent]VarDeclStmt
 *     [variable declaration dump]
 *
 * @param level Current indentation level
 */
void LetStmt::info_dump(int level) const {
    std::println("{}VarDeclStmt", indent(level));
    if (var_decl) {
        var_decl->info_dump(level + 1);
    }
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
void Block::info_dump(int level) const {
    std::println("{}Block", indent(level));
    for (auto& s : this->stmts) {
        s->info_dump(level + 1);
    }
}
