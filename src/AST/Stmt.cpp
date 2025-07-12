#include "AST/Stmt.hpp"
#include "AST/Expr.hpp"
#include "AST/Stmt.hpp"
#include <print>

ReturnStmt::ReturnStmt(SrcLocation location, std::unique_ptr<Expr> expr)
    : Stmt(std::move(location)),
      expr(std::move(expr)) {}

ReturnStmt::~ReturnStmt() = default;

const Expr* ReturnStmt::get_expr() const { return expr.get(); }

void ReturnStmt::info_dump(int level) const {
    std::println("{}ReturnStmt", std::string(level * 2, ' '));
    if (expr) {
        expr->info_dump(level + 1);
    }
}
