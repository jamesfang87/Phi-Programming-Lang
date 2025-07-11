// global scope are functions
//

#include "ast.hpp"
#include <memory>
#include <optional>
#include <string>
#include <unordered_map>
#include <utility>
#include <vector>

class Sema {
public:
    Sema(std::vector<std::unique_ptr<FunctionDecl>> ast)
        : ast(std::move(ast)) {}

    std::vector<std::unique_ptr<ResolvedDecl>> resolve_ast();

private:
    std::vector<std::unique_ptr<FunctionDecl>> ast;
    std::vector<std::unordered_map<std::string, ResolvedDecl*>> active_scopes;

    std::pair<ResolvedDecl*, int> lookup_decl(const std::string& name);
    bool insert_decl(ResolvedDecl* decl);

    std::unique_ptr<ResolvedFunDecl> resolve_fun_decl(const FunctionDecl* fun);
    std::optional<Type> resolve_type(Type type);
    std::unique_ptr<ResolvedParamDecl>
    resolve_param_decl(const ParamDecl* param);

    std::unique_ptr<ResolvedBlock> resolve_block(const Block* block);
};
