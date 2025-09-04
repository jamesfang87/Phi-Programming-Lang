#include "AST/Expr.hpp"

#include <cassert>
#include <optional>
#include <print>
#include <string>
#include <utility>

#include "AST/Decl.hpp"
#include "AST/Type.hpp"
#include "CodeGen/CodeGen.hpp"
#include "Lexer/TokenKind.hpp"
#include "Sema/NameResolver.hpp"
#include "Sema/TypeInference/Infer.hpp"
#include "llvm/IR/Value.h"

namespace {
/// Generates indentation string for AST dumping
std::string indent(int Level) { return std::string(Level * 2, ' '); }
} // namespace

namespace phi {

//======================== Destructor Implementations ========================//
RangeLiteral::~RangeLiteral() = default;
FunCallExpr::~FunCallExpr() = default;
BinaryOp::~BinaryOp() = default;
UnaryOp::~UnaryOp() = default;
FieldInitExpr::~FieldInitExpr() = default;
StructInitExpr::~StructInitExpr() = default;
MemberAccessExpr::~MemberAccessExpr() = default;
MemberFunCallExpr::~MemberFunCallExpr() = default;

//======================== Base Expr Implementation ========================//
bool Expr::accept(NameResolver &R) { return R.visit(*this); }
InferRes Expr::accept(TypeInferencer &I) { return I.visit(*this); }
llvm::Value *Expr::accept(CodeGen &G) { return G.visit(*this); }

//======================== IntLiteral Implementation ========================//
IntLiteral::IntLiteral(SrcLocation Location, const int64_t Value)
    : Expr(Expr::Kind::IntLiteralKind, std::move(Location), std::nullopt),
      Value(Value) {}

void IntLiteral::emit(int Level) const {
  std::string typeStr = Ty.has_value() ? Ty.value().toString() : "<unresolved>";
  std::println("{}IntLiteral: {} (type: {})", indent(Level), Value, typeStr);
}

bool IntLiteral::accept(NameResolver &R) { return R.visit(*this); }
InferRes IntLiteral::accept(TypeInferencer &I) { return I.visit(*this); }
llvm::Value *IntLiteral::accept(CodeGen &G) { return G.visit(*this); }

//======================== FloatLiteral Implementation
//========================//
FloatLiteral::FloatLiteral(SrcLocation Location, const double Value)
    : Expr(Expr::Kind::FloatLiteralKind, std::move(Location), std::nullopt),
      Value(Value) {}

void FloatLiteral::emit(int Level) const {
  std::string typeStr = Ty.has_value() ? Ty.value().toString() : "<unresolved>";
  std::println("{}FloatLiteral: {} (type: {})", indent(Level), Value, typeStr);
}

bool FloatLiteral::accept(NameResolver &R) { return R.visit(*this); }
InferRes FloatLiteral::accept(TypeInferencer &I) { return I.visit(*this); }
llvm::Value *FloatLiteral::accept(CodeGen &G) { return G.visit(*this); }

//======================== StrLiteral Implementation ========================//
StrLiteral::StrLiteral(SrcLocation Location, std::string Value)
    : Expr(Expr::Kind::StrLiteralKind, Location,
           Type::makePrimitive(PrimitiveKind::String, std::move(Location))),
      Value(std::move(Value)) {}

void StrLiteral::emit(int Level) const {
  std::println("{}StrLiteral: {}", indent(Level), Value);
}

bool StrLiteral::accept(NameResolver &R) { return R.visit(*this); }
InferRes StrLiteral::accept(TypeInferencer &I) { return I.visit(*this); }
llvm::Value *StrLiteral::accept(CodeGen &G) { return G.visit(*this); }

//======================== CharLiteral Implementation ========================//
CharLiteral::CharLiteral(SrcLocation Location, char Value)
    : Expr(Expr::Kind::CharLiteralKind, Location,
           Type::makePrimitive(PrimitiveKind::Char, std::move(Location))),
      Value(Value) {}

void CharLiteral::emit(int Level) const {
  std::println("{}CharLiteral: {}", indent(Level), Value);
}

bool CharLiteral::accept(NameResolver &R) { return R.visit(*this); }
InferRes CharLiteral::accept(TypeInferencer &I) { return I.visit(*this); }
llvm::Value *CharLiteral::accept(CodeGen &G) { return G.visit(*this); }

//======================== BoolLiteral Implementation ========================//
BoolLiteral::BoolLiteral(SrcLocation Location, bool Value)
    : Expr(Expr::Kind::BoolLiteralKind, Location,
           Type::makePrimitive(PrimitiveKind::Bool, std::move(Location))),
      Value(Value) {}

void BoolLiteral::emit(int Level) const {
  std::println("{}BoolLiteral: {}", indent(Level), Value);
}

bool BoolLiteral::accept(NameResolver &R) { return R.visit(*this); }
InferRes BoolLiteral::accept(TypeInferencer &I) { return I.visit(*this); }
llvm::Value *BoolLiteral::accept(CodeGen &G) { return G.visit(*this); }

//======================== RangeLiteral Implementation
//========================//
RangeLiteral::RangeLiteral(SrcLocation Location, std::unique_ptr<Expr> Start,
                           std::unique_ptr<Expr> End, const bool Inclusive)
    : Expr(Expr::Kind::RangeLiteralKind, Location,
           Type::makePrimitive(PrimitiveKind::Range, std::move(Location))),
      Start(std::move(Start)), End(std::move(End)), Inclusive(Inclusive) {}

void RangeLiteral::emit(int Level) const {
  std::println("{}RangeLiteral:", indent(Level));
  std::println("{}  start:", indent(Level));
  Start->emit(Level + 2);
  std::println("{}  end:", indent(Level));
  End->emit(Level + 2);
}

bool RangeLiteral::accept(NameResolver &R) { return R.visit(*this); }
InferRes RangeLiteral::accept(TypeInferencer &I) { return I.visit(*this); }
llvm::Value *RangeLiteral::accept(CodeGen &G) { return G.visit(*this); }

//======================== DeclRefExpr Implementation ========================//
DeclRefExpr::DeclRefExpr(SrcLocation Location, std::string Id)
    : Expr(Expr::Kind::DeclRefExprKind, std::move(Location)),
      Id(std::move(Id)) {}

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

bool DeclRefExpr::accept(NameResolver &R) { return R.visit(*this); }
InferRes DeclRefExpr::accept(TypeInferencer &I) { return I.visit(*this); }
llvm::Value *DeclRefExpr::accept(CodeGen &G) { return G.visit(*this); }

//======================== FunCallExpr Implementation ========================//
FunCallExpr::FunCallExpr(SrcLocation Location, std::unique_ptr<Expr> Callee,
                         std::vector<std::unique_ptr<Expr>> Args)
    : Expr(Expr::Kind::FunCallExprKind, std::move(Location)),
      Callee(std::move(Callee)), Args(std::move(Args)) {}

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

bool FunCallExpr::accept(NameResolver &R) { return R.visit(*this); }
InferRes FunCallExpr::accept(TypeInferencer &I) { return I.visit(*this); }
llvm::Value *FunCallExpr::accept(CodeGen &G) { return G.visit(*this); }

//======================== BinaryOp Implementation ========================//
BinaryOp::BinaryOp(std::unique_ptr<Expr> Lhs, std::unique_ptr<Expr> Rhs,
                   const Token &Op)
    : Expr(Expr::Kind::BinaryOpKind, Op.getStart()), Lhs(std::move(Lhs)),
      Rhs(std::move(Rhs)), Op(Op.getKind()) {}

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

bool BinaryOp::accept(NameResolver &R) { return R.visit(*this); }
InferRes BinaryOp::accept(TypeInferencer &I) { return I.visit(*this); }
llvm::Value *BinaryOp::accept(CodeGen &G) { return G.visit(*this); }

//======================== UnaryOp Implementation ========================//
UnaryOp::UnaryOp(std::unique_ptr<Expr> Operand, const Token &Op,
                 const bool IsPrefix)
    : Expr(Expr::Kind::UnaryOpKind, Op.getStart()), Operand(std::move(Operand)),
      Op(Op.getKind()), IsPrefix(IsPrefix) {}

void UnaryOp::emit(int Level) const {
  std::println("{}UnaryOp: {}", indent(Level), tyToStr(Op));
  std::println("{}  expr:", indent(Level));
  Operand->emit(Level + 2);
}

bool UnaryOp::accept(NameResolver &R) { return R.visit(*this); }
InferRes UnaryOp::accept(TypeInferencer &I) { return I.visit(*this); }
llvm::Value *UnaryOp::accept(CodeGen &G) { return G.visit(*this); }

//====================== FieldInitExpr Implementation //======================//
FieldInitExpr::FieldInitExpr(SrcLocation Location, std::string FieldId,
                             std::unique_ptr<Expr> Init)
    : Expr(Expr::Kind::FieldInitKind, std::move(Location)),
      FieldId(std::move(FieldId)), Init(std::move(Init)) {}

void FieldInitExpr::emit(int Level) const {
  std::println("{}FieldInitExpr:", indent(Level));
  std::println("{}  field: {}", indent(Level), FieldId);
  std::println("{}  value:", indent(Level));
  Init->emit(Level + 2);
}

bool FieldInitExpr::accept(NameResolver &R) { return R.visit(*this); }
InferRes FieldInitExpr::accept(TypeInferencer &I) { return I.visit(*this); }
llvm::Value *FieldInitExpr::accept(CodeGen &G) { return G.visit(*this); }

//===================== StructInitExpr Implementation //=====================//
StructInitExpr::StructInitExpr(
    SrcLocation Location, std::string StructId,
    std::vector<std::unique_ptr<FieldInitExpr>> Fields)
    : Expr(Expr::Kind::StructInitKind, std::move(Location)),
      StructId(std::move(StructId)), FieldInits(std::move(Fields)) {}

void StructInitExpr::emit(int Level) const {
  std::println("{}StructInitExpr:", indent(Level));
  std::println("{}  struct: {}", indent(Level), StructId);
  std::println("{}  fields:", indent(Level));
  for (const auto &Field : FieldInits) {
    Field->emit(Level + 2);
  }
}

bool StructInitExpr::accept(NameResolver &R) { return R.visit(*this); }
InferRes StructInitExpr::accept(TypeInferencer &I) { return I.visit(*this); }
llvm::Value *StructInitExpr::accept(CodeGen &G) { return G.visit(*this); }

//======================== MemberAccessExpr Implementation
//========================//
MemberAccessExpr::MemberAccessExpr(SrcLocation Location,
                                   std::unique_ptr<Expr> Base,
                                   std::string MemberId)
    : Expr(Expr::Kind::MemberAccessKind, std::move(Location)),
      Base(std::move(Base)), MemberId(std::move(MemberId)) {}

void MemberAccessExpr::emit(int Level) const {
  std::println("{}MemberAccessExpr:", indent(Level));
  std::println("{}  base:", indent(Level));
  Base->emit(Level + 2);
  std::println("{}  member: {}", indent(Level), MemberId);
}

bool MemberAccessExpr::accept(NameResolver &R) { return R.visit(*this); }
InferRes MemberAccessExpr::accept(TypeInferencer &I) { return I.visit(*this); }
llvm::Value *MemberAccessExpr::accept(CodeGen &G) { return G.visit(*this); }

//=================== MemberFunCallExpr Implementation //=====================//
MemberFunCallExpr::MemberFunCallExpr(SrcLocation Location,
                                     std::unique_ptr<Expr> Base,
                                     std::unique_ptr<FunCallExpr> FunCall)
    : Expr(Expr::Kind::MemberFunAccessKind, std::move(Location)),
      Base(std::move(Base)), FunCall(std::move(FunCall)) {}

void MemberFunCallExpr::emit(int Level) const {
  std::println("{}MemberFunCallExpr:", indent(Level));
  std::println("{}  base:", indent(Level));
  Base->emit(Level + 2);
  std::println("{}  call:", indent(Level));
  FunCall->emit(Level + 2);
}

bool MemberFunCallExpr::accept(NameResolver &R) { return R.visit(*this); }
InferRes MemberFunCallExpr::accept(TypeInferencer &I) { return I.visit(*this); }
llvm::Value *MemberFunCallExpr::accept(CodeGen &G) { return G.visit(*this); }

} // namespace phi
