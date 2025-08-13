#pragma once

#include <memory>
#include <optional>
#include <print>
#include <string>
#include <unordered_map>
#include <vector>

#include "AST/Expr.hpp"
#include "AST/Stmt.hpp"
#include "AST/Type.hpp"
#include "SrcManager/SrcLocation.hpp"

namespace phi {

//===----------------------------------------------------------------------===//
// Decl - Base class for all declarations
//===----------------------------------------------------------------------===//
class Decl {
public:
  enum class Kind : uint8_t {
    VarDecl,
    ParamDecl,
    FieldDecl,
    FunDecl,
    MethodDecl,
    StructDecl
  };

  Decl(Kind K, SrcLocation Loc, std::string Id)
      : DeclKind(K), Location(std::move(Loc)), Id(std::move(Id)) {}

  virtual ~Decl() = default;

  [[nodiscard]] Kind getKind() const { return DeclKind; }
  [[nodiscard]] SrcLocation getLocation() const { return Location; }
  [[nodiscard]] const std::string &getId() const { return Id; }

  virtual void emit(int level) const = 0;

protected:
  Kind DeclKind;
  SrcLocation Location;
  std::string Id;
};

//===----------------------------------------------------------------------===//
// ValueDecl - Base for declarations that have a type and can appear in exprs
//===----------------------------------------------------------------------===//
class ValueDecl : public Decl {
public:
  ValueDecl(Kind K, SrcLocation Loc, std::string Id, Type DeclType)
      : Decl(K, std::move(Loc), std::move(Id)), DeclType(std::move(DeclType)) {}

  [[nodiscard]] bool hasType() const { return DeclType.has_value(); }
  [[nodiscard]] Type getType() const { return *DeclType; }
  [[nodiscard]] virtual bool isConst() const = 0;

protected:
  std::optional<Type> DeclType;
};

//===----------------------------------------------------------------------===//
// VarDecl - Variable declaration
//===----------------------------------------------------------------------===//
class VarDecl final : public ValueDecl {
public:
  VarDecl(SrcLocation Loc, std::string Id, Type DeclType, bool IsConst,
          std::unique_ptr<Expr> Init)
      : ValueDecl(Kind::VarDecl, std::move(Loc), std::move(Id),
                  std::move(DeclType)),
        IsConstFlag(IsConst), Init(std::move(Init)) {}

  [[nodiscard]] bool isConst() const override { return IsConstFlag; }
  [[nodiscard]] bool hasInit() const { return Init != nullptr; }
  [[nodiscard]] Expr &getInit() const { return *Init; }

  static bool classof(const Decl *D) { return D->getKind() == Kind::VarDecl; }

  void emit(int level) const override;

private:
  bool IsConstFlag;
  std::unique_ptr<Expr> Init;
};

//===----------------------------------------------------------------------===//
// ParamDecl - Function parameter
//===----------------------------------------------------------------------===//
class ParamDecl final : public ValueDecl {
public:
  ParamDecl(SrcLocation Loc, std::string Id, Type DeclType, bool IsConst)
      : ValueDecl(Kind::ParamDecl, std::move(Loc), std::move(Id),
                  std::move(DeclType)),
        IsConstFlag(IsConst) {}

  [[nodiscard]] bool isConst() const override { return IsConstFlag; }

  static bool classof(const Decl *D) { return D->getKind() == Kind::ParamDecl; }

  void emit(int level) const override;

private:
  bool IsConstFlag;
};

//===----------------------------------------------------------------------===//
// FieldDecl - Struct/Union field
//===----------------------------------------------------------------------===//
class FieldDecl final : public ValueDecl {
public:
  FieldDecl(SrcLocation Loc, std::string Id, Type DeclType,
            std::unique_ptr<Expr> Init, bool IsPrivate)
      : ValueDecl(Kind::FieldDecl, std::move(Loc), std::move(Id),
                  std::move(DeclType)),
        IsPrivateFlag(IsPrivate), Init(std::move(Init)) {}

  [[nodiscard]] bool isConst() const override { return false; }
  [[nodiscard]] bool isPrivate() const { return IsPrivateFlag; }
  [[nodiscard]] bool hasInit() const { return Init != nullptr; }
  [[nodiscard]] Expr &getInit() const { return *Init; }

  static bool classof(const Decl *D) { return D->getKind() == Kind::FieldDecl; }

  void emit(int level) const override;

private:
  bool IsPrivateFlag;
  std::unique_ptr<Expr> Init;
};

//===----------------------------------------------------------------------===//
// FunDecl - Function declaration
//===----------------------------------------------------------------------===//
class FunDecl : public Decl {
public:
  FunDecl(Kind K, SrcLocation Loc, std::string Id, Type ReturnType,
          std::vector<std::unique_ptr<ParamDecl>> Params,
          std::unique_ptr<Block> Body)
      : Decl(K, std::move(Loc), std::move(Id)),
        ReturnType(std::move(ReturnType)), Params(std::move(Params)),
        Body(std::move(Body)) {}

  [[nodiscard]] const Type &getReturnTy() const { return ReturnType; }
  [[nodiscard]] std::vector<std::unique_ptr<ParamDecl>> &getParams() {
    return Params;
  }
  [[nodiscard]] Block &getBody() const { return *Body; }
  [[nodiscard]] std::unique_ptr<Block> &getBodyPtr() { return Body; }

  void setBody(std::unique_ptr<Block> NewBody) { Body = std::move(NewBody); }

  static bool classof(const Decl *D) {
    return D->getKind() == Kind::FunDecl || D->getKind() == Kind::MethodDecl;
  }

  void emit(int level) const override;

protected:
  Type ReturnType;
  std::vector<std::unique_ptr<ParamDecl>> Params;
  std::unique_ptr<Block> Body;
};

//===----------------------------------------------------------------------===//
// MethodDecl - Struct/Enum member function
//===----------------------------------------------------------------------===//
class MethodDecl final : public FunDecl {
public:
  MethodDecl(SrcLocation Loc, std::string Id, Type ReturnType,
             std::vector<std::unique_ptr<ParamDecl>> Params,
             std::unique_ptr<Block> Body, bool IsPrivate)
      : FunDecl(Kind::MethodDecl, std::move(Loc), std::move(Id),
                std::move(ReturnType), std::move(Params), std::move(Body)),
        IsPrivate(IsPrivate) {}

  MethodDecl(FunDecl &&FD, bool IsConst)
      : FunDecl(Kind::MethodDecl, FD.getLocation(), FD.getId(),
                FD.getReturnTy(), std::move(FD.getParams()),
                std::move(FD.getBodyPtr())),
        IsPrivate(IsConst) {}

  [[nodiscard]] bool isPrivate() const { return IsPrivate; }

  static bool classof(const Decl *D) {
    return D->getKind() == Kind::MethodDecl;
  }

private:
  bool IsPrivate; // For "self const" methods
};

//===----------------------------------------------------------------------===//
// StructDecl - Aggregate type
//===----------------------------------------------------------------------===//
class StructDecl final : public Decl {
public:
  StructDecl(SrcLocation Loc, std::string Id, std::vector<FieldDecl> Fields,
             std::vector<MethodDecl> Methods)
      : Decl(Kind::StructDecl, std::move(Loc), Id), Ty(std::move(Id)),
        Fields(std::move(Fields)), Methods(std::move(Methods)) {
    for (auto &field : this->Fields) {
      FieldMap[field.getId()] = &field;
    }
    for (auto &method : this->Methods) {
      MethodMap[method.getId()] = &method;
    }
  }

  [[nodiscard]] Type getType() const { return Ty; }
  [[nodiscard]] std::vector<FieldDecl> &getFields() { return Fields; }
  [[nodiscard]] std::vector<MethodDecl> &getMethods() { return Methods; }

  [[nodiscard]] FieldDecl *getField(const std::string &Id) {
    auto it = FieldMap.find(Id);
    return it != FieldMap.end() ? it->second : nullptr;
  }

  [[nodiscard]] MethodDecl *getMethod(const std::string &Id) {
    auto it = MethodMap.find(Id);
    return it != MethodMap.end() ? it->second : nullptr;
  }

  static bool classof(const Decl *D) {
    return D->getKind() == Kind::StructDecl;
  }

  void emit(int level) const override;

private:
  Type Ty;
  std::vector<FieldDecl> Fields;
  std::vector<MethodDecl> Methods;

  std::unordered_map<std::string, FieldDecl *> FieldMap;
  std::unordered_map<std::string, MethodDecl *> MethodMap;
};

} // namespace phi
