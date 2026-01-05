#include "AST/Nodes/Decl.hpp"

#include <memory>
#include <print>
#include <string>

#include "AST/Nodes/Expr.hpp"
#include "AST/Nodes/Stmt.hpp"
#include "Sema/NameResolution/NameResolver.hpp"

namespace {

//===----------------------------------------------------------------------===//
// Utility Functions
//===----------------------------------------------------------------------===//

/// Generates indentation string for AST dumping
std::string indent(int Level) { return std::string(Level * 2, ' '); }

} // anonymous namespace

namespace phi {

//===----------------------------------------------------------------------===//
// Block Implementation
//===----------------------------------------------------------------------===//

// Utility Methods
void Block::emit(int Level) const {
  std::println("{}Block", indent(Level));
  for (auto &S : Stmts)
    S->emit(Level + 1);
}

//===----------------------------------------------------------------------===//
// ReturnStmt Implementation
//===----------------------------------------------------------------------===//

ReturnStmt::ReturnStmt(SrcLocation Location, std::unique_ptr<Expr> Expr)
    : Stmt(Kind::ReturnStmtKind, std::move(Location)),
      ReturnExpr(std::move(Expr)) {}

ReturnStmt::~ReturnStmt() = default;

// Utility Methods
void ReturnStmt::emit(int Level) const {
  std::println("{}ReturnStmt", indent(Level));
  if (ReturnExpr)
    ReturnExpr->emit(Level + 1);
}

//===----------------------------------------------------------------------===//
// DeferStmt Implementation
//===----------------------------------------------------------------------===//

DeferStmt::DeferStmt(SrcLocation Location, std::unique_ptr<Expr> Expr)
    : Stmt(Kind::DeferStmtKind, std::move(Location)),
      DeferredExpr(std::move(Expr)) {}

DeferStmt::~DeferStmt() = default;

// Utility Methods
void DeferStmt::emit(int Level) const {
  std::println("{}DeferStmt", indent(Level));
  if (DeferredExpr)
    DeferredExpr->emit(Level + 1);
}

//===----------------------------------------------------------------------===//
// BreakStmt Implementation
//===----------------------------------------------------------------------===//

BreakStmt::BreakStmt(SrcLocation Location)
    : Stmt(Stmt::Kind::BreakStmtKind, std::move(Location)) {}

BreakStmt::~BreakStmt() = default;

// Utility Methods
void BreakStmt::emit(int Level) const {
  std::println("{}BreakStmt", indent(Level));
}

//===----------------------------------------------------------------------===//
// ContinueStmt Implementation
//===----------------------------------------------------------------------===//

ContinueStmt::ContinueStmt(SrcLocation Location)
    : Stmt(Stmt::Kind::ContinueStmtKind, std::move(Location)) {}

ContinueStmt::~ContinueStmt() = default;

// Utility Methods
void ContinueStmt::emit(int Level) const {
  std::println("{}ContinueStmt", indent(Level));
}

//===----------------------------------------------------------------------===//
// IfStmt Implementation
//===----------------------------------------------------------------------===//

IfStmt::IfStmt(SrcLocation Location, std::unique_ptr<Expr> Cond,
               std::unique_ptr<Block> ThenBody, std::unique_ptr<Block> ElseBody)
    : Stmt(Stmt::Kind::IfStmtKind, std::move(Location)), Cond(std::move(Cond)),
      ThenBody(std::move(ThenBody)), ElseBody(std::move(ElseBody)) {}

IfStmt::~IfStmt() = default;

// Utility Methods
void IfStmt::emit(int Level) const {
  std::println("{}IfStmt", indent(Level));
  if (Cond)
    Cond->emit(Level + 1);
  if (ThenBody)
    ThenBody->emit(Level + 1);
  if (ElseBody)
    ElseBody->emit(Level + 1);
}

//===----------------------------------------------------------------------===//
// WhileStmt Implementation
//===----------------------------------------------------------------------===//

WhileStmt::WhileStmt(SrcLocation Location, std::unique_ptr<Expr> Cond,
                     std::unique_ptr<Block> Body)
    : Stmt(Stmt::Kind::WhileStmtKind, std::move(Location)),
      Cond(std::move(Cond)), Body(std::move(Body)) {}

WhileStmt::~WhileStmt() = default;

// Utility Methods
void WhileStmt::emit(int Level) const {
  std::println("{}WhileStmt", indent(Level));
  if (Cond)
    Cond->emit(Level + 1);
  if (Body)
    Body->emit(Level + 1);
}

//===----------------------------------------------------------------------===//
// ForStmt Implementation
//===----------------------------------------------------------------------===//

ForStmt::ForStmt(SrcLocation Location, std::unique_ptr<VarDecl> LoopVar,
                 std::unique_ptr<Expr> Range, std::unique_ptr<Block> Body)
    : Stmt(Stmt::Kind::ForStmtKind, std::move(Location)),
      LoopVar(std::move(LoopVar)), Range(std::move(Range)),
      Body(std::move(Body)) {}

ForStmt::~ForStmt() = default;

// Utility Methods
void ForStmt::emit(int Level) const {
  std::println("{}ForStmt", indent(Level));
  if (LoopVar)
    LoopVar->emit(Level + 1);
  if (Range)
    Range->emit(Level + 1);
  if (Body)
    Body->emit(Level + 1);
}

//===----------------------------------------------------------------------===//
// DeclStmt Implementation
//===----------------------------------------------------------------------===//

DeclStmt::DeclStmt(SrcLocation Location, std::unique_ptr<VarDecl> Var)
    : Stmt(Stmt::Kind::DeclStmtKind, std::move(Location)), Var(std::move(Var)) {
}

DeclStmt::~DeclStmt() = default;

// Utility Methods
void DeclStmt::emit(int Level) const {
  std::println("{}DeclStmt", indent(Level));
  if (Var)
    Var->emit(Level + 1);
}

//===----------------------------------------------------------------------===//
// ExprStmt Implementation
//===----------------------------------------------------------------------===//

ExprStmt::ExprStmt(SrcLocation Location, std::unique_ptr<Expr> Expression)
    : Stmt(Stmt::Kind::ExprStmtKind, std::move(Location)),
      Expression(std::move(Expression)) {}

ExprStmt::~ExprStmt() = default;

// Utility Methods
void ExprStmt::emit(int Level) const {
  std::println("{}ExprStmt", indent(Level));
  if (Expression)
    Expression->emit(Level + 1);
}

} // namespace phi
