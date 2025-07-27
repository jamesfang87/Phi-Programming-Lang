#pragma once

#include <memory>
#include <optional>
#include <string>
#include <vector>

#include "AST/Expr.hpp"
#include "AST/Stmt.hpp"
#include "AST/Type.hpp"
#include "SrcLocation.hpp"

class Decl {
public:
    Decl(SrcLocation location, std::string identifier, Type type)
        : location(std::move(location)),
          identifier(std::move(identifier)),
          type(std::move(type)) {}

    virtual ~Decl() = default;

    [[nodiscard]] std::string get_id() { return identifier; }
    [[nodiscard]] Type get_type() { return *type; }

    virtual void info_dump(int level) const = 0;

protected:
    SrcLocation location;
    std::string identifier;
    std::optional<Type> type;
};

class VarDecl final : public Decl {
public:
    VarDecl(SrcLocation location,
            std::string identifier,
            Type type,
            const bool is_const,
            std::unique_ptr<Expr> initializer);

    [[nodiscard]] bool is_constant() const { return is_const; }
    [[nodiscard]] Expr& get_initializer() const { return *initializer; }
    [[nodiscard]] bool has_initializer() const { return initializer != nullptr; }

    void info_dump(int level) const override;

private:
    const bool is_const;
    std::unique_ptr<Expr> initializer;
};

class ParamDecl final : public Decl {
public:
    ParamDecl(SrcLocation location, std::string identifier, Type type);

    void set_type(Type type) { this->type = std::move(type); }

    void info_dump(int level) const override;

private:
};

class FunDecl final : public Decl {
public:
    FunDecl(SrcLocation location,
            std::string identifier,
            Type return_type,
            std::vector<std::unique_ptr<ParamDecl>> params,
            std::unique_ptr<Block> block_ptr);

    [[nodiscard]] Type get_return_type() { return type.value(); }
    [[nodiscard]] std::vector<std::unique_ptr<ParamDecl>>& get_params() { return params; }
    [[nodiscard]] Block& get_block() const { return *block; }

    void set_return_type(Type type) { this->type = std::move(type); }
    void set_block(std::unique_ptr<Block> block_ptr) { block = std::move(block_ptr); }

    void info_dump(int level) const override;

private:
    std::vector<std::unique_ptr<ParamDecl>> params;
    std::unique_ptr<Block> block;
};
