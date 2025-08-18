#include "AST/Expr.hpp"

#include <cassert>
#include <print>
#include <string>

#include "AST/ASTVisitor.hpp"

#include "AST/Decl.hpp"
#include "AST/Stmt.hpp"
#include "Lexer/TokenKind.hpp"
#include "Sema/HMTI/Infer.hpp"
#include "Sema/NameResolver.hpp"

namespace {
/// Generates indentation string for AST dumping
/// @param Level Current indentation Level
/// @return String of spaces for indentation
std::string indent(int Level) { return std::string(Level * 2, ' '); }
} // namespace

//======================== IntLiteral Implementation ========================//

namespace phi {
// Destructor implementations
RangeLiteral::~RangeLiteral() = default;
FunCallExpr::~FunCallExpr() = default;
BinaryOp::~BinaryOp() = default;
UnaryOp::~UnaryOp() = default;
FieldInitExpr::~FieldInitExpr() = default;
StructInitExpr::~StructInitExpr() = default;
MemberAccessExpr::~MemberAccessExpr() = default;
MemberFunCallExpr::~MemberFunCallExpr() = default;

// Accept method implementations
bool Expr::accept(ASTVisitor<bool> &Visitor) { return Visitor.visit(*this); }

void Expr::accept(ASTVisitor<void> &Visitor) { Visitor.visit(*this); }
bool Expr::accept(NameResolver &R) { return R.visit(*this); }
InferRes Expr::accept(TypeInferencer &I) { return I.visit(*this); }
/**
 * @brief Constructs an integer literal expression
 *
 * Default type is set to i64. The actual type may be
 * adjusted during semantic analysis based on value.
 *
 * @param Location Source Location of literal
 * @param value Integer value
 */
IntLiteral::IntLiteral(SrcLocation Location, const int64_t Value)
    : Expr(Stmt::Kind::IntLiteralKind, std::move(Location),
           Type(Type::PrimitiveKind::I64Kind)),
      Value(Value) {}

/**
 * @brief Dumps integer literal information
 *
 * Output format:
 *   [indent]IntLiteral: value
 *
 * @param Level Current indentation Level
 */
void IntLiteral::emit(int Level) const {
  std::string typeStr = Ty.has_value() ? Ty.value().toString() : "<unresolved>";
  std::println("{}IntLiteral: {} (type: {})", indent(Level), Value, typeStr);
}

bool IntLiteral::accept(ASTVisitor<bool> &Visitor) {
  return Visitor.visit(*this);
}
bool IntLiteral::accept(NameResolver &R) { return R.visit(*this); }
InferRes IntLiteral::accept(TypeInferencer &I) { return I.visit(*this); }
void IntLiteral::accept(ASTVisitor<void> &Visitor) { Visitor.visit(*this); }

// ====================== FloatLiteral Implementation ========================//

/**
 * @brief Constructs a floating-point literal expression
 *
 * Default type is set to f64. The actual type may be
 * adjusted during semantic analysis based on value.
 *
 * @param Location Source Location of literal
 * @param value Floating-point value
 */
FloatLiteral::FloatLiteral(SrcLocation Location, const double Value)
    : Expr(Stmt::Kind::FloatLiteralKind, std::move(Location),
           Type(Type::PrimitiveKind::F64Kind)),
      Value(Value) {}

/**
 * @brief Dumps float literal information
 *
 * Output format:
 *   [indent]FloatLiteral: value
 *
 * @param Level Current indentation Level
 */
void FloatLiteral::emit(int Level) const {
  std::string typeStr = Ty.has_value() ? Ty.value().toString() : "<unresolved>";
  std::println("{}FloatLiteral: {} (type: {})", indent(Level), Value, typeStr);
}

bool FloatLiteral::accept(ASTVisitor<bool> &Visitor) {
  return Visitor.visit(*this);
}

bool FloatLiteral::accept(NameResolver &R) { return R.visit(*this); }
InferRes FloatLiteral::accept(TypeInferencer &I) { return I.visit(*this); }
void FloatLiteral::accept(ASTVisitor<void> &Visitor) { Visitor.visit(*this); }

// ====================== StrLiteral Implementation ========================//

/**
 * @brief Constructs a string literal expression
 *
 * Type is always set to str primitive type.
 *
 * @param Location Source Location of literal
 * @param value String content
 */
StrLiteral::StrLiteral(SrcLocation Location, std::string Value)
    : Expr(Stmt::Kind::StrLiteralKind, std::move(Location),
           Type(Type::PrimitiveKind::StringKind)),
      Value(std::move(Value)) {}

/**
 * @brief Dumps string literal information
 *
 * Output format:
 *   [indent]StrLiteral: value
 *
 * @param Level Current indentation Level
 */
void StrLiteral::emit(int Level) const {
  std::println("{}StrLiteral: {}", indent(Level), Value);
}

bool StrLiteral::accept(NameResolver &R) { return R.visit(*this); }
InferRes StrLiteral::accept(TypeInferencer &I) { return I.visit(*this); }

bool StrLiteral::accept(ASTVisitor<bool> &Visitor) {
  return Visitor.visit(*this);
}

void StrLiteral::accept(ASTVisitor<void> &Visitor) { Visitor.visit(*this); }

// ====================== CharLiteral Implementation ========================//

/**
 * @brief Constructs a character literal expression
 *
 * Type is always set to char primitive type.
 *
 * @param Location Source Location of literal
 * @param value Character value
 */
CharLiteral::CharLiteral(SrcLocation Location, char Value)
    : Expr(Stmt::Kind::CharLiteralKind, std::move(Location),
           Type(Type::PrimitiveKind::CharKind)),
      Value(Value) {}

/**
 * @brief Dumps character literal information
 *
 * Output format:
 *   [indent]CharLiteral: value
 *
 * @param Level Current indentation Level
 */
void CharLiteral::emit(int Level) const {
  std::println("{}CharLiteral: {}", indent(Level), Value);
}

bool CharLiteral::accept(ASTVisitor<bool> &Visitor) {
  return Visitor.visit(*this);
}

bool CharLiteral::accept(NameResolver &R) { return R.visit(*this); }
InferRes CharLiteral::accept(TypeInferencer &I) { return I.visit(*this); }

void CharLiteral::accept(ASTVisitor<void> &Visitor) { Visitor.visit(*this); }

// ====================== BoolLiteral Implementation ========================//

/**
 * @brief Constructs a boolean literal expression
 *
 * Type is always set to bool primitive type.
 *
 * @param Location Source Location of literal
 * @param value Boolean value
 */
BoolLiteral::BoolLiteral(SrcLocation Location, bool Value)
    : Expr(Stmt::Kind::BoolLiteralKind, std::move(Location),
           Type(Type::PrimitiveKind::BoolKind)),
      Value(Value) {}

/**
 * @brief Dumps boolean literal information
 *
 * Output format:
 *   [indent]BoolLiteral: value
 *
 * @param Level Current indentation Level
 */
void BoolLiteral::emit(int Level) const {
  std::println("{}BoolLiteral: {}", indent(Level), Value);
}

bool BoolLiteral::accept(ASTVisitor<bool> &Visitor) {
  return Visitor.visit(*this);
}

bool BoolLiteral::accept(NameResolver &R) { return R.visit(*this); }
InferRes BoolLiteral::accept(TypeInferencer &I) { return I.visit(*this); }

void BoolLiteral::accept(ASTVisitor<void> &Visitor) { Visitor.visit(*this); }

// ====================== RangeLiteral Implementation ========================//

/**
 * @brief Constructs a range literal expression
 *
 * Represents inclusive (..=) or exclusive (..) ranges.
 * Type is set during semantic analysis.
 *
 * @param Location Source Location of range
 * @param start Start expression
 * @param end End expression
 * @param inclusive True for inclusive range, false for exclusive
 */
RangeLiteral::RangeLiteral(SrcLocation Location, std::unique_ptr<Expr> Start,
                           std::unique_ptr<Expr> End, const bool Inclusive)
    : Expr(Stmt::Kind::RangeLiteralKind, std::move(Location),
           Type(Type::PrimitiveKind::RangeKind)),
      Start(std::move(Start)), End(std::move(End)), Inclusive(Inclusive) {}

/**
 * @brief Dumps range literal information
 *
 * Output format:
 *   [indent]RangeLiteral:
 *     [indent]  start:
 *       [child expression dump]
 *     [indent]  end:
 *       [child expression dump]
 *
 * @param Level Current indentation Level
 */
void RangeLiteral::emit(int Level) const {
  std::println("{}RangeLiteral:", indent(Level));
  std::println("{}  start:", indent(Level));
  Start->emit(Level + 2);
  std::println("{}  end:", indent(Level));
  End->emit(Level + 2);
}

bool RangeLiteral::accept(ASTVisitor<bool> &Visitor) {
  return Visitor.visit(*this);
}

bool RangeLiteral::accept(NameResolver &R) { return R.visit(*this); }
InferRes RangeLiteral::accept(TypeInferencer &I) { return I.visit(*this); }

void RangeLiteral::accept(ASTVisitor<void> &Visitor) { Visitor.visit(*this); }

// ====================== DeclRefExpr Implementation ========================//

/**
 * @brief Constructs a declaration reference expression
 *
 * Represents references to variables, functions, etc.
 * The actual declaration is resolved during semantic analysis.
 *
 * @param Location Source Location of reference
 * @param identifier Declaration name
 */
DeclRefExpr::DeclRefExpr(SrcLocation Location, std::string Id)
    : Expr(Stmt::Kind::DeclRefExprKind, std::move(Location)),
      Id(std::move(Id)) {}

/**
 * @brief Dumps declaration reference information
 *
 * Output format:
 *   [indent]DeclRefExpr: identifier
 *
 * @param Level Current indentation Level
 */
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

bool DeclRefExpr::accept(ASTVisitor<bool> &Visitor) {
  return Visitor.visit(*this);
}

bool DeclRefExpr::accept(NameResolver &R) { return R.visit(*this); }
InferRes DeclRefExpr::accept(TypeInferencer &I) { return I.visit(*this); }

void DeclRefExpr::accept(ASTVisitor<void> &Visitor) { Visitor.visit(*this); }

//====================== FunCallExpr Implementation ========================//

/**
 * @brief Constructs a function call expression
 *
 * @param Location Source Location of call
 * @param callee Expression being called (usually DeclRefExpr)
 * @param args Argument expressions
 */
FunCallExpr::FunCallExpr(SrcLocation Location, std::unique_ptr<Expr> Callee,
                         std::vector<std::unique_ptr<Expr>> Args)
    : Expr(Stmt::Kind::FunCallExprKind, std::move(Location)),
      Callee(std::move(Callee)), Args(std::move(Args)) {}

/**
 * @brief Dumps function call information
 *
 * Output format:
 *   [indent]FunCallExpr
 *     [indent]  callee:
 *       [child expression dump]
 *     [indent]  args:
 *       [argument dumps]
 *
 * @param Level Current indentation Level
 */
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

bool FunCallExpr::accept(ASTVisitor<bool> &Visitor) {
  return Visitor.visit(*this);
}

bool FunCallExpr::accept(NameResolver &R) { return R.visit(*this); }
InferRes FunCallExpr::accept(TypeInferencer &I) { return I.visit(*this); }

void FunCallExpr::accept(ASTVisitor<void> &Visitor) { Visitor.visit(*this); }

//====================== BinaryOp Implementation ========================//

/**
 * @brief Constructs a binary operation expression
 *
 * @param lhs Left-hand operand
 * @param rhs Right-hand operand
 * @param op Operator token
 */
BinaryOp::BinaryOp(std::unique_ptr<Expr> Lhs, std::unique_ptr<Expr> Rhs,
                   const Token &Op)
    : Expr(Stmt::Kind::BinaryOpKind, Op.getStart()), Lhs(std::move(Lhs)),
      Rhs(std::move(Rhs)), Op(Op.getKind()) {}

/**
 * @brief Dumps binary operation information
 *
 * Output format:
 *   [indent]BinaryOp: operator
 *     [indent]  lhs:
 *       [child expression dump]
 *     [indent]  rhs:
 *       [child expression dump]
 *   [indent]  type: resolved_type (if available)
 *
 * @param Level Current indentation Level
 */
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

bool BinaryOp::accept(ASTVisitor<bool> &Visitor) {
  return Visitor.visit(*this);
}

bool BinaryOp::accept(NameResolver &R) { return R.visit(*this); }
InferRes BinaryOp::accept(TypeInferencer &I) { return I.visit(*this); }

void BinaryOp::accept(ASTVisitor<void> &Visitor) { Visitor.visit(*this); }

//====================== UnaryOp Implementation ========================//

/**
 * @brief Constructs a unary operation expression
 *
 * @param operand Target expression
 * @param op Operator token
 * @param is_prefix True if prefix operator, false if postfix
 */
UnaryOp::UnaryOp(std::unique_ptr<Expr> Operand, const Token &Op,
                 const bool IsPrefix)
    : Expr(Stmt::Kind::UnaryOpKind, Op.getStart()), Operand(std::move(Operand)),
      Op(Op.getKind()), IsPrefix(IsPrefix) {}

/**
 * @brief Dumps unary operation information
 *
 * Output format:
 *   [indent]UnaryOp: operator
 *     [indent]  expr:
 *       [child expression dump]
 *
 * @param Level Current indentation Level
 */
void UnaryOp::emit(int Level) const {
  std::println("{}UnaryOp: {}", indent(Level), tyToStr(Op));
  std::println("{}  expr:", indent(Level));
  Operand->emit(Level + 2);
}

bool UnaryOp::accept(ASTVisitor<bool> &Visitor) { return Visitor.visit(*this); }

bool UnaryOp::accept(NameResolver &R) { return R.visit(*this); }
InferRes UnaryOp::accept(TypeInferencer &I) { return I.visit(*this); }
void UnaryOp::accept(ASTVisitor<void> &Visitor) { Visitor.visit(*this); }

FieldInitExpr::FieldInitExpr(SrcLocation Location, std::string FieldId,
                             std::unique_ptr<Expr> Init)
    : Expr(Stmt::Kind::FieldInitKind, std::move(Location)),
      FieldId(std::move(FieldId)), Init(std::move(Init)) {}

void FieldInitExpr::emit(int Level) const {
  std::println("{}FieldInitExpr:", indent(Level));
  std::println("{}  field: {}", indent(Level), FieldId);
  std::println("{}  value:", indent(Level));
  Init->emit(Level + 2);
}

bool FieldInitExpr::accept(ASTVisitor<bool> &Visitor) {
  return Visitor.visit(*this);
}

bool FieldInitExpr::accept(NameResolver &R) { return R.visit(*this); }
InferRes FieldInitExpr::accept(TypeInferencer &I) { return I.visit(*this); }

void FieldInitExpr::accept(ASTVisitor<void> &Visitor) { Visitor.visit(*this); }

StructInitExpr::StructInitExpr(
    SrcLocation Location, std::string StructId,
    std::vector<std::unique_ptr<FieldInitExpr>> Fields)
    : Expr(Stmt::Kind::StructInitKind, std::move(Location)),
      StructId(std::move(StructId)), Fields(std::move(Fields)) {}

void StructInitExpr::emit(int Level) const {
  std::println("{}StructInitExpr:", indent(Level));
  std::println("{}  struct: {}", indent(Level), StructId);
  std::println("{}  fields:", indent(Level));
  for (const auto &Field : Fields) {
    Field->emit(Level + 2);
  }
}

bool StructInitExpr::accept(ASTVisitor<bool> &Visitor) {
  return Visitor.visit(*this);
}

bool StructInitExpr::accept(NameResolver &R) { return R.visit(*this); }
InferRes StructInitExpr::accept(TypeInferencer &I) { return I.visit(*this); }

void StructInitExpr::accept(ASTVisitor<void> &Visitor) { Visitor.visit(*this); }

//====================== MemberAccessExpr Implementation
//========================//

/**
 * @brief Constructs a member access expression
 *
 * Represents accessing a field or member of a struct/object (e.g., obj.field)
 *
 * @param Location Source Location of the access
 * @param Base The base expression being accessed
 * @param MemberId The name of the member being accessed
 */
MemberAccessExpr::MemberAccessExpr(SrcLocation Location,
                                   std::unique_ptr<Expr> Base,
                                   std::string MemberId)
    : Expr(Stmt::Kind::MemberAccessKind, std::move(Location)),
      Base(std::move(Base)), MemberId(std::move(MemberId)) {}

/**
 * @brief Dumps member access expression information
 *
 * Output format:
 *   [indent]MemberAccessExpr:
 *     [indent]  base:
 *       [child expression dump]
 *     [indent]  member: member_name
 *
 * @param Level Current indentation Level
 */
void MemberAccessExpr::emit(int Level) const {
  std::println("{}MemberAccessExpr:", indent(Level));
  std::println("{}  base:", indent(Level));
  Base->emit(Level + 2);
  std::println("{}  member: {}", indent(Level), MemberId);
}

bool MemberAccessExpr::accept(ASTVisitor<bool> &Visitor) {
  return Visitor.visit(*this);
}

bool MemberAccessExpr::accept(NameResolver &R) { return R.visit(*this); }
InferRes MemberAccessExpr::accept(TypeInferencer &I) { return I.visit(*this); }

void MemberAccessExpr::accept(ASTVisitor<void> &Visitor) {
  Visitor.visit(*this);
}

//====================== MemberFunCallExpr Implementation
//========================//

/**
 * @brief Constructs a member function call expression
 *
 * Represents calling a method on an object (e.g., obj.method(args))
 *
 * @param Location Source Location of the call
 * @param Base The base expression on which the method is called
 * @param FunCall The function call expression for the method
 */
MemberFunCallExpr::MemberFunCallExpr(SrcLocation Location,
                                     std::unique_ptr<Expr> Base,
                                     std::unique_ptr<FunCallExpr> FunCall)
    : Expr(Stmt::Kind::MemberFunAccessKind, std::move(Location)),
      Base(std::move(Base)), FunCall(std::move(FunCall)) {}

/**
 * @brief Dumps member function call expression information
 *
 * Output format:
 *   [indent]MemberFunCallExpr:
 *     [indent]  base:
 *       [child expression dump]
 *     [indent]  call:
 *       [function call dump]
 *
 * @param Level Current indentation Level
 */
void MemberFunCallExpr::emit(int Level) const {
  std::println("{}MemberFunCallExpr:", indent(Level));
  std::println("{}  base:", indent(Level));
  Base->emit(Level + 2);
  std::println("{}  call:", indent(Level));
  FunCall->emit(Level + 2);
}

bool MemberFunCallExpr::accept(ASTVisitor<bool> &Visitor) {
  return Visitor.visit(*this);
}

bool MemberFunCallExpr::accept(NameResolver &R) { return R.visit(*this); }
InferRes MemberFunCallExpr::accept(TypeInferencer &I) { return I.visit(*this); }
void MemberFunCallExpr::accept(ASTVisitor<void> &Visitor) {
  Visitor.visit(*this);
}

} // namespace phi
