#pragma once

#include "SrcManager/SrcLocation.hpp"
#include <memory>
#include <vector>

namespace phi {

// Forward declarations - no includes needed
template <typename T> class ASTVisitor;
class Expr;
class VarDecl;
class Block;

/**
 * @brief Base class for all statement nodes
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
    DeclStmtKind,
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
    StructInitKind,
    FieldInitKind,
    MemberAccessKind,
    MemberFunAccessKind,
    ExprLast = MemberFunAccessKind
  };

  explicit Stmt(Kind K, SrcLocation location)
      : StmtKind(K), location(std::move(location)) {}

  virtual ~Stmt() = default;

  [[nodiscard]] Kind getKind() const { return StmtKind; }
  [[nodiscard]] SrcLocation &getLocation() { return location; }

  virtual void emit(int level) const = 0;
  virtual bool accept(ASTVisitor<bool> &visitor) = 0;
  virtual void accept(ASTVisitor<void> &visitor) = 0;

private:
  const Kind StmtKind;

protected:
  SrcLocation location;
};

/**
 * @brief Block statement container
 */
class Block {
public:
  explicit Block(std::vector<std::unique_ptr<Stmt>> stmts)
      : stmts(std::move(stmts)) {}

  [[nodiscard]] std::vector<std::unique_ptr<Stmt>> &getStmts() { return stmts; }
  void emit(int level) const;

private:
  std::vector<std::unique_ptr<Stmt>> stmts;
};

// Statement class declarations
class ReturnStmt final : public Stmt {
public:
  ReturnStmt(SrcLocation location, std::unique_ptr<Expr> expr);
  ~ReturnStmt() override;

  [[nodiscard]] bool hasExpr() const { return ReturnExpr != nullptr; }
  [[nodiscard]] Expr &getExpr() const { return *ReturnExpr; }

  void emit(int level) const override;
  bool accept(ASTVisitor<bool> &visitor) override;
  void accept(ASTVisitor<void> &visitor) override;

  static bool classof(const Stmt *S) {
    return S->getKind() == Kind::ReturnStmtKind;
  }

private:
  std::unique_ptr<Expr> ReturnExpr;
};

class IfStmt final : public Stmt {
public:
  IfStmt(SrcLocation location, std::unique_ptr<Expr> cond,
         std::unique_ptr<Block> thenBlock, std::unique_ptr<Block> elseBlock);
  ~IfStmt() override;

  [[nodiscard]] Expr &getCond() const { return *Cond; }
  [[nodiscard]] Block &getThen() const { return *ThenBody; }
  [[nodiscard]] Block &getElse() const { return *ElseBody; }
  [[nodiscard]] bool hasElse() const { return ElseBody != nullptr; }

  void emit(int level) const override;
  bool accept(ASTVisitor<bool> &visitor) override;
  void accept(ASTVisitor<void> &visitor) override;

  static bool classof(const Stmt *S) {
    return S->getKind() == Kind::IfStmtKind;
  }

private:
  std::unique_ptr<Expr> Cond;
  std::unique_ptr<Block> ThenBody;
  std::unique_ptr<Block> ElseBody;
};

class WhileStmt final : public Stmt {
public:
  WhileStmt(SrcLocation location, std::unique_ptr<Expr> cond,
            std::unique_ptr<Block> body);
  ~WhileStmt() override;

  [[nodiscard]] Expr &getCond() const { return *Cond; }
  [[nodiscard]] Block &getBody() const { return *Body; }

  void emit(int level) const override;
  bool accept(ASTVisitor<bool> &visitor) override;
  void accept(ASTVisitor<void> &visitor) override;

  static bool classof(const Stmt *S) {
    return S->getKind() == Kind::WhileStmtKind;
  }

private:
  std::unique_ptr<Expr> Cond;
  std::unique_ptr<Block> Body;
};

class ForStmt final : public Stmt {
public:
  ForStmt(SrcLocation location, std::unique_ptr<VarDecl> loopVar,
          std::unique_ptr<Expr> range, std::unique_ptr<Block> body);
  ~ForStmt() override;

  [[nodiscard]] VarDecl &getLoopVar() { return *LoopVar; }
  [[nodiscard]] Expr &getRange() const { return *Range; }
  [[nodiscard]] Block &getBody() const { return *Body; }

  void emit(int level) const override;
  bool accept(ASTVisitor<bool> &visitor) override;
  void accept(ASTVisitor<void> &visitor) override;

  static bool classof(const Stmt *S) {
    return S->getKind() == Kind::ForStmtKind;
  }

private:
  std::unique_ptr<VarDecl> LoopVar;
  std::unique_ptr<Expr> Range;
  std::unique_ptr<Block> Body;
};

class DeclStmt final : public Stmt {
public:
  DeclStmt(SrcLocation location, std::unique_ptr<VarDecl> decl);
  ~DeclStmt() override;

  [[nodiscard]] VarDecl &getDecl() const { return *Var; }

  void emit(int level) const override;
  bool accept(ASTVisitor<bool> &visitor) override;
  void accept(ASTVisitor<void> &visitor) override;

  static bool classof(const Stmt *S) {
    return S->getKind() == Kind::DeclStmtKind;
  }

private:
  std::unique_ptr<VarDecl> Var;
};

class BreakStmt final : public Stmt {
public:
  BreakStmt(SrcLocation Location);
  ~BreakStmt() override;

  void emit(int level) const override;
  bool accept(ASTVisitor<bool> &Visitor) override;
  void accept(ASTVisitor<void> &Visitor) override;

  static bool classof(const Stmt *S) {
    return S->getKind() == Kind::BreakStmtKind;
  }
};

class ContinueStmt final : public Stmt {
public:
  ContinueStmt(SrcLocation Location);
  ~ContinueStmt() override;

  void emit(int level) const override;
  bool accept(ASTVisitor<bool> &visitor) override;
  void accept(ASTVisitor<void> &visitor) override;

  static bool classof(const Stmt *S) {
    return S->getKind() == Kind::ContinueStmtKind;
  }
};

} // namespace phi
