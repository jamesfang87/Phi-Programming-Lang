#include "AST/Expr.hpp"
#include "AST/Stmt.hpp"
#include "SrcLocation.hpp"
#include "Type.hpp"
#include <memory>
#include <optional>
#include <string>
#include <vector>

#pragma once

class Decl {
public:
    Decl(SrcLocation location, std::string identifier, Type type)
        : location(std::move(location)),
          identifier(std::move(identifier)),
          type(std::move(type)) {}

    virtual ~Decl() = default;

    virtual void info_dump(int level = 0) const = 0;

    [[nodiscard]] std::string& get_id() { return identifier; }
    [[nodiscard]] SrcLocation& get_location() { return location; }
    [[nodiscard]] Type& get_type() { return type.value(); }

protected:
    SrcLocation location;
    std::string identifier;
    std::optional<Type> type;
};

class VarDecl : public Decl {
public:
    VarDecl(SrcLocation location,
            std::string identifier,
            Type type,
            bool is_const,
            std::unique_ptr<Expr> initializer)
        : Decl(std::move(location), std::move(identifier), std::move(type)),
          is_const(is_const),
          initializer(std::move(initializer)) {}

    void info_dump(int level = 0) const override;

    [[nodiscard]] bool is_constant() const { return is_const; }
    [[nodiscard]] Expr* get_initializer() const { return initializer.get(); }
    [[nodiscard]] bool has_initializer() const { return initializer != nullptr; }

private:
    bool is_const;
    std::unique_ptr<Expr> initializer;
};

class ParamDecl : public Decl {
public:
    ParamDecl(SrcLocation location, std::string identifier, Type type)
        : Decl(std::move(location), std::move(identifier), std::move(type)) {}

    void info_dump(int level = 0) const override;

    [[nodiscard]] Type& get_type() { return type.value(); }

    void set_type(Type type) { this->type = std::move(type); }

private:
};

class FunDecl : public Decl {
public:
    FunDecl(SrcLocation location,
            std::string identifier,
            Type return_type,
            std::vector<std::unique_ptr<ParamDecl>> params,
            std::unique_ptr<Block> block_ptr)
        : Decl(std::move(location), std::move(identifier), std::move(return_type)),
          params(std::move(params)),
          block(std::move(block_ptr)) {}

    void info_dump(int level = 0) const override;

    [[nodiscard]] Type& get_return_type() { return type.value(); }
    [[nodiscard]] std::vector<std::unique_ptr<ParamDecl>>& get_params() { return params; }
    [[nodiscard]] Block* get_block() { return block.get(); }

    void set_return_type(Type type) { this->type = std::move(type); }
    void set_block(std::unique_ptr<Block> block_ptr) { block = std::move(block_ptr); }

private:
    std::vector<std::unique_ptr<ParamDecl>> params;
    std::unique_ptr<Block> block;
};
