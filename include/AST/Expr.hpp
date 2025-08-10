#pragma once

#include "AST/Stmt.hpp"
#include "AST/Type.hpp"
#include "Lexer/Token.hpp"
#include "Lexer/TokenKind.hpp"
#include "SrcManager/SrcLocation.hpp"
#include <memory>
#include <optional>
#include <string>
#include <vector>

namespace phi {

// Forward declarations
class Decl;
class FunDecl;
class FieldDecl;
class StructDecl;
template <typename T> class ASTVisitor;

/**
 * @brief Base class for all Eession nodes
 */
class Expr : public Stmt {
public:
  explicit Expr(Kind K, SrcLocation Location,
                std::optional<Type> Ty = std::nullopt)
      : Stmt(K, std::move(Location)), Ty(std::move(Ty)) {}

  [[nodiscard]] Type getType() { return Ty.value(); }
  [[nodiscard]] bool isResolved() const { return Ty.has_value(); }
  void setType(Type t) { Ty = std::move(t); }
  [[nodiscard]] virtual bool isAssignable() const = 0;

  bool accept(ASTVisitor<bool> &Visitor) override;
  void accept(ASTVisitor<void> &Visitor) override;

  static bool classof(const Stmt *S) {
    return S->getKind() >= Kind::ExprFirst && S->getKind() <= Kind::ExprLast;
  }

protected:
  std::optional<Type> Ty;
};

class IntLiteral final : public Expr {
public:
  IntLiteral(SrcLocation Location, const int64_t Value);
  [[nodiscard]] int64_t getValue() const { return Value; }
  [[nodiscard]] bool isAssignable() const override { return false; }
  void emit(int level) const override;
  bool accept(ASTVisitor<bool> &Visitor) override;
  void accept(ASTVisitor<void> &Visitor) override;
  static bool classof(const Stmt *S) {
    return S->getKind() == Kind::IntLiteralKind;
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
  bool accept(ASTVisitor<bool> &Visitor) override;
  void accept(ASTVisitor<void> &Visitor) override;
  static bool classof(const Stmt *S) {
    return S->getKind() == Kind::FloatLiteralKind;
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
  bool accept(ASTVisitor<bool> &Visitor) override;
  void accept(ASTVisitor<void> &Visitor) override;
  static bool classof(const Stmt *S) {
    return S->getKind() == Kind::StrLiteralKind;
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
  bool accept(ASTVisitor<bool> &Visitor) override;
  void accept(ASTVisitor<void> &Visitor) override;
  static bool classof(const Stmt *S) {
    return S->getKind() == Kind::CharLiteralKind;
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
  bool accept(ASTVisitor<bool> &Visitor) override;
  void accept(ASTVisitor<void> &Visitor) override;
  static bool classof(const Stmt *S) {
    return S->getKind() == Kind::BoolLiteralKind;
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
  bool accept(ASTVisitor<bool> &Visitor) override;
  void accept(ASTVisitor<void> &Visitor) override;
  static bool classof(const Stmt *S) {
    return S->getKind() == Kind::RangeLiteralKind;
  }

private:
  std::unique_ptr<Expr> Start, End;
  bool Inclusive;
};

class DeclRefExpr final : public Expr {
public:
  DeclRefExpr(SrcLocation Location, std::string Id);
  [[nodiscard]] std::string getId() { return Id; }
  [[nodiscard]] Decl *getDecl() const { return DeclPtr; }
  void setDecl(Decl *d) { DeclPtr = d; }
  [[nodiscard]] bool isAssignable() const override { return true; }
  void emit(int level) const override;
  bool accept(ASTVisitor<bool> &Visitor) override;
  void accept(ASTVisitor<void> &Visitor) override;
  static bool classof(const Expr *E) {
    return E->getKind() == Stmt::Kind::DeclRefExprKind;
  }

private:
  std::string Id;
  Decl *DeclPtr = nullptr;
};

class FunCallExpr final : public Expr {
public:
  FunCallExpr(SrcLocation Location, std::unique_ptr<Expr> Callee,
              std::vector<std::unique_ptr<Expr>> Args);
  ~FunCallExpr() override; // Need destructor

  [[nodiscard]] Expr &getCallee() const { return *Callee; }
  [[nodiscard]] std::vector<std::unique_ptr<Expr>> &getArgs() { return Args; }
  [[nodiscard]] FunDecl *getDecl() const { return decl; }
  [[nodiscard]] bool isAssignable() const override { return false; }
  void setDecl(FunDecl *f) { decl = f; }
  void emit(int level) const override;
  bool accept(ASTVisitor<bool> &Visitor) override;
  void accept(ASTVisitor<void> &Visitor) override;
  static bool classof(const Expr *E) {
    return E->getKind() == Stmt::Kind::FunCallExprKind;
  }

private:
  std::unique_ptr<Expr> Callee;
  std::vector<std::unique_ptr<Expr>> Args;
  FunDecl *decl = nullptr;
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
  bool accept(ASTVisitor<bool> &Visitor) override;
  void accept(ASTVisitor<void> &Visitor) override;
  static bool classof(const Expr *E) {
    return E->getKind() == Stmt::Kind::BinaryOpKind;
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
  bool accept(ASTVisitor<bool> &Visitor) override;
  void accept(ASTVisitor<void> &Visitor) override;
  static bool classof(const Expr *E) {
    return E->getKind() == Stmt::Kind::UnaryOpKind;
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
  bool accept(ASTVisitor<bool> &Visitor) override;
  void accept(ASTVisitor<void> &Visitor) override;
  static bool classof(const Expr *E) {
    return E->getKind() == Stmt::Kind::FieldInitKind;
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

  [[nodiscard]] const std::string &getStructId() const { return StructId; }
  [[nodiscard]] const std::vector<std::unique_ptr<FieldInitExpr>> &
  getFields() const {
    return Fields;
  }
  [[nodiscard]] bool isAssignable() const override { return false; }
  void emit(int level) const override;
  bool accept(ASTVisitor<bool> &Visitor) override;
  void accept(ASTVisitor<void> &Visitor) override;
  static bool classof(const Expr *E) {
    return E->getKind() == Stmt::Kind::StructInitKind;
  }

  void setStructDecl(StructDecl *decl) { StructDecl = decl; }

private:
  std::string StructId;
  std::vector<std::unique_ptr<FieldInitExpr>> Fields;
  StructDecl *StructDecl = nullptr;
};

class MemberAccessExpr final : public Expr {
public:
  MemberAccessExpr(SrcLocation Location, std::unique_ptr<Expr> Base,
                   std::string MemberId);
  ~MemberAccessExpr() override;

  [[nodiscard]] Expr *getBase() const { return Base.get(); }
  [[nodiscard]] const std::string &getMemberId() const { return MemberId; }
  [[nodiscard]] bool isAssignable() const override { return true; }

  void emit(int level) const override;
  bool accept(ASTVisitor<bool> &Visitor) override;
  void accept(ASTVisitor<void> &Visitor) override;
  static bool classof(const Expr *E) {
    return E->getKind() == Stmt::Kind::MemberAccessKind;
  }

private:
  std::unique_ptr<Expr> Base;
  std::string MemberId;
};

class MemberFunCallExpr final : public Expr {
public:
  MemberFunCallExpr(SrcLocation Location, std::unique_ptr<Expr> Base,
                    std::unique_ptr<FunCallExpr> FunCall);
  ~MemberFunCallExpr() override;

  [[nodiscard]] Expr *getBase() const { return Base.get(); }
  [[nodiscard]] FunCallExpr &getCall() const { return *FunCall; }
  [[nodiscard]] bool isAssignable() const override { return true; }

  void emit(int level) const override;
  bool accept(ASTVisitor<bool> &Visitor) override;
  void accept(ASTVisitor<void> &Visitor) override;
  static bool classof(const Expr *E) {
    return E->getKind() == Stmt::Kind::MemberFunAccessKind;
  }

private:
  std::unique_ptr<Expr> Base;
  std::unique_ptr<FunCallExpr> FunCall;
};

} // namespace phi
