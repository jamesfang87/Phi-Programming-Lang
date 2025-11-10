#include "AST/Stmt.hpp"

#include <memory>
#include <print>
#include <string>

#include "AST/Decl.hpp"
#include "AST/Expr.hpp"
#include "CodeGen/CodeGen.hpp"
#include "Sema/NameResolver.hpp"
#include "Sema/TypeChecker.hpp"
#include "Sema/TypeInference/Infer.hpp"

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

// Constructors & Destructors
ReturnStmt::ReturnStmt(SrcLocation Location, std::unique_ptr<Expr> Expr)
    : Stmt(Kind::ReturnStmtKind, std::move(Location)),
      ReturnExpr(std::move(Expr)) {}

ReturnStmt::~ReturnStmt() = default;

// Visitor Methods
bool ReturnStmt::accept(NameResolver &R) { return R.visit(*this); }
InferRes ReturnStmt::accept(TypeInferencer &I) { return I.visit(*this); }
bool ReturnStmt::accept(TypeChecker &C) { return C.visit(*this); }
void ReturnStmt::accept(CodeGen &G) { return G.visit(*this); }

// Utility Methods
void ReturnStmt::emit(int Level) const {
  std::println("{}ReturnStmt", indent(Level));
  if (ReturnExpr)
    ReturnExpr->emit(Level + 1);
}

//===----------------------------------------------------------------------===//
// DeferStmt Implementation
//===----------------------------------------------------------------------===//

// Constructors & Destructors
DeferStmt::DeferStmt(SrcLocation Location, std::unique_ptr<Expr> Expr)
    : Stmt(Kind::DeferStmtKind, std::move(Location)),
      DeferredExpr(std::move(Expr)) {}

DeferStmt::~DeferStmt() = default;

// Visitor Methods
bool DeferStmt::accept(NameResolver &R) { return R.visit(*this); }
InferRes DeferStmt::accept(TypeInferencer &I) { return I.visit(*this); }
bool DeferStmt::accept(TypeChecker &C) { return C.visit(*this); }
void DeferStmt::accept(CodeGen &G) { return G.visit(*this); }

// Utility Methods
void DeferStmt::emit(int Level) const {
  std::println("{}DeferStmt", indent(Level));
  if (DeferredExpr)
    DeferredExpr->emit(Level + 1);
}

//===----------------------------------------------------------------------===//
// BreakStmt Implementation
//===----------------------------------------------------------------------===//

// Constructors & Destructors
BreakStmt::BreakStmt(SrcLocation Location)
    : Stmt(Stmt::Kind::BreakStmtKind, std::move(Location)) {}

BreakStmt::~BreakStmt() = default;

// Visitor Methods
bool BreakStmt::accept(NameResolver &R) { return R.visit(*this); }
InferRes BreakStmt::accept(TypeInferencer &I) { return I.visit(*this); }
bool BreakStmt::accept(TypeChecker &C) { return C.visit(*this); }
void BreakStmt::accept(CodeGen &G) { return G.visit(*this); }

// Utility Methods
void BreakStmt::emit(int Level) const {
  std::println("{}BreakStmt", indent(Level));
}

//===----------------------------------------------------------------------===//
// ContinueStmt Implementation
//===----------------------------------------------------------------------===//

// Constructors & Destructors
ContinueStmt::ContinueStmt(SrcLocation Location)
    : Stmt(Stmt::Kind::ContinueStmtKind, std::move(Location)) {}

ContinueStmt::~ContinueStmt() = default;

// Visitor Methods
bool ContinueStmt::accept(NameResolver &R) { return R.visit(*this); }
InferRes ContinueStmt::accept(TypeInferencer &I) { return I.visit(*this); }
bool ContinueStmt::accept(TypeChecker &C) { return C.visit(*this); }
void ContinueStmt::accept(CodeGen &G) { return G.visit(*this); }

// Utility Methods
void ContinueStmt::emit(int Level) const {
  std::println("{}ContinueStmt", indent(Level));
}

//===----------------------------------------------------------------------===//
// IfStmt Implementation
//===----------------------------------------------------------------------===//

// Constructors & Destructors
IfStmt::IfStmt(SrcLocation Location, std::unique_ptr<Expr> Cond,
               std::unique_ptr<Block> ThenBody, std::unique_ptr<Block> ElseBody)
    : Stmt(Stmt::Kind::IfStmtKind, std::move(Location)), Cond(std::move(Cond)),
      ThenBody(std::move(ThenBody)), ElseBody(std::move(ElseBody)) {}

IfStmt::~IfStmt() = default;

// Visitor Methods
bool IfStmt::accept(NameResolver &R) { return R.visit(*this); }
InferRes IfStmt::accept(TypeInferencer &I) { return I.visit(*this); }
bool IfStmt::accept(TypeChecker &C) { return C.visit(*this); }
void IfStmt::accept(CodeGen &G) { return G.visit(*this); }

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

// Constructors & Destructors
WhileStmt::WhileStmt(SrcLocation Location, std::unique_ptr<Expr> Cond,
                     std::unique_ptr<Block> Body)
    : Stmt(Stmt::Kind::WhileStmtKind, std::move(Location)),
      Cond(std::move(Cond)), Body(std::move(Body)) {}

WhileStmt::~WhileStmt() = default;

// Visitor Methods
bool WhileStmt::accept(NameResolver &R) { return R.visit(*this); }
InferRes WhileStmt::accept(TypeInferencer &I) { return I.visit(*this); }
bool WhileStmt::accept(TypeChecker &C) { return C.visit(*this); }
void WhileStmt::accept(CodeGen &G) { return G.visit(*this); }

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

// Constructors & Destructors
ForStmt::ForStmt(SrcLocation Location, std::unique_ptr<VarDecl> LoopVar,
                 std::unique_ptr<Expr> Range, std::unique_ptr<Block> Body)
    : Stmt(Stmt::Kind::ForStmtKind, std::move(Location)),
      LoopVar(std::move(LoopVar)), Range(std::move(Range)),
      Body(std::move(Body)) {}

ForStmt::~ForStmt() = default;

// Visitor Methods
bool ForStmt::accept(NameResolver &R) { return R.visit(*this); }
InferRes ForStmt::accept(TypeInferencer &I) { return I.visit(*this); }
bool ForStmt::accept(TypeChecker &C) { return C.visit(*this); }
void ForStmt::accept(CodeGen &G) { return G.visit(*this); }

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

// Constructors & Destructors
DeclStmt::DeclStmt(SrcLocation Location, std::unique_ptr<VarDecl> Var)
    : Stmt(Stmt::Kind::DeclStmtKind, std::move(Location)), Var(std::move(Var)) {
}

DeclStmt::~DeclStmt() = default;

// Visitor Methods
bool DeclStmt::accept(NameResolver &R) { return R.visit(*this); }
InferRes DeclStmt::accept(TypeInferencer &I) { return I.visit(*this); }
bool DeclStmt::accept(TypeChecker &C) { return C.visit(*this); }
void DeclStmt::accept(CodeGen &G) { return G.visit(*this); }

// Utility Methods
void DeclStmt::emit(int Level) const {
  std::println("{}VarDeclStmt", indent(Level));
  if (Var)
    Var->emit(Level + 1);
}

//===----------------------------------------------------------------------===//
// ExprStmt Implementation
//===----------------------------------------------------------------------===//

// Constructors & Destructors
ExprStmt::ExprStmt(SrcLocation Location, std::unique_ptr<Expr> Expression)
    : Stmt(Stmt::Kind::ExprStmtKind, std::move(Location)),
      Expression(std::move(Expression)) {}

ExprStmt::~ExprStmt() = default;

// Visitor Methods
bool ExprStmt::accept(NameResolver &R) { return R.visit(*this); }
InferRes ExprStmt::accept(TypeInferencer &I) { return I.visit(*this); }
bool ExprStmt::accept(TypeChecker &C) { return C.visit(*this); }
void ExprStmt::accept(CodeGen &G) { return G.visit(*this); }

// Utility Methods
void ExprStmt::emit(int Level) const {
  std::println("{}ExprStmt", indent(Level));
  if (Expression)
    Expression->emit(Level + 1);
}

} // namespace phi
