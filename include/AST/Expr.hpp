#pragma once

#include <memory>
#include <optional>
#include <string>
#include <utility>
#include <vector>

#include <llvm/IR/Value.h>

#include "AST/Decl.hpp"
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

/**
 * @brief Base class for all Expression nodes
 */
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
    DeclRefExprKind,
    FunCallExprKind,
    BinaryOpKind,
    UnaryOpKind,
    StructInitKind,
    FieldInitKind,
    MemberAccessKind,
    MemberFunAccessKind
  };

  explicit Expr(Kind K, SrcLocation Location,
                std::optional<Type> Ty = std::nullopt)
      : ExprKind(K), Location(std::move(Location)), Ty(std::move(Ty)) {}

  virtual ~Expr() = default;

  [[nodiscard]] Kind getKind() const { return ExprKind; }
  [[nodiscard]] SrcLocation &getLocation() { return Location; }

  [[nodiscard]] Type getType() { return Ty.value(); }
  [[nodiscard]] bool isResolved() const { return Ty.has_value(); }
  [[nodiscard]] virtual bool isAssignable() const = 0;

  void setType(Type T) { Ty = std::move(T); }

  virtual void emit(int Level) const = 0;
  virtual bool accept(NameResolver &R) = 0;
  virtual InferRes accept(TypeInferencer &I) = 0;
  virtual llvm::Value *accept(CodeGen &G) = 0;

  static bool classof(const Expr *E) { return true; }

private:
  const Kind ExprKind;

protected:
  SrcLocation Location;
  std::optional<Type> Ty;
};

class IntLiteral final : public Expr {
public:
  IntLiteral(SrcLocation Location, const int64_t Value);

  [[nodiscard]] int64_t getValue() const { return Value; }
  [[nodiscard]] bool isAssignable() const override { return false; }

  void emit(int level) const override;

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  llvm::Value *accept(CodeGen &G) override;

  static bool classof(const Expr *E) {
    return E->getKind() == Kind::IntLiteralKind;
  }

private:
  int64_t Value;
};

class FloatLiteral final : public Expr {
public:
  FloatLiteral(SrcLocation Location, const double Value);

  [[nodiscard]] double getValue() const { return Value; }
  [[nodiscard]] bool isAssignable() const override { return false; }

  void emit(int level) const override;

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  llvm::Value *accept(CodeGen &G) override;

  static bool classof(const Expr *E) {
    return E->getKind() == Kind::FloatLiteralKind;
  }

private:
  double Value;
};

class StrLiteral final : public Expr {
public:
  StrLiteral(SrcLocation Location, std::string Value);

  [[nodiscard]] std::string getValue() const { return Value; }
  [[nodiscard]] bool isAssignable() const override { return false; }

  void emit(int level) const override;

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  llvm::Value *accept(CodeGen &G) override;

  static bool classof(const Expr *E) {
    return E->getKind() == Kind::StrLiteralKind;
  }

private:
  std::string Value;
};

class CharLiteral final : public Expr {
public:
  CharLiteral(SrcLocation Location, const char Value);

  [[nodiscard]] char getValue() const { return Value; }
  [[nodiscard]] bool isAssignable() const override { return false; }

  void emit(int level) const override;

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  llvm::Value *accept(CodeGen &G) override;

  static bool classof(const Expr *E) {
    return E->getKind() == Kind::CharLiteralKind;
  }

private:
  char Value;
};

class BoolLiteral final : public Expr {
public:
  BoolLiteral(SrcLocation Location, bool Value);

  [[nodiscard]] bool getValue() const { return Value; }
  [[nodiscard]] bool isAssignable() const override { return false; }

  void emit(int level) const override;

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  llvm::Value *accept(CodeGen &G) override;

  static bool classof(const Expr *E) {
    return E->getKind() == Kind::BoolLiteralKind;
  }

private:
  bool Value;
};

class RangeLiteral final : public Expr {
public:
  RangeLiteral(SrcLocation Location, std::unique_ptr<Expr> Start,
               std::unique_ptr<Expr> End, const bool Inclusive);
  ~RangeLiteral() override; // Need destructor

  [[nodiscard]] Expr &getStart() { return *Start; }
  [[nodiscard]] Expr &getEnd() { return *End; }
  [[nodiscard]] bool isInclusive() const { return Inclusive; }
  [[nodiscard]] bool isAssignable() const override { return false; }

  void emit(int level) const override;

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  llvm::Value *accept(CodeGen &G) override;

  static bool classof(const Expr *E) {
    return E->getKind() == Kind::RangeLiteralKind;
  }

private:
  std::unique_ptr<Expr> Start, End;
  bool Inclusive;
};

class DeclRefExpr final : public Expr {
public:
  DeclRefExpr(SrcLocation Location, std::string Id);

  [[nodiscard]] std::string getId() { return Id; }
  [[nodiscard]] ValueDecl *getDecl() const { return DeclPtr; }
  [[nodiscard]] bool isAssignable() const override { return true; }

  void setDecl(ValueDecl *d) { DeclPtr = d; }

  void emit(int level) const override;

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  llvm::Value *accept(CodeGen &G) override;

  static bool classof(const Expr *E) {
    return E->getKind() == Kind::DeclRefExprKind;
  }

private:
  std::string Id;
  ValueDecl *DeclPtr = nullptr;
};

class FunCallExpr final : public Expr {
public:
  FunCallExpr(SrcLocation Location, std::unique_ptr<Expr> Callee,
              std::vector<std::unique_ptr<Expr>> Args);
  ~FunCallExpr() override; // Need destructor

  [[nodiscard]] Expr &getCallee() const { return *Callee; }
  [[nodiscard]] std::vector<std::unique_ptr<Expr>> &getArgs() { return Args; }
  [[nodiscard]] FunDecl *getDecl() const { return Decl; }
  [[nodiscard]] bool isAssignable() const override { return false; }

  void setDecl(FunDecl *F) { Decl = F; }

  void emit(int level) const override;

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  llvm::Value *accept(CodeGen &G) override;

  static bool classof(const Expr *E) {
    return E->getKind() == Kind::FunCallExprKind;
  }

private:
  std::unique_ptr<Expr> Callee;
  std::vector<std::unique_ptr<Expr>> Args;
  FunDecl *Decl = nullptr;
};

class BinaryOp final : public Expr {
public:
  BinaryOp(std::unique_ptr<Expr> Lhs, std::unique_ptr<Expr> Rhs,
           const Token &Op);
  ~BinaryOp() override; // Need destructor

  [[nodiscard]] Expr &getLhs() const { return *Lhs; }
  [[nodiscard]] Expr &getRhs() const { return *Rhs; }
  [[nodiscard]] bool isAssignable() const override { return false; }
  [[nodiscard]] TokenKind getOp() const { return Op; }

  void emit(int level) const override;

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  llvm::Value *accept(CodeGen &G) override;

  static bool classof(const Expr *E) {
    return E->getKind() == Kind::BinaryOpKind;
  }

private:
  std::unique_ptr<Expr> Lhs;
  std::unique_ptr<Expr> Rhs;
  TokenKind Op;
};

class UnaryOp final : public Expr {
public:
  UnaryOp(std::unique_ptr<Expr> Operand, const Token &Op, const bool IsPrefix);
  ~UnaryOp() override; // Need destructor

  [[nodiscard]] Expr &getOperand() const { return *Operand; }
  [[nodiscard]] TokenKind getOp() const { return Op; }
  [[nodiscard]] bool isPrefixOp() const { return IsPrefix; }
  [[nodiscard]] bool isAssignable() const override { return false; }

  void emit(int level) const override;

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  llvm::Value *accept(CodeGen &G) override;

  static bool classof(const Expr *E) {
    return E->getKind() == Kind::UnaryOpKind;
  }

private:
  std::unique_ptr<Expr> Operand;
  TokenKind Op;
  bool IsPrefix;
};

class FieldInitExpr final : public Expr {
public:
  FieldInitExpr(SrcLocation Location, std::string FieldId,
                std::unique_ptr<Expr> Init);
  ~FieldInitExpr() override; // Need destructor

  [[nodiscard]] const std::string &getFieldId() const { return FieldId; }
  [[nodiscard]] FieldDecl *getDecl() const { return FieldDecl; }
  [[nodiscard]] Expr *getValue() const { return Init.get(); }
  [[nodiscard]] bool isAssignable() const override { return false; }

  void emit(int level) const override;

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  llvm::Value *accept(CodeGen &G) override;

  static bool classof(const Expr *E) {
    return E->getKind() == Kind::FieldInitKind;
  }

  void setFieldDecl(FieldDecl *decl) { FieldDecl = decl; }

private:
  std::string FieldId;
  std::unique_ptr<Expr> Init;
  FieldDecl *FieldDecl = nullptr;
};

class StructInitExpr final : public Expr {
public:
  StructInitExpr(SrcLocation Location, std::string StructId,
                 std::vector<std::unique_ptr<FieldInitExpr>> Fields);
  ~StructInitExpr() override; // Need destructor

  [[nodiscard]] StructDecl *getStructDecl() const { return StructDecl; }
  [[nodiscard]] const std::string &getStructId() const { return StructId; }
  [[nodiscard]] const std::vector<std::unique_ptr<FieldInitExpr>> &
  getFields() const {
    return FieldInits;
  }
  [[nodiscard]] bool isAssignable() const override { return false; }

  void emit(int level) const override;

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  llvm::Value *accept(CodeGen &G) override;

  static bool classof(const Expr *E) {
    return E->getKind() == Kind::StructInitKind;
  }

  void setStructDecl(StructDecl *decl) { StructDecl = decl; }

private:
  std::string StructId;
  std::vector<std::unique_ptr<FieldInitExpr>> FieldInits;
  StructDecl *StructDecl = nullptr;
};

class MemberAccessExpr final : public Expr {
public:
  MemberAccessExpr(SrcLocation Location, std::unique_ptr<Expr> Base,
                   std::string MemberId);
  ~MemberAccessExpr() override;

  [[nodiscard]] const FieldDecl &getField() const { return *Member; }
  [[nodiscard]] Expr *getBase() const { return Base.get(); }
  [[nodiscard]] const std::string &getMemberId() const { return MemberId; }
  [[nodiscard]] bool isAssignable() const override { return true; }

  void setMember(FieldDecl *Field) { Member = Field; }

  void emit(int level) const override;

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  llvm::Value *accept(CodeGen &G) override;

  static bool classof(const Expr *E) {
    return E->getKind() == Kind::MemberAccessKind;
  }

private:
  std::unique_ptr<Expr> Base;
  std::string MemberId;
  FieldDecl *Member = nullptr;
};

class MemberFunCallExpr final : public Expr {
public:
  MemberFunCallExpr(SrcLocation Location, std::unique_ptr<Expr> Base,
                    std::unique_ptr<FunCallExpr> FunCall);
  ~MemberFunCallExpr() override;

  [[nodiscard]] const MethodDecl &getMethod() const { return *Method; }
  [[nodiscard]] Expr *getBase() const { return Base.get(); }
  [[nodiscard]] FunCallExpr &getCall() const { return *FunCall; }
  [[nodiscard]] bool isAssignable() const override { return true; }

  void setMethod(MethodDecl *M) { Method = M; }

  void emit(int level) const override;

  bool accept(NameResolver &R) override;
  InferRes accept(TypeInferencer &I) override;
  llvm::Value *accept(CodeGen &G) override;

  static bool classof(const Expr *E) {
    return E->getKind() == Kind::MemberFunAccessKind;
  }

private:
  std::unique_ptr<Expr> Base;
  std::unique_ptr<FunCallExpr> FunCall;
  MethodDecl *Method;
};

} // namespace phi
