#pragma once

#include <memory>
#include <optional>
#include <string>
#include <utility>
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
    DeclRefExprKind,
    FunCallExprKind,
    BinaryOpKind,
    UnaryOpKind,
    StructLiteralKind,
    FieldInitKind,
    FieldAccessKind,
    MethodCallKind,
    MatchExprKind,
  };

  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  explicit Expr(Kind K, SrcLocation Location,
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

  static bool classof(const Expr *E) { return true; }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//

  virtual void emit(int Level) const = 0;

private:
  const Kind ExprKind;

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

  IntLiteral(SrcLocation Location, const int64_t Value);

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

  FloatLiteral(SrcLocation Location, const double Value);

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] double getValue() const { return Value; }

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
  //===--------------------------------------------------------------------===//

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

  CharLiteral(SrcLocation Location, const char Value);

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
  //===--------------------------------------------------------------------===//

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  bool accept(TypeChecker &C) override;
  llvm::Value *accept(CodeGen &G) override;

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===--------------------------------------------------------------------===//

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
  //===--------------------------------------------------------------------===//

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  bool accept(TypeChecker &C) override;
  llvm::Value *accept(CodeGen &G) override;

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===--------------------------------------------------------------------===//

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
               std::unique_ptr<Expr> End, const bool Inclusive);
  ~RangeLiteral() override;

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] Expr &getStart() { return *Start; }
  [[nodiscard]] Expr &getEnd() { return *End; }
  [[nodiscard]] bool isInclusive() const { return Inclusive; }

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
  //===--------------------------------------------------------------------===//

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  bool accept(TypeChecker &C) override;
  llvm::Value *accept(CodeGen &G) override;

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===--------------------------------------------------------------------===//

  static bool classof(const Expr *E) {
    return E->getKind() == Kind::TupleLiteralKind;
  }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//

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
  //===--------------------------------------------------------------------===//

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  bool accept(TypeChecker &C) override;
  llvm::Value *accept(CodeGen &G) override;

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===--------------------------------------------------------------------===//

  static bool classof(const Expr *E) {
    return E->getKind() == Kind::DeclRefExprKind;
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
  //===--------------------------------------------------------------------===//

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  bool accept(TypeChecker &C) override;
  llvm::Value *accept(CodeGen &G) override;

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===--------------------------------------------------------------------===//

  static bool classof(const Expr *E) {
    return E->getKind() == Kind::FunCallExprKind;
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
    return E->getKind() == Kind::BinaryOpKind;
  }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//

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
  //===--------------------------------------------------------------------===//

  UnaryOp(std::unique_ptr<Expr> Operand, const Token &Op, const bool IsPrefix);
  ~UnaryOp() override;

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] Expr &getOperand() const { return *Operand; }
  [[nodiscard]] TokenKind getOp() const { return Op; }
  [[nodiscard]] bool isPrefixOp() const { return IsPrefix; }

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
    return E->getKind() == Kind::UnaryOpKind;
  }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//

  void emit(int Level) const override;

private:
  std::unique_ptr<Expr> Operand;
  TokenKind Op;
  bool IsPrefix;
};

//===----------------------------------------------------------------------===//
// Struct and Field Expression Classes
//===----------------------------------------------------------------------===//

class FieldInitExpr final : public Expr {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  FieldInitExpr(SrcLocation Location, std::string FieldId,
                std::unique_ptr<Expr> Init);
  ~FieldInitExpr() override;

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] const std::string &getFieldId() const { return FieldId; }
  [[nodiscard]] FieldDecl *getDecl() const { return FieldDecl; }
  [[nodiscard]] Expr *getValue() const { return Init.get(); }

  //===--------------------------------------------------------------------===//
  // Setters
  //===--------------------------------------------------------------------===//

  void setFieldDecl(FieldDecl *decl) { FieldDecl = decl; }

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
    return E->getKind() == Kind::FieldInitKind;
  }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//

  void emit(int Level) const override;

private:
  std::string FieldId;
  std::unique_ptr<Expr> Init;
  FieldDecl *FieldDecl = nullptr;
};

class StructLiteral final : public Expr {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  StructLiteral(SrcLocation Location, std::string StructId,
                std::vector<std::unique_ptr<FieldInitExpr>> Fields);
  ~StructLiteral() override;

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] StructDecl *getStructDecl() const { return StructDecl; }
  [[nodiscard]] const std::string &getStructId() const { return StructId; }
  [[nodiscard]] const auto &getFields() const { return FieldInits; }

  //===--------------------------------------------------------------------===//
  // Setters
  //===--------------------------------------------------------------------===//

  void setStructDecl(StructDecl *decl) { StructDecl = decl; }

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
    return E->getKind() == Kind::StructLiteralKind;
  }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//

  void emit(int Level) const override;

private:
  std::string StructId;
  std::vector<std::unique_ptr<FieldInitExpr>> FieldInits;
  StructDecl *StructDecl = nullptr;
};

//===----------------------------------------------------------------------===//
// Member Access Expression Classes
//===----------------------------------------------------------------------===//

class FieldAccessExpr final : public Expr {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  FieldAccessExpr(SrcLocation Location, std::unique_ptr<Expr> Base,
                  std::string MemberId);
  ~FieldAccessExpr() override;

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] const FieldDecl *getField() const { return Member; }
  [[nodiscard]] Expr *getBase() const { return Base.get(); }
  [[nodiscard]] const std::string &getFieldId() const { return FieldId; }

  //===--------------------------------------------------------------------===//
  // Setters
  //===--------------------------------------------------------------------===//

  void setMember(FieldDecl *Field) { Member = Field; }

  //===--------------------------------------------------------------------===//
  // Type Queries
  //===--------------------------------------------------------------------===//

  [[nodiscard]] bool isAssignable() const override { return true; }

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
    return E->getKind() == Kind::FieldAccessKind;
  }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//

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
  //===--------------------------------------------------------------------===//

  MethodCallExpr(SrcLocation Location, std::unique_ptr<Expr> Base,
                 std::unique_ptr<Expr> Callee,
                 std::vector<std::unique_ptr<Expr>> Args);
  MethodCallExpr(FunCallExpr &&Call, std::unique_ptr<Expr> Base);

  ~MethodCallExpr() override;

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] const MethodDecl &getMethod() const { return *Method; }
  [[nodiscard]] Expr *getBase() const { return Base.get(); }
  // Inherited from FunCallExpr: getCallee(), getArgs(), getDecl(), setDecl()

  //===--------------------------------------------------------------------===//
  // Setters
  //===--------------------------------------------------------------------===//

  void setMethod(MethodDecl *M) { Method = M; }

  //===--------------------------------------------------------------------===//
  // Type Queries
  //===--------------------------------------------------------------------===//

  [[nodiscard]] bool isAssignable() const override { return true; }

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
    return E->getKind() == Kind::MethodCallKind;
  }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//

  void emit(int Level) const override;

private:
  std::unique_ptr<Expr> Base;
  MethodDecl *Method = nullptr;
};

class MatchExpr final : public Expr {
public:
  struct Case {
    std::vector<std::unique_ptr<Expr>> Patterns;
    std::unique_ptr<Block> Body;
    std::unique_ptr<Expr> Return;
  };

  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  MatchExpr(SrcLocation Location, std::unique_ptr<Expr> Value,
            std::vector<Case> Cases);
  ~MatchExpr() override;

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] Expr *getValue() const { return Value.get(); }
  [[nodiscard]] auto &getCases() const { return Cases; }

  //===--------------------------------------------------------------------===//
  // Type Queries
  //===--------------------------------------------------------------------===//

  [[nodiscard]] bool isAssignable() const override { return true; }

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
    return E->getKind() == Kind::MatchExprKind;
  }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//

  void emit(int Level) const override;

private:
  std::unique_ptr<Expr> Value;
  std::vector<Case> Cases;
};

} // namespace phi
