#pragma once

#include <memory>
#include <vector>

#include "Sema/TypeInference/Substitution.hpp"
#include "SrcManager/SrcLocation.hpp"

namespace phi {

// Forward declarations - no includes needed
class NameResolver;
class TypeInferencer;
class CodeGen;

using InferRes = std::pair<Substitution, Monotype>;

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

  explicit Stmt(Kind K, SrcLocation Location)
      : StmtKind(K), Location(std::move(Location)) {}

  virtual ~Stmt() = default;

  [[nodiscard]] Kind getKind() const { return StmtKind; }
  [[nodiscard]] SrcLocation &getLocation() { return Location; }

  virtual void emit(int Level) const = 0;
  virtual bool accept(NameResolver &R) = 0;
  virtual InferRes accept(TypeInferencer &I) = 0;
  virtual void accept(CodeGen &G) = 0;

private:
  const Kind StmtKind;

protected:
  SrcLocation Location;
};

/**
 * @brief Block statement container
 */
class Block {
public:
  explicit Block(std::vector<std::unique_ptr<Stmt>> Stmts)
      : Stmts(std::move(Stmts)) {}

  [[nodiscard]] std::vector<std::unique_ptr<Stmt>> &getStmts() { return Stmts; }
  void emit(int Level) const;

private:
  std::vector<std::unique_ptr<Stmt>> Stmts;
};

// Statement class declarations
class ReturnStmt final : public Stmt {
public:
  ReturnStmt(SrcLocation Location, std::unique_ptr<Expr> expr);
  ~ReturnStmt() override;

  [[nodiscard]] bool hasExpr() const { return ReturnExpr != nullptr; }
  [[nodiscard]] Expr &getExpr() const { return *ReturnExpr; }

  void emit(int Level) const override;
  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  void accept(CodeGen &G) override;

  static bool classof(const Stmt *S) {
    return S->getKind() == Kind::ReturnStmtKind;
  }

private:
  std::unique_ptr<Expr> ReturnExpr;
};

class IfStmt final : public Stmt {
public:
  IfStmt(SrcLocation Location, std::unique_ptr<Expr> Cond,
         std::unique_ptr<Block> ThenBody, std::unique_ptr<Block> ElseBody);
  ~IfStmt() override;

  [[nodiscard]] Expr &getCond() const { return *Cond; }
  [[nodiscard]] Block &getThen() const { return *ThenBody; }
  [[nodiscard]] Block &getElse() const { return *ElseBody; }
  [[nodiscard]] bool hasElse() const { return ElseBody != nullptr; }

  void emit(int Level) const override;
  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  void accept(CodeGen &G) override;

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
  WhileStmt(SrcLocation Location, std::unique_ptr<Expr> Cond,
            std::unique_ptr<Block> Body);
  ~WhileStmt() override;

  [[nodiscard]] Expr &getCond() const { return *Cond; }
  [[nodiscard]] Block &getBody() const { return *Body; }

  void emit(int Level) const override;
  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  void accept(CodeGen &G) override;

  static bool classof(const Stmt *S) {
    return S->getKind() == Kind::WhileStmtKind;
  }

private:
  std::unique_ptr<Expr> Cond;
  std::unique_ptr<Block> Body;
};

class ForStmt final : public Stmt {
public:
  ForStmt(SrcLocation Location, std::unique_ptr<VarDecl> LoopVar,
          std::unique_ptr<Expr> Range, std::unique_ptr<Block> Body);
  ~ForStmt() override;

  [[nodiscard]] VarDecl &getLoopVar() { return *LoopVar; }
  [[nodiscard]] Expr &getRange() const { return *Range; }
  [[nodiscard]] Block &getBody() const { return *Body; }

  void emit(int Level) const override;
  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  void accept(CodeGen &G) override;

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
  DeclStmt(SrcLocation Location, std::unique_ptr<VarDecl> Var);
  ~DeclStmt() override;

  [[nodiscard]] VarDecl &getDecl() const { return *Var; }

  void emit(int Level) const override;
  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  void accept(CodeGen &G) override;

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

  void emit(int Level) const override;
  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  void accept(CodeGen &G) override;

  static bool classof(const Stmt *S) {
    return S->getKind() == Kind::BreakStmtKind;
  }
};

class ContinueStmt final : public Stmt {
public:
  ContinueStmt(SrcLocation Location);
  ~ContinueStmt() override;

  void emit(int Level) const override;
  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  void accept(CodeGen &G) override;

  static bool classof(const Stmt *S) {
    return S->getKind() == Kind::ContinueStmtKind;
  }
};

} // namespace phi
