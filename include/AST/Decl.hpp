#include "AST/Stmt.hpp"
#include "SrcLocation.hpp"
#include "Type.hpp"
#include <memory>
#include <string>
#include <vector>

#pragma once

class Decl {
public:
    Decl(SrcLocation location, std::string identifier)
        : location(std::move(location)),
          identifier(std::move(identifier)) {}

    virtual ~Decl() = default;

    virtual void info_dump(int level = 0) const = 0;

    [[nodiscard]] std::string get_id() const { return identifier; }
    [[nodiscard]] SrcLocation get_location() const { return location; }

protected:
    SrcLocation location;
    std::string identifier;
};

class ParamDecl : public Decl {
public:
    ParamDecl(SrcLocation location, std::string identifier, Type type)
        : Decl(std::move(location), std::move(identifier)),
          type(std::move(type)) {}

    void info_dump(int level = 0) const override;

    [[nodiscard]] const Type& get_type() const { return type; }

private:
    Type type;
};

class Block {
public:
    Block(std::vector<std::unique_ptr<Stmt>> stmts)
        : stmts(std::move(stmts)) {}

    void info_dump(int level = 0) const;

    [[nodiscard]] const std::vector<std::unique_ptr<Stmt>>& get_stmts() const { return stmts; }

private:
    std::vector<std::unique_ptr<Stmt>> stmts;
};

class FunctionDecl : public Decl {
public:
    FunctionDecl(SrcLocation location,
                 std::string identifier,
                 Type return_type,
                 std::unique_ptr<std::vector<std::unique_ptr<ParamDecl>>> params,
                 std::unique_ptr<Block> block_ptr)
        : Decl(std::move(location), std::move(identifier)),
          return_type(std::move(return_type)),
          params(std::move(params)),
          block(std::move(block_ptr)) {}

    void info_dump(int level = 0) const override;

    [[nodiscard]] const Type& get_return_type() const { return return_type; }
    [[nodiscard]] const std::vector<std::unique_ptr<ParamDecl>>* get_params() const {
        return params.get();
    }
    [[nodiscard]] const Block* get_block() const { return block.get(); }

private:
    Type return_type;
    std::unique_ptr<std::vector<std::unique_ptr<ParamDecl>>> params;
    std::unique_ptr<Block> block;
};
