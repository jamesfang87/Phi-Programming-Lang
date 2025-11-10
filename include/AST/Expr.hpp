#pragma once

#include <cassert>
#include <memory>
#include <optional>
#include <string>
#include <utility>
#include <variant>
#include <vector>

#include <llvm/IR/Value.h>

#include "AST/Decl.hpp"
#include "AST/Stmt.hpp"
#include "AST/Type.hpp"
#include "Lexer/Token.hpp"
#include "Lexer/TokenKind.hpp"
#include "Sema/TypeInference/Substitution.hpp"
#include "SrcManager/SrcLocation.hpp"

namespace phi {

// Forward declarations
class Decl;
class ValueDecl;
class FunDecl;
class FieldDecl;
class StructDecl;
class MethodDecl;
class EnumDecl;
class VariantDecl;

class NameResolver;
class TypeInferencer;
class CodeGen;

using InferRes = std::pair<Substitution, Monotype>;

//===----------------------------------------------------------------------===//
// Expr - Base class for all Expression nodes
//===----------------------------------------------------------------------===//

class Expr {
public:
  /// @brief Kind enumeration for LLVM RTTI
  enum class Kind : uint8_t {
    IntLiteralKind,
    FloatLiteralKind,
    StrLiteralKind,
    CharLiteralKind,
    BoolLiteralKind,
    RangeLiteralKind,
    TupleLiteralKind,
    DeclRefKind,
    FunCallKind,
    BinaryOpKind,
    UnaryOpKind,
    MemberInitKind,
    FieldAccessKind,
    MethodCallKind,
    EnumInitKind,
    MatchExprKind,
    CustomTypeCtorKind, // <-- added for new CustomTypeCtor expr
  };

  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  explicit Expr(const Kind K, SrcLocation Location,
                std::optional<Type> Ty = std::nullopt)
      : ExprKind(K), Location(std::move(Location)), Ty(std::move(Ty)) {}

  virtual ~Expr() = default;

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] Kind getKind() const { return ExprKind; }
  [[nodiscard]] SrcLocation &getLocation() { return Location; }
  [[nodiscard]] Type getType() { return Ty.value(); }

  //===--------------------------------------------------------------------===//
  // Setters
  //===--------------------------------------------------------------------===//

  void setType(Type T) { Ty = std::move(T); }

  //===--------------------------------------------------------------------===//
  // Type Queries
  //===--------------------------------------------------------------------===//

  [[nodiscard]] bool hasType() const { return Ty.has_value(); }
  [[nodiscard]] virtual bool isAssignable() const = 0;

  //===--------------------------------------------------------------------===//
  // Visitor Methods
  //===--------------------------------------------------------------------===//

  virtual bool accept(NameResolver &R) = 0;
  virtual InferRes accept(TypeInferencer &I) = 0;
  virtual bool accept(TypeChecker &C) = 0;
  virtual llvm::Value *accept(CodeGen &G) = 0;

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===--------------------------------------------------------------------===//

  static bool classof(const Expr *E) {
    (void)E;
    return true;
  }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//

  virtual void emit(int Level) const = 0;

private:
  Kind ExprKind;

protected:
  SrcLocation Location;
  std::optional<Type> Ty;
};

//===----------------------------------------------------------------------===//
// Literal Expression Classes
//===----------------------------------------------------------------------===//

class IntLiteral final : public Expr {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  IntLiteral(SrcLocation Location, int64_t Value);

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] int64_t getValue() const { return Value; }

  //===--------------------------------------------------------------------===//
  // Type Queries
  //===--------------------------------------------------------------------===//

  [[nodiscard]] bool isAssignable() const override { return false; }

  //===--------------------------------------------------------------------===//
  // Visitor Methods
  //===--------------------------------------------------------------------===//

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  bool accept(TypeChecker &C) override;
  llvm::Value *accept(CodeGen &G) override;

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===--------------------------------------------------------------------===//

  static bool classof(const Expr *E) {
    return E->getKind() == Kind::IntLiteralKind;
  }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//

  void emit(int Level) const override;

private:
  int64_t Value;
};

class FloatLiteral final : public Expr {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  FloatLiteral(SrcLocation Location, double Value);

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] double getValue() const { return Value; }

  //===--------------------------------------------------------------------===//
  // Type Queries
  //===-----------------------------------------------------------------------//

  [[nodiscard]] bool isAssignable() const override { return false; }

  //===--------------------------------------------------------------------===//
  // Visitor Methods
  //===--------------------------------------------------------------------===//

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  bool accept(TypeChecker &C) override;
  llvm::Value *accept(CodeGen &G) override;

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===-----------------------------------------------------------------------//

  static bool classof(const Expr *E) {
    return E->getKind() == Kind::FloatLiteralKind;
  }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//

  void emit(int Level) const override;

private:
  double Value;
};

class StrLiteral final : public Expr {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  StrLiteral(SrcLocation Location, std::string Value);

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] std::string getValue() const { return Value; }

  //===--------------------------------------------------------------------===//
  // Type Queries
  //===--------------------------------------------------------------------===//

  [[nodiscard]] bool isAssignable() const override { return false; }

  //===--------------------------------------------------------------------===//
  // Visitor Methods
  //===--------------------------------------------------------------------===//

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  bool accept(TypeChecker &C) override;
  llvm::Value *accept(CodeGen &G) override;

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===-----------------------------------------------------------------------//

  static bool classof(const Expr *E) {
    return E->getKind() == Kind::StrLiteralKind;
  }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//

  void emit(int Level) const override;

private:
  std::string Value;
};

class CharLiteral final : public Expr {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  CharLiteral(SrcLocation Location, char Value);

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] char getValue() const { return Value; }

  //===--------------------------------------------------------------------===//
  // Type Queries
  //===--------------------------------------------------------------------===//

  [[nodiscard]] bool isAssignable() const override { return false; }

  //===--------------------------------------------------------------------===//
  // Visitor Methods
  //===-----------------------------------------------------------------------//

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  bool accept(TypeChecker &C) override;
  llvm::Value *accept(CodeGen &G) override;

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===-----------------------------------------------------------------------//

  static bool classof(const Expr *E) {
    return E->getKind() == Kind::CharLiteralKind;
  }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//

  void emit(int Level) const override;

private:
  char Value;
};

class BoolLiteral final : public Expr {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  BoolLiteral(SrcLocation Location, bool Value);

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] bool getValue() const { return Value; }

  //===--------------------------------------------------------------------===//
  // Type Queries
  //===--------------------------------------------------------------------===//

  [[nodiscard]] bool isAssignable() const override { return false; }

  //===--------------------------------------------------------------------===//
  // Visitor Methods
  //===-----------------------------------------------------------------------//

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  bool accept(TypeChecker &C) override;
  llvm::Value *accept(CodeGen &G) override;

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===-----------------------------------------------------------------------//

  static bool classof(const Expr *E) {
    return E->getKind() == Kind::BoolLiteralKind;
  }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//

  void emit(int Level) const override;

private:
  bool Value;
};

class RangeLiteral final : public Expr {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  RangeLiteral(SrcLocation Location, std::unique_ptr<Expr> Start,
               std::unique_ptr<Expr> End, bool Inclusive);
  ~RangeLiteral() override;

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] Expr &getStart() const { return *Start; }
  [[nodiscard]] Expr &getEnd() const { return *End; }
  [[nodiscard]] bool isInclusive() const { return Inclusive; }

  //===--------------------------------------------------------------------===//
  // Type Queries
  //===--------------------------------------------------------------------===//

  [[nodiscard]] bool isAssignable() const override { return false; }

  //===--------------------------------------------------------------------===//
  // Visitor Methods
  //===-----------------------------------------------------------------------//

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  bool accept(TypeChecker &C) override;
  llvm::Value *accept(CodeGen &G) override;

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===-----------------------------------------------------------------------//

  static bool classof(const Expr *E) {
    return E->getKind() == Kind::RangeLiteralKind;
  }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//

  void emit(int Level) const override;

private:
  std::unique_ptr<Expr> Start, End;
  bool Inclusive;
};

class TupleLiteral final : public Expr {
public:
  TupleLiteral(SrcLocation Location,
               std::vector<std::unique_ptr<Expr>> Elements);
  ~TupleLiteral() override;

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] const auto &getElements() { return Elements; }

  //===--------------------------------------------------------------------===//
  // Type Queries
  //===--------------------------------------------------------------------===//

  [[nodiscard]] bool isAssignable() const override { return false; }

  //===--------------------------------------------------------------------===//
  // Visitor Methods
  //===-----------------------------------------------------------------------//

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  bool accept(TypeChecker &C) override;
  llvm::Value *accept(CodeGen &G) override;

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===-----------------------------------------------------------------------//

  static bool classof(const Expr *E) {
    return E->getKind() == Kind::TupleLiteralKind;
  }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===-----------------------------------------------------------------------//

  void emit(int Level) const override;

private:
  std::vector<std::unique_ptr<Expr>> Elements;
};

//===----------------------------------------------------------------------===//
// Reference and Call Expression Classes
//===----------------------------------------------------------------------===//

class DeclRefExpr final : public Expr {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  DeclRefExpr(SrcLocation Location, std::string Id);

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] std::string getId() { return Id; }
  [[nodiscard]] ValueDecl *getDecl() const { return DeclPtr; }

  //===--------------------------------------------------------------------===//
  // Setters
  //===--------------------------------------------------------------------===//

  void setDecl(ValueDecl *d) { DeclPtr = d; }

  //===--------------------------------------------------------------------===//
  // Type Queries
  //===--------------------------------------------------------------------===//

  [[nodiscard]] bool isAssignable() const override { return true; }

  //===--------------------------------------------------------------------===//
  // Visitor Methods
  //===-----------------------------------------------------------------------//

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  bool accept(TypeChecker &C) override;
  llvm::Value *accept(CodeGen &G) override;

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===-----------------------------------------------------------------------//

  static bool classof(const Expr *E) {
    return E->getKind() == Kind::DeclRefKind;
  }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//

  void emit(int Level) const override;

private:
  std::string Id;
  ValueDecl *DeclPtr = nullptr;
};

class FunCallExpr : public Expr {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  FunCallExpr(SrcLocation Location, std::unique_ptr<Expr> Callee,
              std::vector<std::unique_ptr<Expr>> Args);

protected:
  // Constructor for derived classes to specify their own Kind
  FunCallExpr(Kind K, SrcLocation Location, std::unique_ptr<Expr> Callee,
              std::vector<std::unique_ptr<Expr>> Args);

  FunCallExpr(FunCallExpr &&Other, Kind K)
      : Expr(K, Other.getLocation()), Callee(std::move(Other.Callee)),
        Args(std::move(Other.Args)), Decl(Other.Decl) {}

public:
  ~FunCallExpr() override;

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] Expr &getCallee() const { return *Callee; }
  [[nodiscard]] std::vector<std::unique_ptr<Expr>> &getArgs() { return Args; }
  [[nodiscard]] const std::vector<std::unique_ptr<Expr>> &getArgs() const {
    return Args;
  }
  [[nodiscard]] FunDecl *getDecl() const { return Decl; }

  //===--------------------------------------------------------------------===//
  // Setters
  //===--------------------------------------------------------------------===//

  void setDecl(FunDecl *F) { Decl = F; }

  //===--------------------------------------------------------------------===//
  // Type Queries
  //===--------------------------------------------------------------------===//

  [[nodiscard]] bool isAssignable() const override { return false; }

  //===--------------------------------------------------------------------===//
  // Visitor Methods
  //===-----------------------------------------------------------------------//

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  bool accept(TypeChecker &C) override;
  llvm::Value *accept(CodeGen &G) override;

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===-----------------------------------------------------------------------//

  static bool classof(const Expr *E) {
    return E->getKind() == Kind::FunCallKind;
  }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//

  void emit(int Level) const override;

private:
  std::unique_ptr<Expr> Callee;
  std::vector<std::unique_ptr<Expr>> Args;
  FunDecl *Decl = nullptr;
};

//===----------------------------------------------------------------------===//
// Operator Expression Classes
//===----------------------------------------------------------------------===//

class BinaryOp final : public Expr {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  BinaryOp(std::unique_ptr<Expr> Lhs, std::unique_ptr<Expr> Rhs,
           const Token &Op);
  ~BinaryOp() override;

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] Expr &getLhs() const { return *Lhs; }
  [[nodiscard]] Expr &getRhs() const { return *Rhs; }
  [[nodiscard]] TokenKind getOp() const { return Op; }

  //===--------------------------------------------------------------------===//
  // Type Queries
  //===-----------------------------------------------------------------------//

  [[nodiscard]] bool isAssignable() const override { return false; }

  //===--------------------------------------------------------------------===//
  // Visitor Methods
  //===-----------------------------------------------------------------------//

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  bool accept(TypeChecker &C) override;
  llvm::Value *accept(CodeGen &G) override;

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===-----------------------------------------------------------------------//

  static bool classof(const Expr *E) {
    return E->getKind() == Kind::BinaryOpKind;
  }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===-----------------------------------------------------------------------//

  void emit(int Level) const override;

private:
  std::unique_ptr<Expr> Lhs;
  std::unique_ptr<Expr> Rhs;
  TokenKind Op;
};

class UnaryOp final : public Expr {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===-----------------------------------------------------------------------//

  UnaryOp(std::unique_ptr<Expr> Operand, const Token &Op, bool IsPrefix);
  ~UnaryOp() override;

  //===--------------------------------------------------------------------===//
  // Getters
  //===-----------------------------------------------------------------------//

  [[nodiscard]] Expr &getOperand() const { return *Operand; }
  [[nodiscard]] TokenKind getOp() const { return Op; }
  [[nodiscard]] bool isPrefixOp() const { return IsPrefix; }

  //===--------------------------------------------------------------------===//
  // Type Queries
  //===-----------------------------------------------------------------------//

  [[nodiscard]] bool isAssignable() const override { return false; }

  //===--------------------------------------------------------------------===//
  // Visitor Methods
  //===-----------------------------------------------------------------------//

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  bool accept(TypeChecker &C) override;
  llvm::Value *accept(CodeGen &G) override;

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===-----------------------------------------------------------------------//

  static bool classof(const Expr *E) {
    return E->getKind() == Kind::UnaryOpKind;
  }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===-----------------------------------------------------------------------//

  void emit(int Level) const override;

private:
  std::unique_ptr<Expr> Operand;
  TokenKind Op;
  bool IsPrefix;
};

//===----------------------------------------------------------------------===//
// Struct and Field Expression Classes
//===----------------------------------------------------------------------===//

class MemberInitExpr final : public Expr {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===-----------------------------------------------------------------------//

  MemberInitExpr(SrcLocation Location, std::string FieldId,
                 std::unique_ptr<Expr> Init);
  ~MemberInitExpr() override;

  //===--------------------------------------------------------------------===//
  // Getters
  //===-----------------------------------------------------------------------//

  [[nodiscard]] const std::string &getId() const { return FieldId; }
  [[nodiscard]] FieldDecl *getDecl() const { return FieldDecl; }
  [[nodiscard]] Expr *getInitValue() const { return InitValue.get(); }

  //===--------------------------------------------------------------------===//
  // Setters
  //===--------------------------------------------------------------------===//

  void setDecl(FieldDecl *decl) { FieldDecl = decl; }

  //===--------------------------------------------------------------------===//
  // Type Queries
  //===-----------------------------------------------------------------------//

  [[nodiscard]] bool isAssignable() const override { return false; }

  //===--------------------------------------------------------------------===//
  // Visitor Methods
  //===-----------------------------------------------------------------------//

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  bool accept(TypeChecker &C) override;
  llvm::Value *accept(CodeGen &G) override;

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===-----------------------------------------------------------------------//

  static bool classof(const Expr *E) {
    return E->getKind() == Kind::MemberInitKind;
  }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===-----------------------------------------------------------------------//

  void emit(int Level) const override;

private:
  std::string FieldId;
  std::unique_ptr<Expr> InitValue;
  FieldDecl *FieldDecl = nullptr;
};

//===----------------------------------------------------------------------===//
// CustomTypeCtor - new expression which can represent either a struct-like
// constructor (with named fields) or an enum variant constructor.
//===----------------------------------------------------------------------===//

class CustomTypeCtor final : public Expr {
public:
  CustomTypeCtor(SrcLocation Location, std::optional<std::string> TypeName,
                 std::vector<std::unique_ptr<MemberInitExpr>> Inits);
  ~CustomTypeCtor() override;

  enum class InterpretAs : uint8_t { Struct, Enum, Unknown };

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] const auto &getTypeName() const { return *TypeName; }
  [[nodiscard]] const auto &getInits() const { return Inits; }
  [[nodiscard]] const auto &getInterpretation() const { return InterpretAs; }
  [[nodiscard]] const auto &getDecl() const { return Decl; }
  [[nodiscard]] bool isAnonymous() const { return TypeName.has_value(); }

  //===--------------------------------------------------------------------===//
  // Setters
  //===--------------------------------------------------------------------===//

  void setDecl(EnumDecl *Found) {
    assert(InterpretAs != InterpretAs::Struct || "Cannot change interpretation"
                                                 "of CustomTypeCtor");
    InterpretAs = InterpretAs::Enum;
    Decl = Found;
  }

  void setActiveVariant(VariantDecl *Variant) {
    assert(Variant != nullptr && "Param Variant must not be null");
    assert(InterpretAs == InterpretAs::Struct && "Interpretation must be enum");
    assert(std::holds_alternative<EnumDecl *>(getDecl()) &&
           "Decl must be enum");

    ActiveVariantDecl = Variant;
    ActiveVariantName = Variant->getId();
  }

  void setDecl(StructDecl *Found) {
    assert(InterpretAs != InterpretAs::Enum || "Cannot change interpretation"
                                               "of CustomTypeCtor");
    InterpretAs = InterpretAs::Struct;
    Decl = Found;
  }

  //===--------------------------------------------------------------------===//
  // Type Queries
  //===-----------------------------------------------------------------------//

  [[nodiscard]] bool isAssignable() const override { return true; }

  //===--------------------------------------------------------------------===//
  // Visitor Methods
  //===-----------------------------------------------------------------------//

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  bool accept(TypeChecker &C) override;
  llvm::Value *accept(CodeGen &G) override;

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===--------------------------------------------------------------------===//

  static bool classof(const Expr *E) {
    return E->getKind() == Kind::CustomTypeCtorKind;
  }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===-----------------------------------------------------------------------//

  void emit(int Level) const override;

private:
  std::optional<std::string> TypeName;
  std::vector<std::unique_ptr<MemberInitExpr>> Inits;

  InterpretAs InterpretAs = InterpretAs::Unknown;
  std::variant<StructDecl *, EnumDecl *, std::monostate> Decl =
      std::monostate();

  std::optional<std::string> ActiveVariantName;
  VariantDecl *ActiveVariantDecl = nullptr;
};

//===----------------------------------------------------------------------===//
// Member Access Expression Classes
//===----------------------------------------------------------------------===//

class FieldAccessExpr final : public Expr {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===-----------------------------------------------------------------------//

  FieldAccessExpr(SrcLocation Location, std::unique_ptr<Expr> Base,
                  std::string MemberId);
  ~FieldAccessExpr() override;

  //===--------------------------------------------------------------------===//
  // Getters
  //===-----------------------------------------------------------------------//

  [[nodiscard]] const FieldDecl *getField() const { return Member; }
  [[nodiscard]] Expr *getBase() const { return Base.get(); }
  [[nodiscard]] const std::string &getFieldId() const { return FieldId; }

  //===--------------------------------------------------------------------===//
  // Setters
  //===-----------------------------------------------------------------------//

  void setMember(FieldDecl *Field) { Member = Field; }

  //===--------------------------------------------------------------------===//
  // Type Queries
  //===-----------------------------------------------------------------------//

  [[nodiscard]] bool isAssignable() const override { return true; }

  //===--------------------------------------------------------------------===//
  // Visitor Methods
  //===-----------------------------------------------------------------------//

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  bool accept(TypeChecker &C) override;
  llvm::Value *accept(CodeGen &G) override;

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===-----------------------------------------------------------------------//

  static bool classof(const Expr *E) {
    return E->getKind() == Kind::FieldAccessKind;
  }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===-----------------------------------------------------------------------//

  void emit(int Level) const override;

private:
  std::unique_ptr<Expr> Base;
  std::string FieldId;
  FieldDecl *Member = nullptr;
};

class MethodCallExpr final : public FunCallExpr {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===-----------------------------------------------------------------------//

  MethodCallExpr(SrcLocation Location, std::unique_ptr<Expr> Base,
                 std::unique_ptr<Expr> Callee,
                 std::vector<std::unique_ptr<Expr>> Args);
  MethodCallExpr(FunCallExpr &&Call, std::unique_ptr<Expr> Base);

  ~MethodCallExpr() override;

  //===--------------------------------------------------------------------===//
  // Getters
  //===-----------------------------------------------------------------------//

  [[nodiscard]] const MethodDecl &getMethod() const { return *Method; }
  [[nodiscard]] Expr *getBase() const { return Base.get(); }
  // Inherited from FunCallExpr: getCallee(), getArgs(), getDecl(), setDecl()

  //===--------------------------------------------------------------------===//
  // Setters
  //===-----------------------------------------------------------------------//

  void setMethod(MethodDecl *M) {
    setDecl(M);
    Method = M;
  }

  //===--------------------------------------------------------------------===//
  // Type Queries
  //===-----------------------------------------------------------------------//

  [[nodiscard]] bool isAssignable() const override { return true; }

  //===--------------------------------------------------------------------===//
  // Visitor Methods
  //===-----------------------------------------------------------------------//

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  bool accept(TypeChecker &C) override;
  llvm::Value *accept(CodeGen &G) override;

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===-----------------------------------------------------------------------//

  static bool classof(const Expr *E) {
    return E->getKind() == Kind::MethodCallKind;
  }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===-----------------------------------------------------------------------//

  void emit(int Level) const override;

private:
  std::unique_ptr<Expr> Base;
  MethodDecl *Method = nullptr;
};

class MatchExpr final : public Expr {
public:
  struct Arm {
    std::vector<std::unique_ptr<Expr>> Patterns;
    std::unique_ptr<Block> Body;
    Expr *Return;
  };

  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===-----------------------------------------------------------------------//

  MatchExpr(SrcLocation Location, std::unique_ptr<Expr> Value,
            std::vector<Arm> Cases);
  ~MatchExpr() override;

  //===--------------------------------------------------------------------===//
  // Getters
  //===-----------------------------------------------------------------------//

  [[nodiscard]] Expr *getValue() const { return Scrutinee.get(); }
  [[nodiscard]] auto &getCases() const { return Arms; }

  //===--------------------------------------------------------------------===//
  // Type Queries
  //===-----------------------------------------------------------------------//

  [[nodiscard]] bool isAssignable() const override { return true; }

  //===--------------------------------------------------------------------===//
  // Visitor Methods
  //===-----------------------------------------------------------------------//

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  bool accept(TypeChecker &C) override;
  llvm::Value *accept(CodeGen &G) override;

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===-----------------------------------------------------------------------//

  static bool classof(const Expr *E) {
    return E->getKind() == Kind::MatchExprKind;
  }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===-----------------------------------------------------------------------//

  void emit(int Level) const override;

private:
  std::unique_ptr<Expr> Scrutinee;
  std::vector<Arm> Arms;
};

} // namespace phi
