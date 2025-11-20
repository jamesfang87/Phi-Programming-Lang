#include "AST/Expr.hpp"

#include <cassert>
#include <memory>
#include <optional>
#include <print>
#include <string>
#include <utility>
#include <variant>

#include "llvm/IR/Value.h"

#include "AST/Decl.hpp"
#include "AST/Pattern.hpp"
#include "AST/Stmt.hpp"
#include "AST/Type.hpp"
#include "CodeGen/CodeGen.hpp"
#include "Lexer/TokenKind.hpp"
#include "Sema/NameResolver.hpp"
#include "Sema/TypeChecker.hpp"
#include "Sema/TypeInference/Infer.hpp"
#include "SrcManager/SrcLocation.hpp"

namespace {

//===----------------------------------------------------------------------===//
// Utility Functions
//===----------------------------------------------------------------------===//

/// Generates indentation string for AST dumping
std::string indent(int Level) { return std::string(Level * 2, ' '); }

/// Helper to emit a SingularPattern
void emitSingularPattern(const PatternAtomics::SingularPattern &SP,
                         int Level) {
  std::visit(
      [Level](const auto &P) {
        using T = std::decay_t<decltype(P)>;
        if constexpr (std::is_same_v<T, PatternAtomics::Wildcard>) {
          std::println("{}Wildcard", indent(Level));
        } else if constexpr (std::is_same_v<T, PatternAtomics::Literal>) {
          std::println("{}Literal:", indent(Level));
          P.Value->emit(Level + 1);
        } else if constexpr (std::is_same_v<T, PatternAtomics::Variant>) {
          std::println("{}Variant: {}", indent(Level), P.VariantName);
          if (!P.Vars.empty()) {
            std::println("{}Variables:", indent(Level + 1));
            for (const auto &Var : P.Vars) {
              std::println("{}{}", indent(Level + 2), Var);
            }
          }
        }
      },
      SP);
}

/// Helper to emit a Pattern
void emitPattern(const Pattern &Pat, int Level) {
  std::visit(
      [Level](const auto &P) {
        using T = std::decay_t<decltype(P)>;
        if constexpr (std::is_same_v<T, PatternAtomics::Wildcard>) {
          std::println("{}Wildcard", indent(Level));
        } else if constexpr (std::is_same_v<T, PatternAtomics::Literal>) {
          std::println("{}Literal:", indent(Level));
          P.Value->emit(Level + 1);
        } else if constexpr (std::is_same_v<T, PatternAtomics::Variant>) {
          std::println("{}Variant: {}", indent(Level), P.VariantName);
          if (!P.Vars.empty()) {
            std::println("{}Variables:", indent(Level + 1));
            for (const auto &Var : P.Vars) {
              std::println("{}{}", indent(Level + 2), Var);
            }
          }
        } else if constexpr (std::is_same_v<T, PatternAtomics::Alternation>) {
          std::println("{}Alternation:", indent(Level));
          for (const auto &SP : P.Patterns) {
            emitSingularPattern(SP, Level + 1);
          }
        }
      },
      Pat);
}

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
  std::println("{}Start:", indent(Level + 1));
  Start->emit(Level + 2);
  std::println("{}End:", indent(Level + 1));
  End->emit(Level + 2);
}

//===----------------------------------------------------------------------===//
// TupleLiteral Implementation
//===----------------------------------------------------------------------===//

// Constructors & Destructors
TupleLiteral::TupleLiteral(SrcLocation Location,
                           std::vector<std::unique_ptr<Expr>> Elements)
    : Expr(Expr::Kind::TupleLiteralKind, std::move(Location), std::nullopt),
      Elements(std::move(Elements)) {}

TupleLiteral::~TupleLiteral() = default;

// Visitor Methods
bool TupleLiteral::accept(NameResolver &R) { return R.visit(*this); }
InferRes TupleLiteral::accept(TypeInferencer &I) { return I.visit(*this); }
bool TupleLiteral::accept(TypeChecker &C) { return C.visit(*this); }
llvm::Value *TupleLiteral::accept(CodeGen &G) { return G.visit(*this); }

// Utility Methods
void TupleLiteral::emit(int Level) const {
  std::println("{}TupleLiteral:", indent(Level));
  std::println("{}Elements:", indent(Level + 1));
  for (auto &E : Elements) {
    E->emit(Level + 2);
  }
}

//===----------------------------------------------------------------------===//
// DeclRefExpr Implementation
//===----------------------------------------------------------------------===//

// Constructors & Destructors
DeclRefExpr::DeclRefExpr(SrcLocation Location, std::string Id)
    : Expr(Expr::Kind::DeclRefKind, std::move(Location)), Id(std::move(Id)) {}

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
    std::string TypeStr =
        DeclPtr->hasType() ? DeclPtr->getType().toString() : "<unresolved>";

    std::println("{}DeclRefExpr: {}; referring to: {} of type {}",
                 indent(Level), Id, DeclPtr->getId(), TypeStr);
  }
}

//===----------------------------------------------------------------------===//
// FunCallExpr Implementation
//===----------------------------------------------------------------------===//

// Constructors & Destructors
FunCallExpr::FunCallExpr(SrcLocation Location, std::unique_ptr<Expr> Callee,
                         std::vector<std::unique_ptr<Expr>> Args)
    : Expr(Expr::Kind::FunCallKind, std::move(Location)),
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
    std::println("{}Calling: {}", indent(Level + 1), getDecl()->getId());
  std::println("{}Callee:", indent(Level + 1));
  Callee->emit(Level + 2);
  std::println("{}Args:", indent(Level + 1));
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
  std::println("{}BinaryOp: {}", indent(Level), TokenKindToStr(Op));
  std::println("{}Lhs:", indent(Level + 1));
  Lhs->emit(Level + 2);
  std::println("{}Rhs:", indent(Level + 1));
  Rhs->emit(Level + 2);
  if (Ty.has_value()) {
    std::println("{}Type: {}", indent(Level + 1), Ty.value().toString());
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
  std::println("{}UnaryOp: {}", indent(Level), TokenKindToStr(Op));
  std::println("{}Expr:", indent(Level + 1));
  Operand->emit(Level + 2);
}

//===----------------------------------------------------------------------===//
// FieldInitExpr Implementation
//===----------------------------------------------------------------------===//

// Constructors & Destructors
MemberInitExpr::MemberInitExpr(SrcLocation Location, std::string FieldId,
                               std::unique_ptr<Expr> Init)
    : Expr(Expr::Kind::MemberInitKind, std::move(Location)),
      FieldId(std::move(FieldId)), InitValue(std::move(Init)) {}

MemberInitExpr::~MemberInitExpr() = default;

// Visitor Methods
bool MemberInitExpr::accept(NameResolver &R) { return R.visit(*this); }
InferRes MemberInitExpr::accept(TypeInferencer &I) { return I.visit(*this); }
bool MemberInitExpr::accept(TypeChecker &C) { return C.visit(*this); }
llvm::Value *MemberInitExpr::accept(CodeGen &G) { return G.visit(*this); }

// Utility Methods
void MemberInitExpr::emit(int Level) const {
  std::println("{}MemberInitExpr:", indent(Level));
  std::println("{}Member: {}", indent(Level + 1), FieldId);
  std::println("{}Value:", indent(Level + 1));
  InitValue->emit(Level + 2);
}

//===----------------------------------------------------------------------===//
// StructInitExpr Implementation
//===----------------------------------------------------------------------===//

CustomTypeCtor::CustomTypeCtor(
    SrcLocation Location, std::optional<std::string> TypeName,
    std::vector<std::unique_ptr<MemberInitExpr>> Inits)
    : Expr(Expr::Kind::CustomTypeCtorKind, std::move(Location)),
      TypeName(std::move(TypeName)), Inits(std::move(Inits)) {}

CustomTypeCtor::~CustomTypeCtor() = default;

// Visitor Methods
bool CustomTypeCtor::accept(NameResolver &R) { return R.visit(*this); }
InferRes CustomTypeCtor::accept(TypeInferencer &I) { return I.visit(*this); }
bool CustomTypeCtor::accept(TypeChecker &C) { return C.visit(*this); }
llvm::Value *CustomTypeCtor::accept(CodeGen &G) { return G.visit(*this); }

// Utility Methods
void CustomTypeCtor::emit(int Level) const {
  std::println("{}CustomTypeCtor:", indent(Level));
  std::println("{}For name: {}", indent(Level + 1),
               TypeName.value_or("No name"));
  std::println("{}Inits:", indent(Level + 1));
  for (const auto &Init : Inits) {
    Init->emit(Level + 2);
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
  std::println("{}FieldAccessExpr:", indent(Level));
  std::println("{}Base:", indent(Level + 1));
  Base->emit(Level + 2);
  std::println("{}Member: {}", indent(Level + 1), FieldId);
}

//===----------------------------------------------------------------------===//
// MethodCallExpr Implementation
//===----------------------------------------------------------------------===//

// Constructors & Destructors
MethodCallExpr::MethodCallExpr(SrcLocation Location, std::unique_ptr<Expr> Base,
                               std::unique_ptr<Expr> Callee,
                               std::vector<std::unique_ptr<Expr>> Args)
    : FunCallExpr(Expr::Kind::MethodCallKind, std::move(Location),
                  std::move(Callee), std::move(Args)),
      Base(std::move(Base)) {}

MethodCallExpr::MethodCallExpr(FunCallExpr &&Call,
                               std::unique_ptr<Expr> BaseExpr)
    : FunCallExpr(std::move(Call), Kind::MethodCallKind),
      Base(std::move(BaseExpr)) {}

MethodCallExpr::~MethodCallExpr() = default;

// Visitor Methods
bool MethodCallExpr::accept(NameResolver &R) { return R.visit(*this); }
InferRes MethodCallExpr::accept(TypeInferencer &I) { return I.visit(*this); }
bool MethodCallExpr::accept(TypeChecker &C) { return C.visit(*this); }
llvm::Value *MethodCallExpr::accept(CodeGen &G) { return G.visit(*this); }

// Utility Methods
void MethodCallExpr::emit(int Level) const {
  std::println("{}MethodCallExpr:", indent(Level));
  std::println("{}Base:", indent(Level + 1));
  Base->emit(Level + 2);
  std::println("{}Callee:", indent(Level + 1));
  getCallee().emit(Level + 2);
  std::println("{}Args:", indent(Level + 1));
  for (const auto &Arg : getArgs()) {
    Arg->emit(Level + 2);
  }
}

//===----------------------------------------------------------------------===//
// MatchExpr Implementation
//===----------------------------------------------------------------------===//

MatchExpr::MatchExpr(SrcLocation Location, std::unique_ptr<Expr> Scrutinee,
                     std::vector<Arm> Arms)
    : Expr(Expr::Kind::MatchExprKind, std::move(Location)),
      Scrutinee(std::move(Scrutinee)), Arms(std::move(Arms)) {}

MatchExpr::~MatchExpr() = default;

void MatchExpr::emit(int Level) const {
  std::println("{}MatchExpr:", indent(Level));
  std::println("{}Scrutinee: ", indent(Level));
  Scrutinee->emit(Level + 2);
  std::println("{}Cases: ", indent(Level + 1));
  for (const auto &Arm : Arms) {
    std::println("{}Pattern: ", indent(Level + 2));
    emitPattern(Arm.Pattern, Level + 3);

    std::println("{}Body: ", indent(Level + 2));
    Arm.Body->emit(Level + 3);

    std::println("{}Return: ", indent(Level + 2));
    Arm.Return->emit(Level + 3);
  }
}

bool MatchExpr::accept(NameResolver &R) { return R.visit(*this); }
InferRes MatchExpr::accept(TypeInferencer &I) { return I.visit(*this); }
bool MatchExpr::accept(TypeChecker &C) { return C.visit(*this); }
llvm::Value *MatchExpr::accept(CodeGen &G) { return G.visit(*this); }

} // namespace phi
