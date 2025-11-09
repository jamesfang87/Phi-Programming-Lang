#pragma once

#include <cstdint>
#include <memory>
#include <optional>
#include <print>
#include <string>
#include <unordered_map>
#include <vector>

#include "AST/Stmt.hpp"
#include "AST/Type.hpp"
#include "SrcManager/SrcLocation.hpp"

namespace phi {

// Forward declarations
class Expr;
class TypeInferencer;
class TypeChecker;
class CodeGen;

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
    StructDecl,
    EnumDecl,
    VariantDecl,
  };

  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  Decl(Kind K, SrcLocation Loc, std::string Id)
      : DeclKind(K), Location(std::move(Loc)), Id(std::move(Id)) {}

  virtual ~Decl() = default;

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] Kind getKind() const { return DeclKind; }
  [[nodiscard]] SrcLocation getLocation() const { return Location; }
  [[nodiscard]] const std::string &getId() const { return Id; }
  [[nodiscard]] virtual Type getType() const = 0;

  //===--------------------------------------------------------------------===//
  // Visitor Methods
  //===--------------------------------------------------------------------===//

  virtual void accept(TypeInferencer &I) = 0;
  virtual bool accept(TypeChecker &C) = 0;
  virtual void accept(CodeGen &G) = 0;

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//

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
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  ValueDecl(Kind K, SrcLocation Loc, std::string Id,
            std::optional<Type> DeclType)
      : Decl(K, std::move(Loc), std::move(Id)), DeclType(std::move(DeclType)) {}

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] Type getType() const override { return *DeclType; }

  //===--------------------------------------------------------------------===//
  // Setters
  //===--------------------------------------------------------------------===//

  void setType(Type T) { DeclType = std::move(T); }

  //===--------------------------------------------------------------------===//
  // Type Queries
  //===--------------------------------------------------------------------===//

  [[nodiscard]] bool hasType() const { return DeclType.has_value(); }
  [[nodiscard]] virtual bool isConst() const = 0;

protected:
  std::optional<Type> DeclType;
};

//===----------------------------------------------------------------------===//
// VarDecl - Variable declaration
//===----------------------------------------------------------------------===//

class VarDecl final : public ValueDecl {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  VarDecl(SrcLocation Loc, std::string Id, std::optional<Type> DeclType,
          bool IsConst, std::unique_ptr<Expr> Init);
  ~VarDecl() override;

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] Expr &getInit() const { return *Init; }

  //===--------------------------------------------------------------------===//
  // Type Queries
  //===--------------------------------------------------------------------===//

  [[nodiscard]] bool isConst() const override { return IsConst; }
  [[nodiscard]] bool hasInit() const { return Init != nullptr; }

  //===--------------------------------------------------------------------===//
  // Visitor Methods
  //===--------------------------------------------------------------------===//

  void accept(TypeInferencer &I) override;
  bool accept(TypeChecker &C) override;
  void accept(CodeGen &G) override;

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===--------------------------------------------------------------------===//

  static bool classof(const Decl *D) { return D->getKind() == Kind::VarDecl; }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//

  void emit(int level) const override;

private:
  bool IsConst;
  std::unique_ptr<Expr> Init;
};

//===----------------------------------------------------------------------===//
// ParamDecl - Function parameter
//===----------------------------------------------------------------------===//

class ParamDecl final : public ValueDecl {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  ParamDecl(SrcLocation Loc, std::string Id, Type DeclType, bool IsConst)
      : ValueDecl(Kind::ParamDecl, std::move(Loc), std::move(Id),
                  std::move(DeclType)),
        IsConst(IsConst) {}

  //===--------------------------------------------------------------------===//
  // Type Queries
  //===--------------------------------------------------------------------===//

  [[nodiscard]] bool isConst() const override { return IsConst; }

  //===--------------------------------------------------------------------===//
  // Visitor Methods
  //===--------------------------------------------------------------------===//

  void accept(TypeInferencer &I) override;
  bool accept(TypeChecker &C) override;
  void accept(CodeGen &G) override;

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===--------------------------------------------------------------------===//

  static bool classof(const Decl *D) { return D->getKind() == Kind::ParamDecl; }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//

  void emit(int level) const override;

private:
  bool IsConst;
};

//===----------------------------------------------------------------------===//
// FieldDecl - Struct/Union field
//===----------------------------------------------------------------------===//

class FieldDecl final : public ValueDecl {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  FieldDecl(SrcLocation Loc, std::string Id, Type DeclType,
            std::unique_ptr<Expr> Init, bool IsPrivate, uint32_t Index);
  ~FieldDecl() override;

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] Expr &getInit() const { return *Init; }
  [[nodiscard]] const class StructDecl *getParent() const { return Parent; }
  [[nodiscard]] uint32_t getIndex() const { return Index; }

  //===--------------------------------------------------------------------===//
  // Setters
  //===--------------------------------------------------------------------===//

  void setParent(StructDecl *P) { Parent = P; }

  //===--------------------------------------------------------------------===//
  // Type Queries
  //===--------------------------------------------------------------------===//

  [[nodiscard]] bool isConst() const override { return false; }
  [[nodiscard]] bool isPrivate() const { return IsPrivate; }
  [[nodiscard]] bool hasInit() const { return Init != nullptr; }

  //===--------------------------------------------------------------------===//
  // Visitor Methods
  //===--------------------------------------------------------------------===//

  void accept(TypeInferencer &I) override;
  bool accept(TypeChecker &C) override;
  void accept(CodeGen &G) override;

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===--------------------------------------------------------------------===//

  static bool classof(const Decl *D) { return D->getKind() == Kind::FieldDecl; }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//

  void emit(int level) const override;

private:
  bool IsPrivate;
  std::unique_ptr<Expr> Init;
  class StructDecl *Parent = nullptr;
  uint32_t Index;
};

//===----------------------------------------------------------------------===//
// FunDecl - Function declaration
//===----------------------------------------------------------------------===//

class FunDecl : public Decl {
protected:
  //===--------------------------------------------------------------------===//
  // Protected Constructor (for subclasses)
  //===--------------------------------------------------------------------===//

  FunDecl(Kind K, SrcLocation Loc, std::string Id, Type ReturnType,
          std::vector<std::unique_ptr<ParamDecl>> Params,
          std::unique_ptr<Block> Body)
      : Decl(K, std::move(Loc), std::move(Id)),
        ReturnType(std::move(ReturnType)), Params(std::move(Params)),
        Body(std::move(Body)) {
    std::vector<Type> ParamTypes;
    ParamTypes.reserve(this->Params.size());
    for (auto &Param : this->Params) {
      ParamTypes.push_back(Param->getType());
    }
    FunType = Type::makeFunction(ParamTypes, this->ReturnType, this->Location);
  }

public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  FunDecl(SrcLocation Loc, std::string Id, Type ReturnType,
          std::vector<std::unique_ptr<ParamDecl>> Params,
          std::unique_ptr<Block> Body)
      : FunDecl(Kind::FunDecl, std::move(Loc), std::move(Id),
                std::move(ReturnType), std::move(Params), std::move(Body)) {
    std::vector<Type> ParamTypes;
    ParamTypes.reserve(this->Params.size());
    for (auto &Param : this->Params) {
      ParamTypes.push_back(Param->getType());
    }
    FunType = Type::makeFunction(ParamTypes, this->ReturnType, this->Location);
  }

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//
  [[nodiscard]] Type getType() const override { return FunType; }
  [[nodiscard]] const Type &getFunType() const { return FunType; }
  [[nodiscard]] const Type &getReturnTy() const { return ReturnType; }
  [[nodiscard]] auto &getParams() { return Params; }
  [[nodiscard]] Block &getBody() const { return *Body; }
  [[nodiscard]] std::unique_ptr<Block> &getBodyPtr() { return Body; }

  //===--------------------------------------------------------------------===//
  // Setters
  //===--------------------------------------------------------------------===//

  void setBody(std::unique_ptr<Block> B) { Body = std::move(B); }

  //===--------------------------------------------------------------------===//
  // Visitor Methods
  //===--------------------------------------------------------------------===//

  void accept(TypeInferencer &I) override;
  bool accept(TypeChecker &C) override;
  void accept(CodeGen &G) override;

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===--------------------------------------------------------------------===//

  static bool classof(const Decl *D) {
    return D->getKind() == Kind::FunDecl || D->getKind() == Kind::MethodDecl;
  }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//

  void emit(int level) const override;

protected:
  Type FunType;
  Type ReturnType;
  std::vector<std::unique_ptr<ParamDecl>> Params;
  std::unique_ptr<Block> Body;
};

//===----------------------------------------------------------------------===//
// MethodDecl - Struct/Enum member function
//===----------------------------------------------------------------------===//

class MethodDecl final : public FunDecl {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  MethodDecl(SrcLocation Loc, std::string Id, Type ReturnType,
             std::vector<std::unique_ptr<ParamDecl>> Params,
             std::unique_ptr<Block> Body, bool IsPrivate)
      : FunDecl(Kind::MethodDecl, std::move(Loc), std::move(Id),
                std::move(ReturnType), std::move(Params), std::move(Body)),
        IsPrivate(IsPrivate) {}

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] const class StructDecl *getParent() const { return Parent; }

  //===--------------------------------------------------------------------===//
  // Setters
  //===--------------------------------------------------------------------===//

  void setParent(StructDecl *P) { Parent = P; }
  void setMangledId(std::string Mangled) { MangledId = std::move(Mangled); }

  //===--------------------------------------------------------------------===//
  // Type Queries
  //===--------------------------------------------------------------------===//

  [[nodiscard]] bool isPrivate() const { return IsPrivate; }
  [[nodiscard]] std::string getMangledId() const { return MangledId; }

  //===--------------------------------------------------------------------===//
  // Visitor Methods
  //===--------------------------------------------------------------------===//

  void accept(TypeInferencer &I) override;
  bool accept(TypeChecker &C) override;
  void accept(CodeGen &G) override;

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===--------------------------------------------------------------------===//

  static bool classof(const Decl *D) {
    return D->getKind() == Kind::MethodDecl;
  }

private:
  class StructDecl *Parent = nullptr;
  std::string MangledId;
  bool IsPrivate;
};

//===----------------------------------------------------------------------===//
// StructDecl - Aggregate type
//===----------------------------------------------------------------------===//

class StructDecl final : public Decl {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  StructDecl(SrcLocation Loc, std::string Id,
             std::vector<std::unique_ptr<FieldDecl>> Fields,
             std::vector<MethodDecl> Methods);

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] Type getType() const override { return DeclType; }
  [[nodiscard]] const auto &getFields() const { return Fields; }
  [[nodiscard]] std::vector<MethodDecl> &getMethods() { return Methods; }

  [[nodiscard]] FieldDecl *getField(const std::string &Id) {
    auto it = FieldMap.find(Id);
    return it != FieldMap.end() ? it->second : nullptr;
  }

  [[nodiscard]] MethodDecl *getMethod(const std::string &Id) {
    auto it = MethodMap.find(Id);
    return it != MethodMap.end() ? it->second : nullptr;
  }

  //===--------------------------------------------------------------------===//
  // Visitor Methods
  //===--------------------------------------------------------------------===//

  void accept(TypeInferencer &I) override;
  bool accept(TypeChecker &C) override;
  void accept(CodeGen &G) override;

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===--------------------------------------------------------------------===//

  static bool classof(const Decl *D) {
    return D->getKind() == Kind::StructDecl;
  }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//

  void emit(int level) const override;

private:
  Type DeclType;
  std::vector<std::unique_ptr<FieldDecl>> Fields;
  std::vector<MethodDecl> Methods;

  std::unordered_map<std::string, FieldDecl *> FieldMap;
  std::unordered_map<std::string, MethodDecl *> MethodMap;
};

class VariantDecl final : public Decl {
public:
  VariantDecl(SrcLocation Loc, std::string Id, std::optional<Type> DeclType);

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] bool hasType() const { return DeclType.has_value(); };
  [[nodiscard]] Type getType() const override { return *DeclType; }
  [[nodiscard]] std::optional<Type> getOptType() const { return DeclType; };

  //===--------------------------------------------------------------------===//
  // Visitor Methods
  //===--------------------------------------------------------------------===//

  void accept(TypeInferencer &I) override;
  bool accept(TypeChecker &C) override;
  void accept(CodeGen &G) override;

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===--------------------------------------------------------------------===//

  static bool classof(const Decl *D) {
    return D->getKind() == Kind::VariantDecl;
  }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//

  void emit(int level) const override;

private:
  std::optional<Type> DeclType;
};

class EnumDecl final : public Decl {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  EnumDecl(SrcLocation Loc, std::string Id, std::vector<VariantDecl> Variants,
           std::vector<MethodDecl> Methods);

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] Type getType() const override { return DeclType; }
  [[nodiscard]] auto &getVariants() { return Variants; }
  [[nodiscard]] auto &getMethods() { return Methods; }

  [[nodiscard]] VariantDecl *getVariant(const std::string &Id) {
    auto It = VariantMap.find(Id);
    return It != VariantMap.end() ? It->second : nullptr;
  }

  //===--------------------------------------------------------------------===//
  // Visitor Methods
  //===--------------------------------------------------------------------===//

  void accept(TypeInferencer &I) override;
  bool accept(TypeChecker &C) override;
  void accept(CodeGen &G) override;

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===--------------------------------------------------------------------===//

  static bool classof(const Decl *D) { return D->getKind() == Kind::EnumDecl; }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//

  void emit(int level) const override;

private:
  Type DeclType;
  std::vector<VariantDecl> Variants;
  std::vector<MethodDecl> Methods;

  std::unordered_map<std::string, VariantDecl *> VariantMap;
};

} // namespace phi
