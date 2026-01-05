#include "AST/Nodes/Expr.hpp"

#include <cassert>
#include <memory>
#include <optional>
#include <print>
#include <string>
#include <utility>
#include <variant>
#include <vector>

#include "AST/Nodes/Decl.hpp"
#include "AST/Nodes/Stmt.hpp"
#include "AST/Pattern.hpp"
#include "AST/TypeSystem/Context.hpp"
#include "AST/TypeSystem/Type.hpp"
#include "Lexer/TokenKind.hpp"
#include "Sema/NameResolution/NameResolver.hpp"
#include "SrcManager/SrcLocation.hpp"

namespace {

//===----------------------------------------------------------------------===//
// Utility Functions
//===----------------------------------------------------------------------===//

/// Generates indentation string for AST dumping
std::string indent(int Level) { return std::string(Level * 2, ' '); }

/// Helper to emit a SingularPattern
void emitSingularPattern(const Pattern &Pat, int Level) {
  std::visit(
      [Level](const auto &Pat) {
        using T = std::decay_t<decltype(Pat)>;
        if constexpr (std::is_same_v<T, PatternAtomics::Wildcard>) {
          std::println("{}Wildcard", indent(Level));
        } else if constexpr (std::is_same_v<T, PatternAtomics::Literal>) {
          std::println("{}Literal:", indent(Level));
          Pat.Value->emit(Level + 1);
        } else if constexpr (std::is_same_v<T, PatternAtomics::Variant>) {
          std::println("{}Variant: {}", indent(Level), Pat.VariantName);
          if (!Pat.Vars.empty()) {
            std::println("{}Variables:", indent(Level + 1));
            for (const auto &Var : Pat.Vars) {
              std::println("{}{}", indent(Level + 2), Var->getId());
            }
          }
        }
      },
      Pat);
}

/// Helper to emit a Pattern
void emitPatterns(const std::vector<Pattern> &Patterns, int Level) {
  for (auto &Pattern : Patterns) {
    emitSingularPattern(Pattern, Level + 1);
  }
}

} // anonymous namespace

namespace phi {

//===----------------------------------------------------------------------===//
// Base Expr Implementation
//===----------------------------------------------------------------------===//

//===----------------------------------------------------------------------===//
// IntLiteral Implementation
//===----------------------------------------------------------------------===//

IntLiteral::IntLiteral(SrcLocation Location, const int64_t Value)
    : Expr(Expr::Kind::IntLiteralKind, Location,
           TypeCtx::getVar(VarTy::Domain::Int, SrcSpan(std::move(Location)))),
      Value(Value) {}

void IntLiteral::emit(int Level) const {
  std::string TypeStr = Type.toString();
  std::println("{}IntLiteral: {} (type: {})", indent(Level), Value, TypeStr);
}

//===----------------------------------------------------------------------===//
// FloatLiteral Implementation
//===----------------------------------------------------------------------===//

FloatLiteral::FloatLiteral(SrcLocation Location, const double Value)
    : Expr(Expr::Kind::FloatLiteralKind, Location,
           TypeCtx::getVar(VarTy::Domain::Float, SrcSpan(std::move(Location)))),
      Value(Value) {}

void FloatLiteral::emit(int Level) const {
  std::string TypeStr = Type.toString();
  std::println("{}FloatLiteral: {} (type: {})", indent(Level), Value, TypeStr);
}

//===----------------------------------------------------------------------===//
// StrLiteral Implementation
//===----------------------------------------------------------------------===//

StrLiteral::StrLiteral(SrcLocation Location, std::string Value)
    : Expr(
          Expr::Kind::StrLiteralKind, Location,
          TypeCtx::getBuiltin(BuiltinTy::String, SrcSpan(std::move(Location)))),
      Value(std::move(Value)) {}

void StrLiteral::emit(int Level) const {
  std::println("{}StrLiteral: {}", indent(Level), Value);
}

//===----------------------------------------------------------------------===//
// CharLiteral Implementation
//===----------------------------------------------------------------------===//

CharLiteral::CharLiteral(SrcLocation Location, char Value)
    : Expr(Expr::Kind::CharLiteralKind, Location,
           TypeCtx::getBuiltin(BuiltinTy::Char, SrcSpan(std::move(Location)))),
      Value(Value) {}

void CharLiteral::emit(int Level) const {
  std::println("{}CharLiteral: {}", indent(Level), Value);
}

//===----------------------------------------------------------------------===//
// BoolLiteral Implementation
//===----------------------------------------------------------------------===//

BoolLiteral::BoolLiteral(SrcLocation Location, bool Value)
    : Expr(Expr::Kind::BoolLiteralKind, Location,
           TypeCtx::getBuiltin(BuiltinTy::Bool, SrcSpan(std::move(Location)))),
      Value(Value) {}

void BoolLiteral::emit(int Level) const {
  std::println("{}BoolLiteral: {}", indent(Level), Value);
}

//===----------------------------------------------------------------------===//
// RangeLiteral Implementation
//===----------------------------------------------------------------------===//

RangeLiteral::RangeLiteral(SrcLocation Location, std::unique_ptr<Expr> Start,
                           std::unique_ptr<Expr> End, const bool Inclusive)
    : Expr(Expr::Kind::RangeLiteralKind, Location,
           TypeCtx::getBuiltin(BuiltinTy::Range, SrcSpan(std::move(Location)))),
      Start(std::move(Start)), End(std::move(End)), Inclusive(Inclusive) {}

RangeLiteral::~RangeLiteral() = default;

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

TupleLiteral::TupleLiteral(SrcLocation Location,
                           std::vector<std::unique_ptr<Expr>> Elements)
    : Expr(Expr::Kind::TupleLiteralKind, std::move(Location)),
      Elements(std::move(Elements)) {}

TupleLiteral::~TupleLiteral() = default;

void TupleLiteral::emit(int Level) const {
  std::println("{}TupleLiteral:", indent(Level));
  std::println("{}Type: {}", indent(Level + 1), Type.toString());
  std::println("{}Elements:", indent(Level + 1));
  for (auto &E : Elements) {
    E->emit(Level + 2);
  }
}

//===----------------------------------------------------------------------===//
// DeclRefExpr Implementation
//===----------------------------------------------------------------------===//

DeclRefExpr::DeclRefExpr(SrcLocation Location, std::string Id)
    : Expr(Expr::Kind::DeclRefKind, std::move(Location)), Id(std::move(Id)) {}

void DeclRefExpr::emit(int Level) const {
  if (Id == "val" && DeclPtr == nullptr) {
    std::println("This was not found");
  }
  if (DeclPtr == nullptr) {
    std::println("{}DeclRefExpr: {} ", indent(Level), Id);
    std::println("{}Type: {} ", indent(Level + 1), Type.toString());
  } else {
    std::string TypeStr = DeclPtr->getType().toString();
    std::println("{}DeclRefExpr: {}; referring to: {} of type {}",
                 indent(Level), Id, DeclPtr->getId(), TypeStr);
  }
}

//===----------------------------------------------------------------------===//
// FunCallExpr Implementation
//===----------------------------------------------------------------------===//

FunCallExpr::FunCallExpr(SrcLocation Location, std::unique_ptr<Expr> Callee,
                         std::vector<std::unique_ptr<Expr>> Args)
    : Expr(Expr::Kind::FunCallKind, std::move(Location)),
      Callee(std::move(Callee)), Args(std::move(Args)) {}

FunCallExpr::FunCallExpr(Kind K, SrcLocation Location,
                         std::unique_ptr<Expr> Callee,
                         std::vector<std::unique_ptr<Expr>> Args)
    : Expr(K, std::move(Location)), Callee(std::move(Callee)),
      Args(std::move(Args)) {}

FunCallExpr::~FunCallExpr() = default;

void FunCallExpr::emit(int Level) const {
  std::println("{}FunCallExpr", indent(Level));
  if (getDecl() != nullptr)
    std::println("{}Calling: {}", indent(Level + 1), getDecl()->getId());
  std::println("{}Type: {} ", indent(Level + 1), Type.toString());
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

BinaryOp::BinaryOp(std::unique_ptr<Expr> Lhs, std::unique_ptr<Expr> Rhs,
                   const Token &Op)
    : Expr(Expr::Kind::BinaryOpKind, Op.getStart()), Lhs(std::move(Lhs)),
      Rhs(std::move(Rhs)), Op(Op.getKind()) {}

BinaryOp::~BinaryOp() = default;

void BinaryOp::emit(int Level) const {
  std::println("{}BinaryOp: {}", indent(Level), Op.toString());
  std::println("{}Type: {} ", indent(Level + 1), Type.toString());
  std::println("{}Lhs:", indent(Level + 1));
  Lhs->emit(Level + 2);
  std::println("{}Rhs:", indent(Level + 1));
  Rhs->emit(Level + 2);
  if (hasType()) {
    std::println("{}Type: {}", indent(Level + 1), Type.toString());
  }
}

//===----------------------------------------------------------------------===//
// UnaryOp Implementation
//===----------------------------------------------------------------------===//

UnaryOp::UnaryOp(std::unique_ptr<Expr> Operand, const Token &Op,
                 const bool IsPrefix)
    : Expr(Expr::Kind::UnaryOpKind, Op.getStart()), Operand(std::move(Operand)),
      Op(Op.getKind()), IsPrefix(IsPrefix) {}

UnaryOp::~UnaryOp() = default;

void UnaryOp::emit(int Level) const {
  std::println("{}UnaryOp: {}", indent(Level), Op.toString());
  std::println("{}Type: {} ", indent(Level + 1), Type.toString());
  std::println("{}Expr:", indent(Level + 1));
  Operand->emit(Level + 2);
}

//===----------------------------------------------------------------------===//
// MemberInit Implementation
//===----------------------------------------------------------------------===//

MemberInit::MemberInit(SrcLocation Location, std::string FieldId,
                       std::unique_ptr<Expr> Init)
    : Expr(Expr::Kind::MemberInitKind, std::move(Location)),
      FieldId(std::move(FieldId)), InitValue(std::move(Init)) {}

MemberInit::~MemberInit() = default;

void MemberInit::emit(int Level) const {
  std::println("{}MemberInit:", indent(Level));
  std::println("{}Type: {} ", indent(Level + 1), Type.toString());
  std::println("{}Member: {}", indent(Level + 1), FieldId);
  if (InitValue) {
    std::println("{}Value:", indent(Level + 1));
    InitValue->emit(Level + 2);
  }
}

//===----------------------------------------------------------------------===//
// AdtInit Implementation
//===----------------------------------------------------------------------===//

AdtInit::AdtInit(SrcLocation Location, std::optional<std::string> TypeName,
                 std::vector<std::unique_ptr<MemberInit>> Inits)
    : Expr(Expr::Kind::CustomTypeCtorKind, Location,
           TypeName ? TypeCtx::getAdt(*TypeName, nullptr, SrcSpan(Location))
                    : TypeCtx::getVar(VarTy::Domain::Adt,
                                      SrcSpan(std::move(Location)))),
      TypeName(std::move(TypeName)), Inits(std::move(Inits)) {}

AdtInit::~AdtInit() = default;

void AdtInit::emit(int Level) const {
  std::println("{}AdtInit:", indent(Level));
  std::println("{}For name: {}", indent(Level + 1),
               TypeName.value_or("No name"));
  if (Decl)
    std::println("{}Referring to: {}", indent(Level + 1), Decl->getId());
  std::println("{}Type: {} ", indent(Level + 1), Type.toString());
  std::println("{}Inits:", indent(Level + 1));
  for (const auto &Init : Inits) {
    Init->emit(Level + 2);
  }
}

//===----------------------------------------------------------------------===//
// FieldAccessExpr Implementation
//===----------------------------------------------------------------------===//

FieldAccessExpr::FieldAccessExpr(SrcLocation Location,
                                 std::unique_ptr<Expr> Base,
                                 std::string MemberId)
    : Expr(Expr::Kind::FieldAccessKind, std::move(Location)),
      Base(std::move(Base)), FieldId(std::move(MemberId)) {}

FieldAccessExpr::~FieldAccessExpr() = default;

void FieldAccessExpr::emit(int Level) const {
  std::println("{}FieldAccessExpr:", indent(Level));
  std::println("{}Type: {} ", indent(Level + 1), Type.toString());
  std::println("{}Base:", indent(Level + 1));
  Base->emit(Level + 2);
  std::println("{}Member: {}", indent(Level + 1), FieldId);
}

//===----------------------------------------------------------------------===//
// MethodCallExpr Implementation
//===----------------------------------------------------------------------===//

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

void MethodCallExpr::emit(int Level) const {
  std::println("{}MethodCallExpr:", indent(Level));
  std::println("{}Type: {} ", indent(Level + 1), Type.toString());
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
  std::println("{}Type: {} ", indent(Level + 1), Type.toString());
  std::println("{}Scrutinee: ", indent(Level + 1));
  Scrutinee->emit(Level + 2);
  std::println("{}Cases: ", indent(Level + 1));
  for (const auto &Arm : Arms) {
    std::println("{}Pattern: ", indent(Level + 2));
    emitPatterns(Arm.Patterns, Level + 3);

    std::println("{}Body: ", indent(Level + 2));
    Arm.Body->emit(Level + 3);

    std::println("{}Return: ", indent(Level + 2));
    Arm.Return->emit(Level + 3);
  }
}

} // namespace phi
