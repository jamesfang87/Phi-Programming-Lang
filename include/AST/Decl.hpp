#include "AST/Stmt.hpp"
#include "SrcLocation.hpp"
#include "Type.hpp"
#include <memory>
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
    [[nodiscard]] Type& get_type() { return type; }

protected:
    SrcLocation location;
    std::string identifier;
    Type type;
};

class VarDecl : public Decl {
public:
    VarDecl(SrcLocation location, std::string identifier, Type type)
        : Decl(std::move(location), std::move(identifier), std::move(type)) {}

    void info_dump(int level = 0) const override;

private:
};

class ParamDecl : public Decl {
public:
    ParamDecl(SrcLocation location, std::string identifier, Type type)
        : Decl(std::move(location), std::move(identifier), std::move(type)) {}

    void info_dump(int level = 0) const override;

    [[nodiscard]] Type& get_type() { return type; }

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

    [[nodiscard]] Type& get_return_type() { return type; }
    [[nodiscard]] std::vector<std::unique_ptr<ParamDecl>>& get_params() { return params; }
    [[nodiscard]] Block* get_block() { return block.get(); }

    void set_return_type(Type type) { this->type = std::move(type); }
    void set_block(std::unique_ptr<Block> block_ptr) { block = std::move(block_ptr); }

private:
    std::vector<std::unique_ptr<ParamDecl>> params;
    std::unique_ptr<Block> block;
};
