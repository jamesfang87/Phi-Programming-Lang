#include "AST/ASTVisitor.hpp"
#include "AST/Decl.hpp"
#include "AST/Expr.hpp"
#include "AST/Stmt.hpp"
#include "AST/Type.hpp"
#include <memory>
#include <optional>
#include <string>
#include <unordered_map>
#include <utility>
#include <vector>

#pragma once

class ScopeGuard {
public:
    ScopeGuard(std::vector<std::unordered_map<std::string, Decl*>>& scopes)
        : scopes(scopes) {
        scopes.emplace_back();
    }

    ~ScopeGuard() { scopes.pop_back(); }

    // Non-copyable, non-movable
    ScopeGuard(const ScopeGuard&) = delete;
    ScopeGuard& operator=(const ScopeGuard&) = delete;
    ScopeGuard(ScopeGuard&&) = delete;
    ScopeGuard& operator=(ScopeGuard&&) = delete;

private:
    std::vector<std::unordered_map<std::string, Decl*>>& scopes;
};

class Sema : public ASTVisitor {
public:
    Sema(std::vector<std::unique_ptr<FunDecl>> ast)
        : ast(std::move(ast)) {}

    std::pair<bool, std::vector<std::unique_ptr<FunDecl>>> resolve_ast();

    // ASTVisitor interface - Expression visitors
    bool visit(IntLiteral& expr) override;
    bool visit(FloatLiteral& expr) override;
    bool visit(StrLiteral& expr) override;
    bool visit(CharLiteral& expr) override;
    bool visit(BoolLiteral& expr) override;
    bool visit(RangeLiteral& expr) override;
    bool visit(DeclRefExpr& expr) override;
    bool visit(FunCallExpr& expr) override;
    bool visit(BinaryOp& expr) override;
    bool visit(UnaryOp& expr) override;

    // ASTVisitor interface - Statement visitors
    bool visit(Block& block) override;
    bool visit(ReturnStmt& stmt) override;
    bool visit(IfStmt& stmt) override;
    bool visit(WhileStmt& stmt) override;
    bool visit(ForStmt& stmt) override;
    bool visit(VarDeclStmt& stmt) override;
    bool visit(Expr& stmt) override;

private:
    std::vector<std::unique_ptr<FunDecl>> ast;
    std::vector<std::unordered_map<std::string, Decl*>> active_scopes;

    FunDecl* cur_fun;
    bool is_function_body_block = false; // Flag to indicate if the next block is a function body

    Decl* lookup_decl(const std::string& name);
    bool insert_decl(Decl* decl);

    bool resolve_decl_ref(DeclRefExpr* declref, bool function_call);
    bool resolve_function_call(FunCallExpr* call);
    bool resolve_return_stmt(ReturnStmt* stmt);

    bool resolve_fun_decl(FunDecl* fun);
    bool resolve_var_decl(VarDecl* var);
    std::optional<Type> resolve_type(Type type);
    bool resolve_param_decl(ParamDecl* param);

    // Type checking helper functions
    bool is_integer_type(const Type& type);
    bool is_numeric_type(const Type& type);
    Type promote_numeric_types(const Type& lhs, const Type& rhs);
};
