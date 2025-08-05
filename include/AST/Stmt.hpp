#pragma once

#include <memory>
#include <vector>

#include "AST/ASTVisitor.hpp"
#include "SrcManager/SrcLocation.hpp"

namespace phi {

// Forward declarations
class Decl;
class FunDecl;
class VarDecl;

/**
 * @brief Base class for all statement nodes
 *
 * Represents executable constructs in the Phi language. Contains source
 * location information and defines the visitor acceptance interface.
 */
class Stmt {
public:
  /// @brief Kind enumeration for LLVM RTTI
  enum class Kind : uint8_t {
    // Statements
    ReturnStmtKind,
    IfStmtKind,
    WhileStmtKind,
    ForStmtKind,
    LetStmtKind,
    ContinueStmtKind,
    BreakStmtKind,

    // Expressions (ranges for easier checking)
    ExprFirst,
    IntLiteralKind = ExprFirst,
    FloatLiteralKind,
    StrLiteralKind,
    CharLiteralKind,
    BoolLiteralKind,
    RangeLiteralKind,
    DeclRefExprKind,
    FunCallExprKind,
    BinaryOpKind,
    UnaryOpKind,
    ExprLast = UnaryOpKind
  };

  explicit Stmt(Kind K, SrcLocation location)
      : StmtKind(K), location(std::move(location)) {}

  virtual ~Stmt() = default;

  /// @brief Get the kind of this statement
  [[nodiscard]] Kind getKind() const { return StmtKind; }

  /**
   * @brief Retrieves source location
   * @return Reference to source location
   */
  [[nodiscard]] SrcLocation &getLocation() { return location; }

  /**
   * @brief Debug output method
   * @param level Indentation level
   */
  virtual void emit(int level) const = 0;

  /**
   * @brief Accepts AST visitor
   * @param visitor Visitor implementation
   * @return Visitor control result
   */
  virtual bool accept(ASTVisitor<bool> &visitor) = 0;

  /**
   * @brief Accepts AST visitor for code generation
   * @param visitor Visitor implementation returning void
   */
  virtual void accept(ASTVisitor<void> &visitor) = 0;

private:
  const Kind StmtKind; ///< Kind of this statement for RTTI

protected:
  SrcLocation location; ///< Source location of statement
};

/**
 * @brief Block statement container
 *
 * Represents a sequence of statements enclosed in braces. Used in control flow
 * structures and function bodies.
 */
class Block {
public:
  /**
   * @brief Constructs block
   * @param stmts Contained statements
   */
  explicit Block(std::vector<std::unique_ptr<Stmt>> stmts)
      : stmts(std::move(stmts)) {}

  /**
   * @brief Retrieves contained statements
   * @return Reference to statement list
   */
  [[nodiscard]] std::vector<std::unique_ptr<Stmt>> &getStmts() { return stmts; }

  /**
   * @brief Debug output for block
   * @param level Indentation level
   */
  void emit(int level) const;

private:
  std::vector<std::unique_ptr<Stmt>> stmts; ///< Contained statements
};

/**
 * @brief Return statement
 */
class ReturnStmt final : public Stmt {
public:
  /**
   * @brief Constructs return statement
   * @param location Source location
   * @param expr Return value expression
   */
  ReturnStmt(SrcLocation location, std::unique_ptr<Expr> expr);
  ~ReturnStmt() override;

  /**
   * @brief Checks if has return value
   * @return true if has expression, false otherwise
   */
  [[nodiscard]] bool hasExpr() const { return expr != nullptr; }

  /**
   * @brief Retrieves return expression
   * @return Reference to return expression
   */
  [[nodiscard]] Expr &getExpr() const { return *expr; }

  void emit(int level) const override;
  bool accept(ASTVisitor<bool> &visitor) override {
    return visitor.visit(*this);
  }
  void accept(ASTVisitor<void> &visitor) override { visitor.visit(*this); }

  /// @brief LLVM RTTI support
  static bool classof(const Stmt *S) {
    return S->getKind() == Kind::ReturnStmtKind;
  }

private:
  std::unique_ptr<Expr> expr; ///< Return value expression
};

/**
 * @brief If condal statement
 */
class IfStmt final : public Stmt {
public:
  /**
   * @brief Constructs if statement
   * @param location Source location
   * @param cond condal expression
   * @param then_block Then block
   * @param else_block Else block (optional)
   */
  IfStmt(SrcLocation location, std::unique_ptr<Expr> cond,
         std::unique_ptr<Block> thenBlock, std::unique_ptr<Block> elseBlock);
  ~IfStmt() override;

  /**
   * @brief Retrieves cond expression
   * @return Reference to cond
   */
  [[nodiscard]] Expr &getCond() const { return *cond; }

  /**
   * @brief Retrieves then block
   * @return Reference to then block
   */
  [[nodiscard]] Block &getThen() const { return *thenBlock; }

  /**
   * @brief Retrieves else block
   * @return Reference to else block
   */
  [[nodiscard]] Block &getElse() const { return *elseBlock; }

  /**
   * @brief Checks if has else block
   * @return true if has else block, false otherwise
   */
  [[nodiscard]] bool hasElse() const { return elseBlock != nullptr; }

  void emit(int level) const override;
  bool accept(ASTVisitor<bool> &visitor) override {
    return visitor.visit(*this);
  }
  void accept(ASTVisitor<void> &visitor) override { visitor.visit(*this); }

  /// @brief LLVM RTTI support
  static bool classof(const Stmt *S) {
    return S->getKind() == Kind::IfStmtKind;
  }

private:
  std::unique_ptr<Expr> cond;       ///< cond expression
  std::unique_ptr<Block> thenBlock; ///< Then clause block
  std::unique_ptr<Block> elseBlock; ///< Else clause block (optional)
};

/**
 * @brief While loop statement
 */
class WhileStmt final : public Stmt {
public:
  /**
   * @brief Constructs while loop
   * @param location Source location
   * @param cond Loop cond
   * @param body Loop body block
   */
  WhileStmt(SrcLocation location, std::unique_ptr<Expr> cond,
            std::unique_ptr<Block> body);
  ~WhileStmt() override;

  /**
   * @brief Retrieves cond expression
   * @return Reference to cond
   */
  [[nodiscard]] Expr &getCond() const { return *cond; }

  /**
   * @brief Retrieves loop body
   * @return Reference to body block
   */
  [[nodiscard]] Block &getBody() const { return *body; };

  void emit(int level) const override;
  bool accept(ASTVisitor<bool> &visitor) override {
    return visitor.visit(*this);
  }
  void accept(ASTVisitor<void> &visitor) override { visitor.visit(*this); }

  /// @brief LLVM RTTI support
  static bool classof(const Stmt *S) {
    return S->getKind() == Kind::WhileStmtKind;
  }

private:
  std::unique_ptr<Expr> cond;  ///< Loop cond
  std::unique_ptr<Block> body; ///< Loop body block
};

/**
 * @brief For loop statement
 */
class ForStmt final : public Stmt {
public:
  /**
   * @brief Constructs for loop
   * @param location Source location
   * @param loop_var Loop variable declaration
   * @param range Range expression
   * @param body Loop body block
   */
  ForStmt(SrcLocation location, std::unique_ptr<VarDecl> loopVar,
          std::unique_ptr<Expr> range, std::unique_ptr<Block> body);
  ~ForStmt() override;

  /**
   * @brief Retrieves loop variable
   * @return Reference to loop variable declaration
   */
  [[nodiscard]] VarDecl &getLoopVar() { return *loopVar; }

  /**
   * @brief Retrieves range expression
   * @return Reference to range expression
   */
  [[nodiscard]] Expr &getRange() const { return *range; }

  /**
   * @brief Retrieves loop body
   * @return Reference to body block
   */
  [[nodiscard]] Block &getBody() const { return *body; }

  void emit(int level) const override;
  bool accept(ASTVisitor<bool> &visitor) override {
    return visitor.visit(*this);
  }
  void accept(ASTVisitor<void> &visitor) override { visitor.visit(*this); }

  /// @brief LLVM RTTI support
  static bool classof(const Stmt *S) {
    return S->getKind() == Kind::ForStmtKind;
  }

private:
  std::unique_ptr<VarDecl> loopVar; ///< Loop variable declaration
  std::unique_ptr<Expr> range;      ///< Range expression
  std::unique_ptr<Block> body;      ///< Loop body block
};

/**
 * @brief Variable declaration statement
 */
class LetStmt final : public Stmt {
public:
  /**
   * @brief Constructs let statement
   * @param location Source location
   * @param decl Variable declaration
   */
  LetStmt(SrcLocation location, std::unique_ptr<VarDecl> decl);
  ~LetStmt() override;

  /**
   * @brief Retrieves variable declaration
   * @return Reference to variable declaration
   */
  [[nodiscard]] VarDecl &getDecl() const { return *decl; }

  void emit(int level) const override;
  bool accept(ASTVisitor<bool> &visitor) override {
    return visitor.visit(*this);
  }
  void accept(ASTVisitor<void> &visitor) override { visitor.visit(*this); }

  /// @brief LLVM RTTI support
  static bool classof(const Stmt *S) {
    return S->getKind() == Kind::LetStmtKind;
  }

private:
  std::unique_ptr<VarDecl> decl; ///< Variable declaration
};

class BreakStmt final : public Stmt {
public:
  /**
   * @brief Constructs break statement
   * @param location Source location
   */
  BreakStmt(SrcLocation Location);
  ~BreakStmt() override;

  void emit(int level) const override;
  bool accept(ASTVisitor<bool> &Visitor) override {
    return Visitor.visit(*this);
  }
  void accept(ASTVisitor<void> &Visitor) override { Visitor.visit(*this); }

  /// @brief LLVM RTTI support
  static bool classof(const Stmt *S) {
    return S->getKind() == Kind::BreakStmtKind;
  }
};

class ContinueStmt final : public Stmt {
public:
  /**
   * @brief Constructs continue statement
   * @param location Source location
   */
  ContinueStmt(SrcLocation Location);
  ~ContinueStmt() override;

  void emit(int level) const override;
  bool accept(ASTVisitor<bool> &visitor) override {
    return visitor.visit(*this);
  }
  void accept(ASTVisitor<void> &visitor) override { visitor.visit(*this); }

  /// @brief LLVM RTTI support
  static bool classof(const Stmt *S) {
    return S->getKind() == Kind::ContinueStmtKind;
  }
};

} // namespace phi
