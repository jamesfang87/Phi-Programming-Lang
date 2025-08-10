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
 * @brief Base class for all expression nodes
 */
class Expr : public Stmt {
public:
  explicit Expr(Kind K, SrcLocation location,
                std::optional<Type> type = std::nullopt)
      : Stmt(K, std::move(location)), type(std::move(type)) {}

  [[nodiscard]] Type getType() { return type.value(); }
  [[nodiscard]] bool isResolved() const { return type.has_value(); }
  void setType(Type t) { type = std::move(t); }
  [[nodiscard]] virtual bool isAssignable() const = 0;

  bool accept(ASTVisitor<bool> &visitor) override;
  void accept(ASTVisitor<void> &visitor) override;

  static bool classof(const Stmt *S) {
    return S->getKind() >= Kind::ExprFirst && S->getKind() <= Kind::ExprLast;
  }

protected:
  std::optional<Type> type;
};

class IntLiteral final : public Expr {
public:
  IntLiteral(SrcLocation location, const int64_t value);
  [[nodiscard]] int64_t getValue() const { return Value; }
  [[nodiscard]] bool isAssignable() const override { return false; }
  void emit(int level) const override;
  bool accept(ASTVisitor<bool> &visitor) override;
  void accept(ASTVisitor<void> &visitor) override;
  static bool classof(const Stmt *S) {
    return S->getKind() == Kind::IntLiteralKind;
  }

private:
  int64_t Value;
};

class FloatLiteral final : public Expr {
public:
  FloatLiteral(SrcLocation location, const double value);
  [[nodiscard]] double getValue() const { return Value; }
  [[nodiscard]] bool isAssignable() const override { return false; }
  void emit(int level) const override;
  bool accept(ASTVisitor<bool> &visitor) override;
  void accept(ASTVisitor<void> &visitor) override;
  static bool classof(const Stmt *S) {
    return S->getKind() == Kind::FloatLiteralKind;
  }

private:
  double Value;
};

class StrLiteral final : public Expr {
public:
  StrLiteral(SrcLocation location, std::string value);
  [[nodiscard]] std::string getValue() const { return Value; }
  [[nodiscard]] bool isAssignable() const override { return false; }
  void emit(int level) const override;
  bool accept(ASTVisitor<bool> &visitor) override;
  void accept(ASTVisitor<void> &visitor) override;
  static bool classof(const Stmt *S) {
    return S->getKind() == Kind::StrLiteralKind;
  }

private:
  std::string Value;
};

class CharLiteral final : public Expr {
public:
  CharLiteral(SrcLocation location, const char value);
  [[nodiscard]] char getValue() const { return Value; }
  [[nodiscard]] bool isAssignable() const override { return false; }
  void emit(int level) const override;
  bool accept(ASTVisitor<bool> &visitor) override;
  void accept(ASTVisitor<void> &visitor) override;
  static bool classof(const Stmt *S) {
    return S->getKind() == Kind::CharLiteralKind;
  }

private:
  char Value;
};

class BoolLiteral final : public Expr {
public:
  BoolLiteral(SrcLocation location, bool value);
  [[nodiscard]] bool getValue() const { return Value; }
  [[nodiscard]] bool isAssignable() const override { return false; }
  void emit(int level) const override;
  bool accept(ASTVisitor<bool> &visitor) override;
  void accept(ASTVisitor<void> &visitor) override;
  static bool classof(const Stmt *S) {
    return S->getKind() == Kind::BoolLiteralKind;
  }

private:
  bool Value;
};

class RangeLiteral final : public Expr {
public:
  RangeLiteral(SrcLocation location, std::unique_ptr<Expr> start,
               std::unique_ptr<Expr> end, const bool inclusive);
  ~RangeLiteral() override; // Need destructor

  [[nodiscard]] Expr &getStart() { return *Start; }
  [[nodiscard]] Expr &getEnd() { return *End; }
  [[nodiscard]] bool isInclusive() const { return Inclusive; }
  [[nodiscard]] bool isAssignable() const override { return false; }
  void emit(int level) const override;
  bool accept(ASTVisitor<bool> &visitor) override;
  void accept(ASTVisitor<void> &visitor) override;
  static bool classof(const Stmt *S) {
    return S->getKind() == Kind::RangeLiteralKind;
  }

private:
  std::unique_ptr<Expr> Start, End;
  bool Inclusive;
};

class DeclRefExpr final : public Expr {
public:
  DeclRefExpr(SrcLocation location, std::string id);
  [[nodiscard]] std::string getId() { return Id; }
  [[nodiscard]] Decl *getDecl() const { return decl; }
  void setDecl(Decl *d) { decl = d; }
  [[nodiscard]] bool isAssignable() const override { return true; }
  void emit(int level) const override;
  bool accept(ASTVisitor<bool> &visitor) override;
  void accept(ASTVisitor<void> &visitor) override;
  static bool classof(const Expr *expr) {
    return expr->getKind() == Stmt::Kind::DeclRefExprKind;
  }

private:
  std::string Id;
  Decl *decl = nullptr;
};

class FunCallExpr final : public Expr {
public:
  FunCallExpr(SrcLocation location, std::unique_ptr<Expr> callee,
              std::vector<std::unique_ptr<Expr>> args);
  ~FunCallExpr() override; // Need destructor

  [[nodiscard]] Expr &getCallee() const { return *Callee; }
  [[nodiscard]] std::vector<std::unique_ptr<Expr>> &getArgs() { return Args; }
  [[nodiscard]] FunDecl *getDecl() const { return decl; }
  [[nodiscard]] bool isAssignable() const override { return false; }
  void setDecl(FunDecl *f) { decl = f; }
  void emit(int level) const override;
  bool accept(ASTVisitor<bool> &visitor) override;
  void accept(ASTVisitor<void> &visitor) override;
  static bool classof(const Expr *expr) {
    return expr->getKind() == Stmt::Kind::FunCallExprKind;
  }

private:
  std::unique_ptr<Expr> Callee;
  std::vector<std::unique_ptr<Expr>> Args;
  FunDecl *decl = nullptr;
};

class BinaryOp final : public Expr {
public:
  BinaryOp(std::unique_ptr<Expr> lhs, std::unique_ptr<Expr> rhs,
           const Token &op);
  ~BinaryOp() override; // Need destructor

  [[nodiscard]] Expr &getLhs() const { return *Lhs; }
  [[nodiscard]] Expr &getRhs() const { return *Rhs; }
  [[nodiscard]] bool isAssignable() const override { return false; }
  [[nodiscard]] TokenKind getOp() const { return Op; }
  void emit(int level) const override;
  bool accept(ASTVisitor<bool> &visitor) override;
  void accept(ASTVisitor<void> &visitor) override;
  static bool classof(const Expr *expr) {
    return expr->getKind() == Stmt::Kind::BinaryOpKind;
  }

private:
  std::unique_ptr<Expr> Lhs;
  std::unique_ptr<Expr> Rhs;
  TokenKind Op;
};

class UnaryOp final : public Expr {
public:
  UnaryOp(std::unique_ptr<Expr> operand, const Token &op, const bool isPrefix);
  ~UnaryOp() override; // Need destructor

  [[nodiscard]] Expr &getOperand() const { return *Operand; }
  [[nodiscard]] TokenKind getOp() const { return Op; }
  [[nodiscard]] bool isPrefixOp() const { return IsPrefix; }
  [[nodiscard]] bool isAssignable() const override { return false; }
  void emit(int level) const override;
  bool accept(ASTVisitor<bool> &visitor) override;
  void accept(ASTVisitor<void> &visitor) override;
  static bool classof(const Expr *expr) {
    return expr->getKind() == Stmt::Kind::UnaryOpKind;
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
  bool accept(ASTVisitor<bool> &visitor) override;
  void accept(ASTVisitor<void> &visitor) override;
  static bool classof(const Expr *expr) {
    return expr->getKind() == Stmt::Kind::FieldInitKind;
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
  bool accept(ASTVisitor<bool> &visitor) override;
  void accept(ASTVisitor<void> &visitor) override;
  static bool classof(const Expr *expr) {
    return expr->getKind() == Stmt::Kind::StructInitKind;
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
  bool accept(ASTVisitor<bool> &visitor) override;
  void accept(ASTVisitor<void> &visitor) override;
  static bool classof(const Expr *expr) {
    return expr->getKind() == Stmt::Kind::MemberAccessKind;
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
  bool accept(ASTVisitor<bool> &visitor) override;
  void accept(ASTVisitor<void> &visitor) override;
  static bool classof(const Expr *expr) {
    return expr->getKind() == Stmt::Kind::MemberFunAccessKind;
  }

private:
  std::unique_ptr<Expr> Base;
  std::unique_ptr<FunCallExpr> FunCall;
};

} // namespace phi
