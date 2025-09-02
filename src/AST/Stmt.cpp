#include "AST/Stmt.hpp"

#include <memory>
#include <print>
#include <string>

#include "AST/Decl.hpp"
#include "AST/Expr.hpp"
#include "CodeGen/CodeGen.hpp"
#include "Sema/NameResolver.hpp"
#include "Sema/TypeInference/Infer.hpp"

namespace {
std::string indent(int Level) { return std::string(Level * 2, ' '); }
} // namespace

namespace phi {

//======================== Control Flow Statements =========================//

// IfStmt
IfStmt::IfStmt(SrcLocation Location, std::unique_ptr<Expr> Cond,
               std::unique_ptr<Block> ThenBody, std::unique_ptr<Block> ElseBody)
    : Stmt(Stmt::Kind::IfStmtKind, std::move(Location)), Cond(std::move(Cond)),
      ThenBody(std::move(ThenBody)), ElseBody(std::move(ElseBody)) {}
IfStmt::~IfStmt() = default;

void IfStmt::emit(int Level) const {
  std::println("{}IfStmt", indent(Level));
  if (Cond)
    Cond->emit(Level + 1);
  if (ThenBody)
    ThenBody->emit(Level + 1);
  if (ElseBody)
    ElseBody->emit(Level + 1);
}

bool IfStmt::accept(NameResolver &R) { return R.visit(*this); }
InferRes IfStmt::accept(TypeInferencer &I) { return I.visit(*this); }
void IfStmt::accept(CodeGen &G) { return G.visit(*this); }

// WhileStmt
WhileStmt::WhileStmt(SrcLocation Location, std::unique_ptr<Expr> Cond,
                     std::unique_ptr<Block> Body)
    : Stmt(Stmt::Kind::WhileStmtKind, std::move(Location)),
      Cond(std::move(Cond)), Body(std::move(Body)) {}
WhileStmt::~WhileStmt() = default;

void WhileStmt::emit(int Level) const {
  std::println("{}WhileStmt", indent(Level));
  if (Cond)
    Cond->emit(Level + 1);
  if (Body)
    Body->emit(Level + 1);
}

bool WhileStmt::accept(NameResolver &R) { return R.visit(*this); }
InferRes WhileStmt::accept(TypeInferencer &I) { return I.visit(*this); }
void WhileStmt::accept(CodeGen &G) { return G.visit(*this); }

// ForStmt
ForStmt::ForStmt(SrcLocation Location, std::unique_ptr<VarDecl> LoopVar,
                 std::unique_ptr<Expr> Range, std::unique_ptr<Block> Body)
    : Stmt(Stmt::Kind::ForStmtKind, std::move(Location)),
      LoopVar(std::move(LoopVar)), Range(std::move(Range)),
      Body(std::move(Body)) {}
ForStmt::~ForStmt() = default;

void ForStmt::emit(int Level) const {
  std::println("{}ForStmt", indent(Level));
  if (LoopVar)
    LoopVar->emit(Level + 1);
  if (Range)
    Range->emit(Level + 1);
  if (Body)
    Body->emit(Level + 1);
}

bool ForStmt::accept(NameResolver &R) { return R.visit(*this); }
InferRes ForStmt::accept(TypeInferencer &I) { return I.visit(*this); }
void ForStmt::accept(CodeGen &G) { return G.visit(*this); }

// BreakStmt
BreakStmt::BreakStmt(SrcLocation Location)
    : Stmt(Stmt::Kind::BreakStmtKind, std::move(Location)) {}
BreakStmt::~BreakStmt() = default;

void BreakStmt::emit(int Level) const {
  std::println("{}BreakStmt", indent(Level));
}

bool BreakStmt::accept(NameResolver &R) { return R.visit(*this); }
InferRes BreakStmt::accept(TypeInferencer &I) { return I.visit(*this); }
void BreakStmt::accept(CodeGen &G) { return G.visit(*this); }

// ContinueStmt
ContinueStmt::ContinueStmt(SrcLocation Location)
    : Stmt(Stmt::Kind::ContinueStmtKind, std::move(Location)) {}
ContinueStmt::~ContinueStmt() = default;

void ContinueStmt::emit(int Level) const {
  std::println("{}ContinueStmt", indent(Level));
}

bool ContinueStmt::accept(NameResolver &R) { return R.visit(*this); }
InferRes ContinueStmt::accept(TypeInferencer &I) { return I.visit(*this); }
void ContinueStmt::accept(CodeGen &G) { return G.visit(*this); }

//======================== Declarations and Return =========================//

// ReturnStmt
ReturnStmt::ReturnStmt(SrcLocation Location, std::unique_ptr<Expr> ReturnExpr)
    : Stmt(Kind::ReturnStmtKind, std::move(Location)),
      ReturnExpr(std::move(ReturnExpr)) {}
ReturnStmt::~ReturnStmt() = default;

void ReturnStmt::emit(int Level) const {
  std::println("{}ReturnStmt", indent(Level));
  if (ReturnExpr)
    ReturnExpr->emit(Level + 1);
}

bool ReturnStmt::accept(NameResolver &R) { return R.visit(*this); }
InferRes ReturnStmt::accept(TypeInferencer &I) { return I.visit(*this); }
void ReturnStmt::accept(CodeGen &G) { return G.visit(*this); }

// DeclStmt
DeclStmt::DeclStmt(SrcLocation Location, std::unique_ptr<VarDecl> Var)
    : Stmt(Stmt::Kind::DeclStmtKind, std::move(Location)), Var(std::move(Var)) {
}
DeclStmt::~DeclStmt() = default;

void DeclStmt::emit(int Level) const {
  std::println("{}VarDeclStmt", indent(Level));
  if (Var)
    Var->emit(Level + 1);
}

bool DeclStmt::accept(NameResolver &R) { return R.visit(*this); }
InferRes DeclStmt::accept(TypeInferencer &I) { return I.visit(*this); }
void DeclStmt::accept(CodeGen &G) { return G.visit(*this); }

//======================== Block Implementation =========================//
void Block::emit(int Level) const {
  std::println("{}Block", indent(Level));
  for (auto &s : Stmts)
    s->emit(Level + 1);
}

} // namespace phi
