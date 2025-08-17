#include "AST/Stmt.hpp"

#include <memory>
#include <print>
#include <string>

#include "AST/ASTVisitor.hpp"
#include "AST/Decl.hpp"
#include "AST/Expr.hpp"
#include "Sema/HMTI/Infer.hpp"
#include "Sema/NameResolver.hpp"

namespace {
/// Generates indentation string for AST dumping
/// @param Level Current indentation Level
/// @return String of spaces for indentation
std::string indent(int Level) { return std::string(Level * 2, ' '); }
} // namespace

namespace phi {

//======================== ReturnStmt Implementation =========================//

/**
 * @brief Constructs a return statement
 *
 * @param Location Source Location of return
 * @param expr Return value expression (optional)
 */
ReturnStmt::ReturnStmt(SrcLocation Location, std::unique_ptr<Expr> ReturnExpr)
    : Stmt(Kind::ReturnStmtKind, std::move(Location)),
      ReturnExpr(std::move(ReturnExpr)) {}

ReturnStmt::~ReturnStmt() = default;

/**
 * @brief Dumps return statement information
 *
 * Output format:
 *   [indent]ReturnStmt
 *     [child expression dump] (if exists)
 *
 * @param Level Current indentation Level
 */
void ReturnStmt::emit(int Level) const {
  std::println("{}ReturnStmt", indent(Level));
  if (ReturnExpr) {
    ReturnExpr->emit(Level + 1);
  }
}

bool ReturnStmt::accept(ASTVisitor<bool> &Visitor) {
  return Visitor.visit(*this);
}

bool ReturnStmt::accept(NameResolver &R) { return R.visit(*this); };
InferRes ReturnStmt::accept(TypeInferencer &I) { return I.visit(*this); }

void ReturnStmt::accept(ASTVisitor<void> &Visitor) { Visitor.visit(*this); }

//======================== IfStmt Implementation =========================//

/**
 * @brief Constructs an if statement
 *
 * @param Location Source Location of if
 * @param cond condal expression
 * @param then_block Then clause block
 * @param else_block Else clause block (optional)
 */
IfStmt::IfStmt(SrcLocation Location, std::unique_ptr<Expr> Cond,
               std::unique_ptr<Block> ThenBody, std::unique_ptr<Block> ElseBody)
    : Stmt(Stmt::Kind::IfStmtKind, std::move(Location)), Cond(std::move(Cond)),
      ThenBody(std::move(ThenBody)), ElseBody(std::move(ElseBody)) {}

IfStmt::~IfStmt() = default;

/**
 * @brief Dumps if statement information
 *
 * Output format:
 *   [indent]IfStmt
 *     [cond dump]
 *     [then block dump]
 *     [else block dump] (if exists)
 *
 * @param Level Current indentation Level
 */
void IfStmt::emit(int Level) const {
  std::println("{}IfStmt", indent(Level));
  if (Cond) {
    Cond->emit(Level + 1);
  }
  if (ThenBody) {
    ThenBody->emit(Level + 1);
  }
  if (ElseBody) {
    ElseBody->emit(Level + 1);
  }
}

void IfStmt::accept(ASTVisitor<void> &Visitor) { Visitor.visit(*this); }

bool IfStmt::accept(NameResolver &R) { return R.visit(*this); };
InferRes IfStmt::accept(TypeInferencer &I) { return I.visit(*this); }
bool IfStmt::accept(ASTVisitor<bool> &Visitor) { return Visitor.visit(*this); }

//======================== WhileStmt Implementation =========================//

/**
 * @brief Constructs a while loop statement
 *
 * @param Location Source Location of while
 * @param cond Loop cond expression
 * @param body Loop body block
 */
WhileStmt::WhileStmt(SrcLocation Location, std::unique_ptr<Expr> Cond,
                     std::unique_ptr<Block> Body)
    : Stmt(Stmt::Kind::WhileStmtKind, std::move(Location)),
      Cond(std::move(Cond)), Body(std::move(Body)) {}

WhileStmt::~WhileStmt() = default;

/**
 * @brief Dumps while loop information
 *
 * Output format:
 *   [indent]WhileStmt
 *     [cond dump]
 *     [body dump]
 *
 * @param Level Current indentation Level
 */
void WhileStmt::emit(int Level) const {
  std::println("{}WhileStmt", indent(Level));
  if (Cond) {
    Cond->emit(Level + 1);
  }
  if (Body) {
    Body->emit(Level + 1);
  }
}

bool WhileStmt::accept(ASTVisitor<bool> &Visitor) {
  return Visitor.visit(*this);
}

bool WhileStmt::accept(NameResolver &R) { return R.visit(*this); };
InferRes WhileStmt::accept(TypeInferencer &I) { return I.visit(*this); }
void WhileStmt::accept(ASTVisitor<void> &Visitor) { Visitor.visit(*this); }

//======================== ForStmt Implementation =========================//

/**
 * @brief Constructs a for loop statement
 *
 * @param Location Source Location of for
 * @param loop_var Loop variable declaration
 * @param range Range expression
 * @param body Loop body block
 */
ForStmt::ForStmt(SrcLocation Location, std::unique_ptr<VarDecl> LoopVar,
                 std::unique_ptr<Expr> Range, std::unique_ptr<Block> Body)
    : Stmt(Stmt::Kind::ForStmtKind, std::move(Location)),
      LoopVar(std::move(LoopVar)), Range(std::move(Range)),
      Body(std::move(Body)) {}

ForStmt::~ForStmt() = default;

/**
 * @brief Dumps for loop information
 *
 * Output format:
 *   [indent]ForStmt
 *     [loop variable dump]
 *     [range expression dump]
 *     [body dump]
 *
 * @param Level Current indentation Level
 */
void ForStmt::emit(int Level) const {
  std::println("{}ForStmt", indent(Level));
  if (LoopVar) {
    LoopVar->emit(Level + 1);
  }
  if (Range) {
    Range->emit(Level + 1);
  }
  if (Body) {
    Body->emit(Level + 1);
  }
}

bool ForStmt::accept(ASTVisitor<bool> &Visitor) { return Visitor.visit(*this); }

bool ForStmt::accept(NameResolver &R) { return R.visit(*this); };
InferRes ForStmt::accept(TypeInferencer &I) { return I.visit(*this); }
void ForStmt::accept(ASTVisitor<void> &Visitor) { Visitor.visit(*this); }

//======================== LetStmt Implementation =========================//

/**
 * @brief Constructs a variable declaration statement
 *
 * @param Location Source Location of declaration
 * @param decl Variable declaration
 */
DeclStmt::DeclStmt(SrcLocation Location, std::unique_ptr<VarDecl> Var)
    : Stmt(Stmt::Kind::DeclStmtKind, std::move(Location)), Var(std::move(Var)) {
}

DeclStmt::~DeclStmt() = default;

/**
 * @brief Dumps variable declaration statement information
 *
 * Output format:
 *   [indent]VarDeclStmt
 *     [variable declaration dump]
 *
 * @param Level Current indentation Level
 */
void DeclStmt::emit(int Level) const {
  std::println("{}VarDeclStmt", indent(Level));
  if (Var) {
    Var->emit(Level + 1);
  }
}

bool DeclStmt::accept(ASTVisitor<bool> &Visitor) {
  return Visitor.visit(*this);
}

bool DeclStmt::accept(NameResolver &R) { return R.visit(*this); };
InferRes DeclStmt::accept(TypeInferencer &I) { return I.visit(*this); }
void DeclStmt::accept(ASTVisitor<void> &Visitor) { Visitor.visit(*this); }

//======================== BreakStmt Implementation =========================//

/**
 * @brief Constructs a break statement
 *
 * @param Location Source Location of break statement
 */
BreakStmt::BreakStmt(SrcLocation Location)
    : Stmt(Stmt::Kind::BreakStmtKind, std::move(Location)) {}

BreakStmt::~BreakStmt() = default;

/**
 * @brief Dumps break statement information
 *
 * Output format:
 *   [indent]BreakStmt
 *
 * @param Level Current indentation Level
 */
void BreakStmt::emit(int Level) const {
  std::println("{}BreakStmt", indent(Level));
}

bool BreakStmt::accept(ASTVisitor<bool> &Visitor) {
  return Visitor.visit(*this);
}

bool BreakStmt::accept(NameResolver &R) { return R.visit(*this); };
InferRes BreakStmt::accept(TypeInferencer &I) { return I.visit(*this); }
void BreakStmt::accept(ASTVisitor<void> &Visitor) { Visitor.visit(*this); }

//======================= ContinueStmt Implementation ========================//

/**
 * @brief Constructs a continue statement
 *
 * @param Location Source Location of continue statement
 */
ContinueStmt::ContinueStmt(SrcLocation Location)
    : Stmt(Stmt::Kind::ContinueStmtKind, std::move(Location)) {}

ContinueStmt::~ContinueStmt() = default;

/**
 * @brief Dumps continue statement information
 *
 * Output format:
 *   [indent]ContinueStmt
 *
 * @param Level Current indentation Level
 */
void ContinueStmt::emit(int Level) const {
  std::println("{}ContinueStmt", indent(Level));
}

bool ContinueStmt::accept(ASTVisitor<bool> &Visitor) {
  return Visitor.visit(*this);
}

bool ContinueStmt::accept(NameResolver &R) { return R.visit(*this); };
InferRes ContinueStmt::accept(TypeInferencer &I) { return I.visit(*this); }
void ContinueStmt::accept(ASTVisitor<void> &Visitor) { Visitor.visit(*this); }

//======================== Block Implementation =========================//

/**
 * @brief Dumps block statement information
 *
 * Output format:
 *   [indent]Block
 *     [statement dumps]
 *
 * @param Level Current indentation Level
 */
void Block::emit(int Level) const {
  std::println("{}Block", indent(Level));
  for (auto &s : this->Stmts) {
    s->emit(Level + 1);
  }
}

} // namespace phi
