#pragma once

#include <memory>
#include <vector>

#include "Sema/TypeInference/Substitution.hpp"
#include "SrcManager/SrcLocation.hpp"

namespace phi {

// Forward declarations - no includes needed
class NameResolver;
class TypeInferencer;
class TypeChecker;
class CodeGen;
class Expr;
class VarDecl;

using InferRes = std::pair<Substitution, Monotype>;

//===----------------------------------------------------------------------===//
// Stmt - Base class for all statement nodes
//===----------------------------------------------------------------------===//

class Stmt {
public:
  /// @brief Kind enumeration for LLVM RTTI
  enum class Kind : uint8_t {
    // Statements
    ReturnStmtKind,
    DeferStmtKind,
    IfStmtKind,
    WhileStmtKind,
    ForStmtKind,
    DeclStmtKind,
    ContinueStmtKind,
    BreakStmtKind,
    ExprStmtKind
  };

  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  explicit Stmt(Kind K, SrcLocation Location)
      : StmtKind(K), Location(std::move(Location)) {}

  virtual ~Stmt() = default;

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] Kind getKind() const { return StmtKind; }
  [[nodiscard]] SrcLocation &getLocation() { return Location; }

  //===--------------------------------------------------------------------===//
  // Visitor Methods
  //===--------------------------------------------------------------------===//

  virtual bool accept(NameResolver &R) = 0;
  virtual InferRes accept(TypeInferencer &I) = 0;
  virtual bool accept(TypeChecker &C) = 0;
  virtual void accept(CodeGen &G) = 0;

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//

  virtual void emit(int Level) const = 0;

private:
  const Kind StmtKind;

protected:
  SrcLocation Location;
};

//===----------------------------------------------------------------------===//
// Block - Statement container
//===----------------------------------------------------------------------===//

class Block {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  explicit Block(std::vector<std::unique_ptr<Stmt>> Stmts)
      : Stmts(std::move(Stmts)) {}

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] std::vector<std::unique_ptr<Stmt>> &getStmts() { return Stmts; }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//

  void emit(int Level) const;

private:
  std::vector<std::unique_ptr<Stmt>> Stmts;
};

//===----------------------------------------------------------------------===//
// Control Flow Statements
//===----------------------------------------------------------------------===//

class ReturnStmt final : public Stmt {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  ReturnStmt(SrcLocation Location, std::unique_ptr<Expr> Expr);
  ~ReturnStmt() override;

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] Expr &getExpr() const { return *ReturnExpr; }

  //===--------------------------------------------------------------------===//
  // Type Queries
  //===--------------------------------------------------------------------===//

  [[nodiscard]] bool hasExpr() const { return ReturnExpr != nullptr; }

  //===--------------------------------------------------------------------===//
  // Visitor Methods
  //===--------------------------------------------------------------------===//

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  bool accept(TypeChecker &C) override;
  void accept(CodeGen &G) override;

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===--------------------------------------------------------------------===//

  static bool classof(const Stmt *S) {
    return S->getKind() == Kind::ReturnStmtKind;
  }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//

  void emit(int Level) const override;

private:
  std::unique_ptr<Expr> ReturnExpr;
};

class DeferStmt final : public Stmt {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  DeferStmt(SrcLocation Location, std::unique_ptr<Expr> Expr);
  ~DeferStmt() override;

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] Expr &getDeferred() const { return *DeferredExpr; }

  //===--------------------------------------------------------------------===//
  // Visitor Methods
  //===--------------------------------------------------------------------===//

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  bool accept(TypeChecker &C) override;
  void accept(CodeGen &G) override;

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===--------------------------------------------------------------------===//

  static bool classof(const Stmt *S) {
    return S->getKind() == Kind::DeferStmtKind;
  }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//

  void emit(int Level) const override;

private:
  std::unique_ptr<Expr> DeferredExpr;
};

class BreakStmt final : public Stmt {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  BreakStmt(SrcLocation Location);
  ~BreakStmt() override;

  //===--------------------------------------------------------------------===//
  // Visitor Methods
  //===--------------------------------------------------------------------===//

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  bool accept(TypeChecker &C) override;
  void accept(CodeGen &G) override;

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===--------------------------------------------------------------------===//

  static bool classof(const Stmt *S) {
    return S->getKind() == Kind::BreakStmtKind;
  }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//

  void emit(int Level) const override;
};

class ContinueStmt final : public Stmt {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  ContinueStmt(SrcLocation Location);
  ~ContinueStmt() override;

  //===--------------------------------------------------------------------===//
  // Visitor Methods
  //===--------------------------------------------------------------------===//

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  bool accept(TypeChecker &C) override;
  void accept(CodeGen &G) override;

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===--------------------------------------------------------------------===//

  static bool classof(const Stmt *S) {
    return S->getKind() == Kind::ContinueStmtKind;
  }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//

  void emit(int Level) const override;
};

//===----------------------------------------------------------------------===//
// Conditional and Loop Statements
//===----------------------------------------------------------------------===//

class IfStmt final : public Stmt {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  IfStmt(SrcLocation Location, std::unique_ptr<Expr> Cond,
         std::unique_ptr<Block> ThenBody, std::unique_ptr<Block> ElseBody);
  ~IfStmt() override;

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] Expr &getCond() const { return *Cond; }
  [[nodiscard]] Block &getThen() const { return *ThenBody; }
  [[nodiscard]] Block &getElse() const { return *ElseBody; }

  //===--------------------------------------------------------------------===//
  // Type Queries
  //===--------------------------------------------------------------------===//

  [[nodiscard]] bool hasElse() const { return ElseBody != nullptr; }

  //===--------------------------------------------------------------------===//
  // Visitor Methods
  //===--------------------------------------------------------------------===//

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  bool accept(TypeChecker &C) override;
  void accept(CodeGen &G) override;

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===--------------------------------------------------------------------===//

  static bool classof(const Stmt *S) {
    return S->getKind() == Kind::IfStmtKind;
  }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//

  void emit(int Level) const override;

private:
  std::unique_ptr<Expr> Cond;
  std::unique_ptr<Block> ThenBody;
  std::unique_ptr<Block> ElseBody;
};

class WhileStmt final : public Stmt {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  WhileStmt(SrcLocation Location, std::unique_ptr<Expr> Cond,
            std::unique_ptr<Block> Body);
  ~WhileStmt() override;

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] Expr &getCond() const { return *Cond; }
  [[nodiscard]] Block &getBody() const { return *Body; }

  //===--------------------------------------------------------------------===//
  // Visitor Methods
  //===--------------------------------------------------------------------===//

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  bool accept(TypeChecker &C) override;
  void accept(CodeGen &G) override;

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===--------------------------------------------------------------------===//

  static bool classof(const Stmt *S) {
    return S->getKind() == Kind::WhileStmtKind;
  }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//

  void emit(int Level) const override;

private:
  std::unique_ptr<Expr> Cond;
  std::unique_ptr<Block> Body;
};

class ForStmt final : public Stmt {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  ForStmt(SrcLocation Location, std::unique_ptr<VarDecl> LoopVar,
          std::unique_ptr<Expr> Range, std::unique_ptr<Block> Body);
  ~ForStmt() override;

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] VarDecl &getLoopVar() { return *LoopVar; }
  [[nodiscard]] Expr &getRange() const { return *Range; }
  [[nodiscard]] Block &getBody() const { return *Body; }

  //===--------------------------------------------------------------------===//
  // Visitor Methods
  //===--------------------------------------------------------------------===//

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  bool accept(TypeChecker &C) override;
  void accept(CodeGen &G) override;

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===--------------------------------------------------------------------===//

  static bool classof(const Stmt *S) {
    return S->getKind() == Kind::ForStmtKind;
  }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//

  void emit(int Level) const override;

private:
  std::unique_ptr<VarDecl> LoopVar;
  std::unique_ptr<Expr> Range;
  std::unique_ptr<Block> Body;
};

//===----------------------------------------------------------------------===//
// Declaration and Expression Statements
//===----------------------------------------------------------------------===//

class DeclStmt final : public Stmt {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  DeclStmt(SrcLocation Location, std::unique_ptr<VarDecl> Var);
  ~DeclStmt() override;

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] VarDecl &getDecl() const { return *Var; }

  //===--------------------------------------------------------------------===//
  // Visitor Methods
  //===--------------------------------------------------------------------===//

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  bool accept(TypeChecker &C) override;
  void accept(CodeGen &G) override;

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===--------------------------------------------------------------------===//

  static bool classof(const Stmt *S) {
    return S->getKind() == Kind::DeclStmtKind;
  }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//

  void emit(int Level) const override;

private:
  std::unique_ptr<VarDecl> Var;
};

class ExprStmt final : public Stmt {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  ExprStmt(SrcLocation Location, std::unique_ptr<Expr> Expression);
  ~ExprStmt() override;

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] Expr &getExpr() const { return *Expression; }

  //===--------------------------------------------------------------------===//
  // Visitor Methods
  //===--------------------------------------------------------------------===//

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  bool accept(TypeChecker &C) override;
  void accept(CodeGen &G) override;

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===--------------------------------------------------------------------===//

  static bool classof(const Stmt *S) {
    return S->getKind() == Kind::ExprStmtKind;
  }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//

  void emit(int Level) const override;

private:
  std::unique_ptr<Expr> Expression;
};

} // namespace phi
