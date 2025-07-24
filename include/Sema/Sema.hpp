#pragma once

#include <memory>
#include <optional>
#include <string>
#include <unordered_map>
#include <utility>
#include <vector>

#include "AST/ASTVisitor.hpp"
#include "AST/Decl.hpp"
#include "AST/Type.hpp"
#include "Sema/SymbolTable.hpp"

class Sema final : public ASTVisitor {
public:
    explicit Sema(std::vector<std::unique_ptr<FunDecl>> ast)
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
    bool visit(LetStmt& stmt) override;
    bool visit(Expr& stmt) override;

private:
    std::vector<std::unique_ptr<FunDecl>> ast;
    SymbolTable symbol_table;
    std::vector<std::unordered_map<std::string, Decl*>> active_scopes;

    FunDecl* cur_fun = nullptr;
    bool is_function_body_block = false; // Flag to indicate if the next block is a function body

    bool resolve_decl_ref(DeclRefExpr* declref, bool function_call);
    bool resolve_function_call(FunCallExpr* call);

    static bool resolve_fun_decl(FunDecl* fun);
    static std::optional<Type> resolve_type(Type type);
    static bool resolve_param_decl(ParamDecl* param);

    // Type checking helper functions
    static bool is_integer_type(const Type& type);
    static bool is_numeric_type(const Type& type);
    static Type promote_numeric_types(const Type& lhs, const Type& rhs);
};
