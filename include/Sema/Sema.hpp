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

namespace phi {

/**
 * @brief Semantic analyzer for the Phi programming language
 *
 * Performs semantic analysis on the AST including:
 * 1. Symbol resolution (binding identifiers to declarations)
 * 2. Type checking and validation
 * 3. Type inference for expressions
 * 4. Control flow validation
 * 5. Error reporting for semantic violations
 *
 * Implements the visitor pattern to traverse and annotate the AST.
 */
class Sema final : public ASTVisitor<bool> {
public:
    /**
     * @brief Constructs a semantic analyzer
     *
     * @param ast The abstract syntax tree to analyze
     */
    explicit Sema(std::vector<std::unique_ptr<FunDecl>> ast)
        : ast(std::move(ast)) {}

    /**
     * @brief Main entry point for semantic analysis
     *
     * @return std::pair<bool, std::vector<std::unique_ptr<FunDecl>>>
     *         First element: true if analysis succeeded without errors,
     *                      false if semantic errors were found
     *         Second element: The annotated AST (modified in-place)
     */
    std::pair<bool, std::vector<std::unique_ptr<FunDecl>>> resolve_ast();

    // EXPRESSION VISITORS
    /// Resolves integer literal expressions (trivial, sets type to appropriate integer type)
    bool visit(IntLiteral& expr) override;

    /// Resolves floating-point literal expressions (sets type to float type)
    bool visit(FloatLiteral& expr) override;

    /// Resolves string literal expressions (sets type to string type)
    bool visit(StrLiteral& expr) override;

    /// Resolves character literal expressions (sets type to char type)
    bool visit(CharLiteral& expr) override;

    /// Resolves boolean literal expressions (sets type to bool type)
    bool visit(BoolLiteral& expr) override;

    /// Resolves range literals (validates operand types and sets range type)
    bool visit(RangeLiteral& expr) override;

    /**
     * @brief Resolves declaration references (variables, functions)
     *
     * @param expr The reference expression
     * @return true if resolution succeeded, false if identifier not found
     */
    bool visit(DeclRefExpr& expr) override;

    /**
     * @brief Resolves function calls (validates callee and arguments)
     *
     * Performs:
     * - Callee expression resolution
     * - Argument count validation
     * - Argument type checking
     * - Return type propagation
     *
     * @param expr The function call expression
     * @return true if resolution succeeded, false otherwise
     */
    bool visit(FunCallExpr& expr) override;

    /**
     * @brief Resolves binary operations
     *
     * Performs:
     * - Operand type resolution
     * - Operator validity checking for operand types
     * - Type promotion for numeric operations
     * - Result type determination
     *
     * @param expr The binary operation expression
     * @return true if resolution succeeded, false otherwise
     */
    bool visit(BinaryOp& expr) override;

    /**
     * @brief Resolves unary operations
     *
     * Performs:
     * - Operand type resolution
     * - Operator validity checking
     * - Result type determination
     *
     * @param expr The unary operation expression
     * @return true if resolution succeeded, false otherwise
     */
    bool visit(UnaryOp& expr) override;

    // STATEMENT VISITORS
    /**
     * @brief Validates return statements
     *
     * Checks:
     * - Presence of return value matches function return type
     * - Type compatibility of return expression with function return type
     * - Return statement placement (not allowed outside functions)
     *
     * @param stmt The return statement
     * @return true if validation succeeded, false otherwise
     */
    bool visit(ReturnStmt& stmt) override;

    /**
     * @brief Validates if statements
     *
     * Checks:
     * - Condition expression is boolean
     * - Resolves then and else blocks
     *
     * @param stmt The if statement
     * @return true if validation succeeded, false otherwise
     */
    bool visit(IfStmt& stmt) override;

    /**
     * @brief Validates while loops
     *
     * Checks:
     * - Condition expression is boolean
     * - Resolves loop body
     *
     * @param stmt The while statement
     * @return true if validation succeeded, false otherwise
     */
    bool visit(WhileStmt& stmt) override;

    /**
     * @brief Validates for loops
     *
     * Checks:
     * - Loop variable declaration
     * - Range expression is valid range type
     * - Resolves loop body
     *
     * @param stmt The for statement
     * @return true if validation succeeded, false otherwise
     */
    bool visit(ForStmt& stmt) override;

    /**
     * @brief Validates variable declarations
     *
     * Performs:
     * - Type resolution for variable
     * - Type checking for initializer expression
     * - Insertion into symbol table
     *
     * @param stmt The let statement
     * @return true if validation succeeded, false otherwise
     */
    bool visit(LetStmt& stmt) override;

    /**
     * @brief Validates expression statements
     *
     * Primarily ensures expression is valid and has side effects
     *
     * @param stmt The expression statement
     * @return true if validation succeeded, false otherwise
     */
    bool visit(Expr& stmt) override;

private:
    /// The AST being analyzed (function declarations)
    std::vector<std::unique_ptr<FunDecl>> ast;

    /// Symbol table for identifier resolution
    SymbolTable symbol_table;

    /**
     * @brief Stack of active scopes for diagnostics
     *
     * Maintains parallel information to symbol table for richer
     * error reporting and scope-aware diagnostics.
     */
    std::vector<std::unordered_map<std::string, Decl*>> active_scopes;

    /// Pointer to the function currently being analyzed
    FunDecl* cur_fun = nullptr;

    // RESOLUTION HELPERS
    /**
     * @brief Resolves a declaration reference
     *
     * @param declref The declaration reference expression
     * @param function_call True if resolving for function call context
     * @return true if resolution succeeded, false otherwise
     */
    bool resolve_decl_ref(DeclRefExpr* declref, bool function_call);

    /**
     * @brief Resolves a function call expression
     *
     * @param call The function call expression
     * @return true if resolution succeeded, false otherwise
     */
    bool resolve_function_call(FunCallExpr* call);

    /**
     * @brief Resolves a block statement
     *
     * @param block The block to resolve
     * @param scope_created True if new scope was already created
     * @return true if resolution succeeded, false otherwise
     */
    bool resolve_block(Block& block, bool scope_created);

    // DECLARATION RESOLUTION
    /**
     * @brief Resolves a function declaration
     *
     * Processes parameters, return type, and body
     *
     * @param fun The function declaration
     * @return true if resolution succeeded, false otherwise
     */
    static bool resolve_fun_decl(FunDecl* fun);

    /**
     * @brief Validates a type
     *
     * @param type The type to validate
     * @return true if type is valid, false otherwise
     */
    static bool resolve_type(std::optional<Type> type);

    /**
     * @brief Resolves a parameter declaration
     *
     * @param param The parameter declaration
     * @return true if resolution succeeded, false otherwise
     */
    static bool resolve_param_decl(ParamDecl* param);

    // TYPE SYSTEM UTILITIES
    /// Checks if type is any integer type (signed or unsigned)
    /**
     * @brief Determines result type for binary operations on numeric types
     *
     * Implements type promotion rules based on promotion rank:
     * - If either operand is float, result is the float type (f64 > f32)
     * - For integers, promotes to type with higher rank:
     *   i8(1) < u8(2) < i16(3) < u16(4) < i32(5) < u32(6) < i64(7) < u64(8)
     * - Mixed signed/unsigned of same size promotes to unsigned
     * - Floats have highest precedence: f32(10) < f64(11)
     *
     * @param lhs Type of left operand
     * @param rhs Type of right operand
     * @return Type Resulting promoted type with highest promotion rank
     */
    static Type promote_numeric_types(const Type& lhs, const Type& rhs);
};

} // namespace phi
