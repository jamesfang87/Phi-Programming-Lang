#include "AST/Stmt.hpp"

#include <print>

#include "AST/Decl.hpp"
#include "AST/Expr.hpp"

ReturnStmt::ReturnStmt(SrcLocation location, std::unique_ptr<Expr> expr)
    : Stmt(std::move(location)),
      expr(std::move(expr)) {}

ReturnStmt::~ReturnStmt() = default;

Expr* ReturnStmt::get_expr() { return expr.get(); }

void ReturnStmt::info_dump(int level) const {
    std::println("{}ReturnStmt", std::string(level * 2, ' '));
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

Expr& IfStmt::get_condition() { return *condition; }

Block& IfStmt::get_true_body() { return *true_body; }

Block& IfStmt::get_false_body() { return *false_body; }

void IfStmt::info_dump(int level) const {
    std::println("{}IfStmt", std::string(level * 2, ' '));
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

Expr& WhileStmt::get_condition() { return *condition; }

Block& WhileStmt::get_body() { return *body; }

void WhileStmt::info_dump(int level) const {
    std::println("{}WhileStmt", std::string(level * 2, ' '));
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

Expr& ForStmt::get_range() { return *range; }

Block& ForStmt::get_body() { return *body; }

void ForStmt::info_dump(int level) const {
    std::println("{}ForStmt", std::string(level * 2, ' '));
    if (!loop_var.empty()) {
        std::println("{}Loop variable: {}", std::string(level * 2 + 2, ' '), loop_var);
    }
    if (range) {
        range->info_dump(level + 1);
    }
    if (body) {
        body->info_dump(level + 1);
    }
}

VarDeclStmt::VarDeclStmt(SrcLocation location, std::unique_ptr<VarDecl> decl)
    : Stmt(std::move(location)),
      var_decl(std::move(decl)) {}

VarDeclStmt::~VarDeclStmt() = default;

void VarDeclStmt::info_dump(int level) const {
    std::println("{}VarDeclStmt", std::string(level * 2, ' '));
    if (var_decl) {
        var_decl->info_dump(level + 1);
    }
}
