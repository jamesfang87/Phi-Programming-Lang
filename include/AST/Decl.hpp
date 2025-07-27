#pragma once

#include <memory>
#include <optional>
#include <string>
#include <vector>

#include "AST/Expr.hpp"
#include "AST/Stmt.hpp"
#include "AST/Type.hpp"
#include "SrcManager/SrcLocation.hpp"

/**
 * @brief Base class for all declaration nodes in the AST
 *
 * Represents named entities in the source code like variables, parameters, and
 * functions. Contains common properties including source location, identifier,
 * and type information.
 */
class Decl {
public:
    /**
     * @brief Constructs a Decl node
     * @param location Source location of declaration
     * @param identifier Name of declared entity
     * @param type Type of declared entity
     */
    Decl(SrcLocation location, std::string identifier, Type type)
        : location(std::move(location)),
          identifier(std::move(identifier)),
          type(std::move(type)) {}

    virtual ~Decl() = default;

    /**
     * @brief Retrieves declared identifier
     * @return Identifier string
     */
    [[nodiscard]] std::string get_id() { return identifier; }

    /**
     * @brief Retrieves declared type
     * @return Type object
     */
    [[nodiscard]] Type get_type() { return *type; }

    /**
     * @brief Debug output for AST structure
     * @param level Current indentation level for formatting
     */
    virtual void info_dump(int level) const = 0;

protected:
    SrcLocation location;     ///< Source location of declaration
    std::string identifier;   ///< Name of declared entity
    std::optional<Type> type; ///< Type information (std::nullopt for
                              ///< unresolved declarations)
};

/**
 * @brief Variable declaration node
 *
 * Represents variable declarations including:
 * - Constant vs mutable status
 * - Optional initializer expression
 * - Type annotation
 */
class VarDecl final : public Decl {
public:
    /**
     * @brief Constructs a VarDecl node
     * @param location Source location of declaration
     * @param identifier Variable name
     * @param type Variable type
     * @param is_const Constant flag (true for const)
     * @param initializer Initial value expression
     */
    VarDecl(SrcLocation location,
            std::string identifier,
            Type type,
            const bool is_const,
            std::unique_ptr<Expr> initializer);

    /**
     * @brief Checks constant status
     * @return true if constant, false if mutable
     */
    [[nodiscard]] bool is_constant() const { return is_const; }

    /**
     * @brief Retrieves initializer expression
     * @return Reference to initializer expression
     */
    [[nodiscard]] Expr& get_initializer() const { return *initializer; }

    /**
     * @brief Checks for initializer presence
     * @return true if has initializer, false otherwise
     */
    [[nodiscard]] bool has_initializer() const { return initializer != nullptr; }

    /**
     * @brief Debug output for variable declaration
     * @param level Current indentation level
     */
    void info_dump(int level) const override;

private:
    const bool is_const;               ///< Constant declaration flag
    std::unique_ptr<Expr> initializer; ///< Initial value expression
};

/**
 * @brief Function parameter declaration node
 *
 * Represents formal parameters in function declarations. Contains parameter
 * name and type information.
 */
class ParamDecl final : public Decl {
public:
    /**
     * @brief Constructs a ParamDecl node
     * @param location Source location of parameter
     * @param identifier Parameter name
     * @param type Parameter type
     */
    ParamDecl(SrcLocation location, std::string identifier, Type type);

    /**
     * @brief Sets parameter type
     * @param type New type for parameter
     */
    void set_type(Type type) { this->type = std::move(type); }

    /**
     * @brief Debug output for parameter
     * @param level Current indentation level
     */
    void info_dump(int level) const override;
};

/**
 * @brief Function declaration node
 *
 * Represents function definitions containing:
 * - Return type
 * - Parameter list
 * - Function body block
 */
class FunDecl final : public Decl {
public:
    /**
     * @brief Constructs a FunDecl node
     * @param location Source location of function
     * @param identifier Function name
     * @param return_type Function return type
     * @param params List of function parameters
     * @param block_ptr Function body block
     */
    FunDecl(SrcLocation location,
            std::string identifier,
            Type return_type,
            std::vector<std::unique_ptr<ParamDecl>> params,
            std::unique_ptr<Block> block_ptr);

    /**
     * @brief Retrieves return type
     * @return Function return type
     */
    [[nodiscard]] Type get_return_type() { return type.value(); }

    /**
     * @brief Retrieves parameter declarations
     * @return Reference to parameter list
     */
    [[nodiscard]] std::vector<std::unique_ptr<ParamDecl>>& get_params() { return params; }

    /**
     * @brief Retrieves function body
     * @return Reference to body block
     */
    [[nodiscard]] Block& get_block() const { return *block; }

    /**
     * @brief Sets return type
     * @param type New return type
     */
    void set_return_type(Type type) { this->type = std::move(type); }

    /**
     * @brief Sets function body
     * @param block_ptr New body block
     */
    void set_block(std::unique_ptr<Block> block_ptr) { block = std::move(block_ptr); }

    /**
     * @brief Debug output for function
     * @param level Current indentation level
     */
    void info_dump(int level) const override;

private:
    std::vector<std::unique_ptr<ParamDecl>> params; ///< Function parameters
    std::unique_ptr<Block> block;                   ///< Function body statements
};
