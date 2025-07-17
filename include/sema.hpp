// global scope are functions
//

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

class Sema {
public:
    Sema(std::vector<std::unique_ptr<FunDecl>> ast)
        : ast(std::move(ast)) {}

    std::pair<bool, std::vector<std::unique_ptr<FunDecl>>> resolve_ast();

private:
    std::vector<std::unique_ptr<FunDecl>> ast;
    std::vector<std::unordered_map<std::string, Decl*>> active_scopes;

    Decl* lookup_decl(const std::string& name);
    bool insert_decl(Decl* decl);

    bool resolve_expr(Expr* expr);

    bool resolve_return_stmt(ReturnStmt* stmt);

    bool resolve_decl_ref(DeclRefExpr* declref, bool function_call);
    bool resolve_function_call(FunCallExpr* call);

    bool resolve_stmt(Stmt* stmt);
    bool resolve_fun_decl(FunDecl* fun);
    std::optional<Type> resolve_type(Type type);
    bool resolve_param_decl(ParamDecl* param);

    bool resolve_block(Block* block);

    FunDecl* cur_fun;
};
