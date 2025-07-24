#include "AST/Stmt.hpp"

#include <memory>
#include <print>
#include <string>

#include "AST/Decl.hpp"
#include "AST/Expr.hpp"

namespace {
std::string indent(int level) { return std::string(level * 2, ' '); }
} // namespace

ReturnStmt::ReturnStmt(SrcLocation location, std::unique_ptr<Expr> expr)
    : Stmt(std::move(location)),
      expr(std::move(expr)) {}

ReturnStmt::~ReturnStmt() = default;

Expr* ReturnStmt::get_expr() const { return expr.get(); }

void ReturnStmt::info_dump(int level) const {
    std::println("{}ReturnStmt", indent(level));
    if (expr) {
        expr->info_dump(level + 1);
    }
}

IfStmt::IfStmt(SrcLocation location,
               std::unique_ptr<Expr> condition,
               std::unique_ptr<Block> true_body,
               std::unique_ptr<Block> false_body)
    : Stmt(std::move(location)),
      condition(std::move(condition)),
      true_body(std::move(true_body)),
      false_body(std::move(false_body)) {}

IfStmt::~IfStmt() = default;

Expr& IfStmt::get_condition() const { return *condition; }

Block& IfStmt::get_true_body() const { return *true_body; }

Block& IfStmt::get_false_body() const { return *false_body; }

void IfStmt::info_dump(int level) const {
    std::println("{}IfStmt", indent(level));
    if (condition) {
        condition->info_dump(level + 1);
    }
    if (true_body) {
        true_body->info_dump(level + 1);
    }
    if (false_body) {
        false_body->info_dump(level + 1);
    }
}

WhileStmt::WhileStmt(SrcLocation location,
                     std::unique_ptr<Expr> condition,
                     std::unique_ptr<Block> body)
    : Stmt(std::move(location)),
      condition(std::move(condition)),
      body(std::move(body)) {}

WhileStmt::~WhileStmt() = default;

Expr& WhileStmt::get_condition() const { return *condition; }

Block& WhileStmt::get_body() const { return *body; }

void WhileStmt::info_dump(int level) const {
    std::println("{}WhileStmt", indent(level));
    if (condition) {
        condition->info_dump(level + 1);
    }
    if (body) {
        body->info_dump(level + 1);
    }
}

ForStmt::ForStmt(SrcLocation location,
                 std::string loop_var,
                 std::unique_ptr<Expr> range,
                 std::unique_ptr<Block> body)
    : Stmt(std::move(location)),
      loop_var(std::move(loop_var)),
      range(std::move(range)),
      body(std::move(body)) {}

ForStmt::~ForStmt() = default;

std::string& ForStmt::get_loop_var() { return loop_var; }

Expr& ForStmt::get_range() const { return *range; }

Block& ForStmt::get_body() const { return *body; }

void ForStmt::info_dump(int level) const {
    std::println("{}ForStmt", indent(level));
    if (!loop_var.empty()) {
        std::println("{}Loop variable: {}", indent(level + 1), loop_var);
    }
    if (range) {
        range->info_dump(level + 1);
    }
    if (body) {
        body->info_dump(level + 1);
    }
}

LetStmt::LetStmt(SrcLocation location, std::unique_ptr<VarDecl> decl)
    : Stmt(std::move(location)),
      var_decl(std::move(decl)) {}

LetStmt::~LetStmt() = default;

void LetStmt::info_dump(int level) const {
    std::println("{}VarDeclStmt", indent(level));
    if (var_decl) {
        var_decl->info_dump(level + 1);
    }
}
