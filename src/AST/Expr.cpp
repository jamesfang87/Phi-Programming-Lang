#include "AST/Expr.hpp"

#include <cassert>
#include <optional>
#include <print>
#include <string>
#include <utility>

#include "AST/Decl.hpp"
#include "AST/Stmt.hpp"
#include "AST/Type.hpp"
#include "CodeGen/CodeGen.hpp"
#include "Lexer/TokenKind.hpp"
#include "Sema/NameResolver.hpp"
#include "Sema/TypeChecker.hpp"
#include "Sema/TypeInference/Infer.hpp"
#include <llvm/IR/Value.h>

namespace {

//===----------------------------------------------------------------------===//
// Utility Functions
//===----------------------------------------------------------------------===//

/// Generates indentation string for AST dumping
std::string indent(int Level) { return std::string(Level * 2, ' '); }

} // anonymous namespace

namespace phi {

//===----------------------------------------------------------------------===//
// Base Expr Implementation
//===----------------------------------------------------------------------===//

// Default visitor implementations (can be overridden by subclasses)
bool Expr::accept(NameResolver &R) { return R.visit(*this); }
InferRes Expr::accept(TypeInferencer &I) { return I.visit(*this); }
bool Expr::accept(TypeChecker &C) { return C.visit(*this); }
llvm::Value *Expr::accept(CodeGen &G) { return G.visit(*this); }

//===----------------------------------------------------------------------===//
// IntLiteral Implementation
//===----------------------------------------------------------------------===//

// Constructors & Destructors
IntLiteral::IntLiteral(SrcLocation Location, const int64_t Value)
    : Expr(Expr::Kind::IntLiteralKind, std::move(Location), std::nullopt),
      Value(Value) {}

// Visitor Methods
bool IntLiteral::accept(NameResolver &R) { return R.visit(*this); }
InferRes IntLiteral::accept(TypeInferencer &I) { return I.visit(*this); }
bool IntLiteral::accept(TypeChecker &C) { return C.visit(*this); }
llvm::Value *IntLiteral::accept(CodeGen &G) { return G.visit(*this); }

// Utility Methods
void IntLiteral::emit(int Level) const {
  std::string typeStr = Ty.has_value() ? Ty.value().toString() : "<unresolved>";
  std::println("{}IntLiteral: {} (type: {})", indent(Level), Value, typeStr);
}

//===----------------------------------------------------------------------===//
// FloatLiteral Implementation
//===----------------------------------------------------------------------===//

// Constructors & Destructors
FloatLiteral::FloatLiteral(SrcLocation Location, const double Value)
    : Expr(Expr::Kind::FloatLiteralKind, std::move(Location), std::nullopt),
      Value(Value) {}

// Visitor Methods
bool FloatLiteral::accept(NameResolver &R) { return R.visit(*this); }
InferRes FloatLiteral::accept(TypeInferencer &I) { return I.visit(*this); }
bool FloatLiteral::accept(TypeChecker &C) { return C.visit(*this); }
llvm::Value *FloatLiteral::accept(CodeGen &G) { return G.visit(*this); }

// Utility Methods
void FloatLiteral::emit(int Level) const {
  std::string typeStr = Ty.has_value() ? Ty.value().toString() : "<unresolved>";
  std::println("{}FloatLiteral: {} (type: {})", indent(Level), Value, typeStr);
}

//===----------------------------------------------------------------------===//
// StrLiteral Implementation
//===----------------------------------------------------------------------===//

// Constructors & Destructors
StrLiteral::StrLiteral(SrcLocation Location, std::string Value)
    : Expr(Expr::Kind::StrLiteralKind, Location,
           Type::makePrimitive(PrimitiveKind::String, std::move(Location))),
      Value(std::move(Value)) {}

// Visitor Methods
bool StrLiteral::accept(NameResolver &R) { return R.visit(*this); }
InferRes StrLiteral::accept(TypeInferencer &I) { return I.visit(*this); }
bool StrLiteral::accept(TypeChecker &C) { return C.visit(*this); }
llvm::Value *StrLiteral::accept(CodeGen &G) { return G.visit(*this); }

// Utility Methods
void StrLiteral::emit(int Level) const {
  std::println("{}StrLiteral: {}", indent(Level), Value);
}

//===----------------------------------------------------------------------===//
// CharLiteral Implementation
//===----------------------------------------------------------------------===//

// Constructors & Destructors
CharLiteral::CharLiteral(SrcLocation Location, char Value)
    : Expr(Expr::Kind::CharLiteralKind, Location,
           Type::makePrimitive(PrimitiveKind::Char, std::move(Location))),
      Value(Value) {}

// Visitor Methods
bool CharLiteral::accept(NameResolver &R) { return R.visit(*this); }
InferRes CharLiteral::accept(TypeInferencer &I) { return I.visit(*this); }
bool CharLiteral::accept(TypeChecker &C) { return C.visit(*this); }
llvm::Value *CharLiteral::accept(CodeGen &G) { return G.visit(*this); }

// Utility Methods
void CharLiteral::emit(int Level) const {
  std::println("{}CharLiteral: {}", indent(Level), Value);
}

//===----------------------------------------------------------------------===//
// BoolLiteral Implementation
//===----------------------------------------------------------------------===//

// Constructors & Destructors
BoolLiteral::BoolLiteral(SrcLocation Location, bool Value)
    : Expr(Expr::Kind::BoolLiteralKind, Location,
           Type::makePrimitive(PrimitiveKind::Bool, std::move(Location))),
      Value(Value) {}

// Visitor Methods
bool BoolLiteral::accept(NameResolver &R) { return R.visit(*this); }
InferRes BoolLiteral::accept(TypeInferencer &I) { return I.visit(*this); }
bool BoolLiteral::accept(TypeChecker &C) { return C.visit(*this); }
llvm::Value *BoolLiteral::accept(CodeGen &G) { return G.visit(*this); }

// Utility Methods
void BoolLiteral::emit(int Level) const {
  std::println("{}BoolLiteral: {}", indent(Level), Value);
}

//===----------------------------------------------------------------------===//
// RangeLiteral Implementation
//===----------------------------------------------------------------------===//

// Constructors & Destructors
RangeLiteral::RangeLiteral(SrcLocation Location, std::unique_ptr<Expr> Start,
                           std::unique_ptr<Expr> End, const bool Inclusive)
    : Expr(Expr::Kind::RangeLiteralKind, Location,
           Type::makePrimitive(PrimitiveKind::Range, std::move(Location))),
      Start(std::move(Start)), End(std::move(End)), Inclusive(Inclusive) {}

RangeLiteral::~RangeLiteral() = default;

// Visitor Methods
bool RangeLiteral::accept(NameResolver &R) { return R.visit(*this); }
InferRes RangeLiteral::accept(TypeInferencer &I) { return I.visit(*this); }
bool RangeLiteral::accept(TypeChecker &C) { return C.visit(*this); }
llvm::Value *RangeLiteral::accept(CodeGen &G) { return G.visit(*this); }

// Utility Methods
void RangeLiteral::emit(int Level) const {
  std::println("{}RangeLiteral:", indent(Level));
  std::println("{}  start:", indent(Level));
  Start->emit(Level + 2);
  std::println("{}  end:", indent(Level));
  End->emit(Level + 2);
}

//===----------------------------------------------------------------------===//
// DeclRefExpr Implementation
//===----------------------------------------------------------------------===//

// Constructors & Destructors
DeclRefExpr::DeclRefExpr(SrcLocation Location, std::string Id)
    : Expr(Expr::Kind::DeclRefExprKind, std::move(Location)),
      Id(std::move(Id)) {}

// Visitor Methods
bool DeclRefExpr::accept(NameResolver &R) { return R.visit(*this); }
InferRes DeclRefExpr::accept(TypeInferencer &I) { return I.visit(*this); }
bool DeclRefExpr::accept(TypeChecker &C) { return C.visit(*this); }
llvm::Value *DeclRefExpr::accept(CodeGen &G) { return G.visit(*this); }

// Utility Methods
void DeclRefExpr::emit(int Level) const {
  if (DeclPtr == nullptr) {
    std::println("{}DeclRefExpr: {} ", indent(Level), Id);
  } else {
    std::string typeStr =
        DeclPtr->hasType() ? DeclPtr->getType().toString() : "<unresolved>";

    std::println("{}DeclRefExpr: {}; referring to: {} of type {}",
                 indent(Level), Id, DeclPtr->getId(), typeStr);
  }
}

//===----------------------------------------------------------------------===//
// FunCallExpr Implementation
//===----------------------------------------------------------------------===//

// Constructors & Destructors
FunCallExpr::FunCallExpr(SrcLocation Location, std::unique_ptr<Expr> Callee,
                         std::vector<std::unique_ptr<Expr>> Args)
    : Expr(Expr::Kind::FunCallExprKind, std::move(Location)),
      Callee(std::move(Callee)), Args(std::move(Args)) {}

// Protected constructor for derived classes
FunCallExpr::FunCallExpr(Kind K, SrcLocation Location,
                         std::unique_ptr<Expr> Callee,
                         std::vector<std::unique_ptr<Expr>> Args)
    : Expr(K, std::move(Location)), Callee(std::move(Callee)),
      Args(std::move(Args)) {}

FunCallExpr::~FunCallExpr() = default;

// Visitor Methods
bool FunCallExpr::accept(NameResolver &R) { return R.visit(*this); }
InferRes FunCallExpr::accept(TypeInferencer &I) { return I.visit(*this); }
bool FunCallExpr::accept(TypeChecker &C) { return C.visit(*this); }
llvm::Value *FunCallExpr::accept(CodeGen &G) { return G.visit(*this); }

// Utility Methods
void FunCallExpr::emit(int Level) const {
  std::println("{}FunCallExpr", indent(Level));
  if (getDecl() != nullptr)
    std::println("{}  calling: {}", indent(Level), getDecl()->getId());
  std::println("{}  callee:", indent(Level));
  Callee->emit(Level + 2);
  std::println("{}  args:", indent(Level));
  for (const auto &arg : Args) {
    arg->emit(Level + 2);
  }
}

//===----------------------------------------------------------------------===//
// BinaryOp Implementation
//===----------------------------------------------------------------------===//

// Constructors & Destructors
BinaryOp::BinaryOp(std::unique_ptr<Expr> Lhs, std::unique_ptr<Expr> Rhs,
                   const Token &Op)
    : Expr(Expr::Kind::BinaryOpKind, Op.getStart()), Lhs(std::move(Lhs)),
      Rhs(std::move(Rhs)), Op(Op.getKind()) {}

BinaryOp::~BinaryOp() = default;

// Visitor Methods
bool BinaryOp::accept(NameResolver &R) { return R.visit(*this); }
InferRes BinaryOp::accept(TypeInferencer &I) { return I.visit(*this); }
bool BinaryOp::accept(TypeChecker &C) { return C.visit(*this); }
llvm::Value *BinaryOp::accept(CodeGen &G) { return G.visit(*this); }

// Utility Methods
void BinaryOp::emit(int Level) const {
  std::println("{}BinaryOp: {}", indent(Level), tyToStr(Op));
  std::println("{}  lhs:", indent(Level));
  Lhs->emit(Level + 2);
  std::println("{}  rhs:", indent(Level));
  Rhs->emit(Level + 2);
  if (Ty.has_value()) {
    std::println("{}  type: {}", indent(Level), Ty.value().toString());
  }
}

//===----------------------------------------------------------------------===//
// UnaryOp Implementation
//===----------------------------------------------------------------------===//

// Constructors & Destructors
UnaryOp::UnaryOp(std::unique_ptr<Expr> Operand, const Token &Op,
                 const bool IsPrefix)
    : Expr(Expr::Kind::UnaryOpKind, Op.getStart()), Operand(std::move(Operand)),
      Op(Op.getKind()), IsPrefix(IsPrefix) {}

UnaryOp::~UnaryOp() = default;

// Visitor Methods
bool UnaryOp::accept(NameResolver &R) { return R.visit(*this); }
InferRes UnaryOp::accept(TypeInferencer &I) { return I.visit(*this); }
bool UnaryOp::accept(TypeChecker &C) { return C.visit(*this); }
llvm::Value *UnaryOp::accept(CodeGen &G) { return G.visit(*this); }

// Utility Methods
void UnaryOp::emit(int Level) const {
  std::println("{}UnaryOp: {}", indent(Level), tyToStr(Op));
  std::println("{}  expr:", indent(Level));
  Operand->emit(Level + 2);
}

//===----------------------------------------------------------------------===//
// FieldInitExpr Implementation
//===----------------------------------------------------------------------===//

// Constructors & Destructors
FieldInitExpr::FieldInitExpr(SrcLocation Location, std::string FieldId,
                             std::unique_ptr<Expr> Init)
    : Expr(Expr::Kind::FieldInitKind, std::move(Location)),
      FieldId(std::move(FieldId)), Init(std::move(Init)) {}

FieldInitExpr::~FieldInitExpr() = default;

// Visitor Methods
bool FieldInitExpr::accept(NameResolver &R) { return R.visit(*this); }
InferRes FieldInitExpr::accept(TypeInferencer &I) { return I.visit(*this); }
bool FieldInitExpr::accept(TypeChecker &C) { return C.visit(*this); }
llvm::Value *FieldInitExpr::accept(CodeGen &G) { return G.visit(*this); }

// Utility Methods
void FieldInitExpr::emit(int Level) const {
  std::println("{}FieldInitExpr:", indent(Level));
  std::println("{}  field: {}", indent(Level), FieldId);
  std::println("{}  value:", indent(Level));
  Init->emit(Level + 2);
}

//===----------------------------------------------------------------------===//
// StructInitExpr Implementation
//===----------------------------------------------------------------------===//

// Constructors & Destructors
StructInitExpr::StructInitExpr(
    SrcLocation Location, std::string StructId,
    std::vector<std::unique_ptr<FieldInitExpr>> Fields)
    : Expr(Expr::Kind::StructInitKind, std::move(Location)),
      StructId(std::move(StructId)), FieldInits(std::move(Fields)) {}

StructInitExpr::~StructInitExpr() = default;

// Visitor Methods
bool StructInitExpr::accept(NameResolver &R) { return R.visit(*this); }
InferRes StructInitExpr::accept(TypeInferencer &I) { return I.visit(*this); }
bool StructInitExpr::accept(TypeChecker &C) { return C.visit(*this); }
llvm::Value *StructInitExpr::accept(CodeGen &G) { return G.visit(*this); }

// Utility Methods
void StructInitExpr::emit(int Level) const {
  std::println("{}StructInitExpr:", indent(Level));
  std::println("{}  struct: {}", indent(Level), StructId);
  std::println("{}  fields:", indent(Level));
  for (const auto &Field : FieldInits) {
    Field->emit(Level + 2);
  }
}

//===----------------------------------------------------------------------===//
// FieldAccessExpr Implementation
//===----------------------------------------------------------------------===//

// Constructors & Destructors
FieldAccessExpr::FieldAccessExpr(SrcLocation Location,
                                 std::unique_ptr<Expr> Base,
                                 std::string MemberId)
    : Expr(Expr::Kind::FieldAccessKind, std::move(Location)),
      Base(std::move(Base)), FieldId(std::move(MemberId)) {}

FieldAccessExpr::~FieldAccessExpr() = default;

// Visitor Methods
bool FieldAccessExpr::accept(NameResolver &R) { return R.visit(*this); }
InferRes FieldAccessExpr::accept(TypeInferencer &I) { return I.visit(*this); }
bool FieldAccessExpr::accept(TypeChecker &C) { return C.visit(*this); }
llvm::Value *FieldAccessExpr::accept(CodeGen &G) { return G.visit(*this); }

// Utility Methods
void FieldAccessExpr::emit(int Level) const {
  std::println("{}MemberAccessExpr:", indent(Level));
  std::println("{}  base:", indent(Level));
  Base->emit(Level + 2);
  std::println("{}  member: {}", indent(Level), FieldId);
}

//===----------------------------------------------------------------------===//
// MethodCallExpr Implementation
//===----------------------------------------------------------------------===//

// Constructors & Destructors
MethodCallExpr::MethodCallExpr(SrcLocation Location, std::unique_ptr<Expr> Base,
                               std::unique_ptr<Expr> Callee,
                               std::vector<std::unique_ptr<Expr>> Args)
    : FunCallExpr(Expr::Kind::MemberFunAccessKind, std::move(Location),
                  std::move(Callee), std::move(Args)),
      Base(std::move(Base)) {}

MethodCallExpr::~MethodCallExpr() = default;

// Visitor Methods
bool MethodCallExpr::accept(NameResolver &R) { return R.visit(*this); }
InferRes MethodCallExpr::accept(TypeInferencer &I) { return I.visit(*this); }
bool MethodCallExpr::accept(TypeChecker &C) { return C.visit(*this); }
llvm::Value *MethodCallExpr::accept(CodeGen &G) { return G.visit(*this); }

// Utility Methods
void MethodCallExpr::emit(int Level) const {
  std::println("{}MemberFunCallExpr:", indent(Level));
  std::println("{}  base:", indent(Level));
  Base->emit(Level + 2);
  std::println("{}  callee:", indent(Level));
  getCallee().emit(Level + 2);
  std::println("{}  args:", indent(Level));
  for (const auto &Arg : getArgs()) {
    Arg->emit(Level + 2);
  }
}

//===----------------------------------------------------------------------===//
// Additional TypeChecker accept implementations
// (keeps structure consistent; added for completeness)
//===----------------------------------------------------------------------===//

// Note: The per-node TypeChecker::accept implementations are duplicates of the
// pattern used above (returning C.visit(*this)). They are present here to
// explicitly override the base implementation where needed. If a node already
// had its TypeChecker accept method defined above, this block will not
// redefine it — the definitions above are the canonical ones.

} // namespace phi
