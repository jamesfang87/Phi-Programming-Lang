#pragma once

#include <cstdint>
#include <memory>
#include <optional>
#include <print>
#include <string>
#include <unordered_map>
#include <utility>
#include <vector>

#include <llvm/Support/Casting.h>

#include "AST/Nodes/Stmt.hpp"
#include "AST/TypeSystem/Context.hpp"
#include "AST/TypeSystem/Type.hpp"
#include "SrcManager/SrcSpan.hpp"

namespace phi {

//===----------------------------------------------------------------------===//
// Forward Declarations
//===----------------------------------------------------------------------===//

class Expr;

//===----------------------------------------------------------------------===//
// Enums
//===----------------------------------------------------------------------===//

enum class Visibility {
  Private,
  Public,
};

enum class Mutability {
  Const,
  Var,
};

//===----------------------------------------------------------------------===//
// Decl - Root of all declarations
//===----------------------------------------------------------------------===//

class Decl {
public:
  enum class Kind : uint8_t {
    Module,
    Fun,
    Struct,
    Enum,
    Field,
    Method,
    Variant,
    Var,
    Param,
    TypeArg,
  };

  //===--------------------------------------------------------------------===//
  // Constructors & Destructor
  //===--------------------------------------------------------------------===//
  Decl(Kind K, SrcSpan Span) : K(K), Span(std::move(Span)) {}
  virtual ~Decl() = default;

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//
  [[nodiscard]] Kind getKind() const { return K; }
  [[nodiscard]] SrcSpan getSpan() const { return Span; }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//
  virtual void emit(int Level) const = 0;

private:
  Kind K;
  SrcSpan Span;
};

//===----------------------------------------------------------------------===//
// NamedDecl - Decl that introduces a name
//===----------------------------------------------------------------------===//

class NamedDecl : public Decl {
protected:
  //===--------------------------------------------------------------------===//
  // Constructors
  //===--------------------------------------------------------------------===//
  NamedDecl(Kind K, SrcSpan Span, std::string Id)
      : Decl(K, Span), Id(std::move(Id)) {}

public:
  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//
  [[nodiscard]] const std::string &getId() const { return Id; }

private:
  const std::string Id;
};

class TypeArgDecl : public NamedDecl {
public:
  TypeArgDecl(SrcSpan Span, std::string Id)
      : NamedDecl(Decl::Kind::TypeArg, std::move(Span), std::move(Id)) {}

  static bool classof(const Decl *D) { return D->getKind() == Kind::TypeArg; }

  void emit(int Level) const override;
};

//===----------------------------------------------------------------------===//
// ItemDecl - Top-level declarations
//===----------------------------------------------------------------------===//

class ItemDecl : public NamedDecl {
protected:
  //===--------------------------------------------------------------------===//
  // Constructors
  //===--------------------------------------------------------------------===//
  ItemDecl(Kind K, SrcSpan Span, Visibility Vis, std::string Id,
           std::vector<std::unique_ptr<TypeArgDecl>> TypeArgs)
      : NamedDecl(K, Span, std::move(Id)), TheVisibility(Vis),
        TypeArgs(std::move(TypeArgs)) {}

public:
  //===--------------------------------------------------------------------===//
  // Getters & Setters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] auto hasTypeArgs() const { return !TypeArgs.empty(); }
  [[nodiscard]] auto &getTypeArgs() const { return TypeArgs; }
  [[nodiscard]] Visibility getVisibility() const { return TheVisibility; }
  void setVisibility(Visibility Vis) { TheVisibility = Vis; }

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===--------------------------------------------------------------------===//
  static bool classof(const Decl *D) {
    switch (D->getKind()) {
    case Kind::Module:
    case Kind::Fun:
    case Kind::Struct:
    case Kind::Enum:
      return true;
    default:
      return false;
    }
  }

private:
  Visibility TheVisibility;
  std::vector<std::unique_ptr<TypeArgDecl>> TypeArgs;
};

//===----------------------------------------------------------------------===//
// LocalDecl - Decl inside function/block
//===----------------------------------------------------------------------===//

class LocalDecl : public NamedDecl {
protected:
  //===--------------------------------------------------------------------===//
  // Constructors
  //===--------------------------------------------------------------------===//
  LocalDecl(Kind K, SrcSpan Span, std::string Id, TypeRef Type)
      : NamedDecl(K, Span, std::move(Id)), Type(Type) {}

public:
  [[nodiscard]] auto getType() const { return Type; }
  [[nodiscard]] auto getMutability() const { return TheMutability; }
  [[nodiscard]] auto isConst() const {
    return TheMutability == Mutability::Const;
  }

  static bool classof(const Decl *D) {
    return D->getKind() == Kind::Param || D->getKind() == Kind::Var;
  }

protected:
  TypeRef Type;
  Mutability TheMutability;
};

//===----------------------------------------------------------------------===//
// ParamDecl
//===----------------------------------------------------------------------===//

class ParamDecl final : public LocalDecl {
public:
  //===--------------------------------------------------------------------===//
  // Constructors
  //===--------------------------------------------------------------------===//
  ParamDecl(SrcSpan Span, Mutability TheMutability, std::string Id,
            TypeRef Type);

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===--------------------------------------------------------------------===//
  static bool classof(const Decl *D) { return D->getKind() == Kind::Param; }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//
  void emit(int Level) const override;
};

//===----------------------------------------------------------------------===//
// VarDecl
//===----------------------------------------------------------------------===//

class VarDecl final : public LocalDecl {
public:
  //===--------------------------------------------------------------------===//
  // Constructors
  //===--------------------------------------------------------------------===//
  VarDecl(SrcSpan Span, Mutability TheMutability, std::string Id,
          std::optional<TypeRef> Type, std::unique_ptr<Expr> Init);

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//
  [[nodiscard]] auto hasType() const { return !Type.isVar(); }
  [[nodiscard]] auto hasInit() const { return Init != nullptr; }
  [[nodiscard]] auto &getInit() const { return *Init; }

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//
  void setType(TypeRef T) {
    assert((Type.isVar() || T.getPtr() == Type.getPtr()) &&
           "Cannot change the type of a variable from concrete type");
    Type = T;
  }

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===--------------------------------------------------------------------===//
  static bool classof(const Decl *D) { return D->getKind() == Kind::Var; }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//
  void emit(int Level) const override;

private:
  std::unique_ptr<Expr> Init;
};

//===----------------------------------------------------------------------===//
// MemberDecl
//===----------------------------------------------------------------------===//

class MemberDecl : public NamedDecl {
protected:
  //===--------------------------------------------------------------------===//
  // Constructors
  //===--------------------------------------------------------------------===//
  MemberDecl(Kind K, SrcSpan Span, Visibility Vis, std::string Id)
      : NamedDecl(K, Span, std::move(Id)), TheVisibility(Vis) {}

public:
  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//
  [[nodiscard]] auto getVisibility() const { return TheVisibility; }
  [[nodiscard]] auto *getParent() const { return Parent; }

  //===--------------------------------------------------------------------===//
  // Setters
  //===--------------------------------------------------------------------===//
  void setParent(AdtDecl *D) { Parent = D; }

private:
  AdtDecl *Parent = nullptr;
  Visibility TheVisibility;
};

//===----------------------------------------------------------------------===//
// FieldDecl
//===----------------------------------------------------------------------===//

class FieldDecl final : public MemberDecl {
public:
  //===--------------------------------------------------------------------===//
  // Constructors
  //===--------------------------------------------------------------------===//
  FieldDecl(SrcSpan Span, uint32_t Index, Visibility Vis, std::string Id,
            TypeRef Type, std::unique_ptr<Expr> Init);

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//
  [[nodiscard]] auto getIndex() const { return Index; }
  [[nodiscard]] auto getType() const { return Type; }
  [[nodiscard]] auto hasInit() const { return Init != nullptr; }
  [[nodiscard]] auto &getInit() const { return *Init; }

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===--------------------------------------------------------------------===//
  static bool classof(const Decl *D) { return D->getKind() == Kind::Field; }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//
  void emit(int Level) const override;

private:
  uint32_t Index;

  TypeRef Type;
  std::unique_ptr<Expr> Init;
};

//===----------------------------------------------------------------------===//
// MethodDecl
//===----------------------------------------------------------------------===//

class MethodDecl final : public MemberDecl {
public:
  //===--------------------------------------------------------------------===//
  // Constructors
  //===--------------------------------------------------------------------===//
  MethodDecl(SrcSpan Span, Visibility Vis, std::string Id,
             std::vector<std::unique_ptr<TypeArgDecl>> TypeArgs,
             std::vector<std::unique_ptr<ParamDecl>> Params, TypeRef ReturnType,
             std::unique_ptr<Block> Body);

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//
  [[nodiscard]] auto hasTypeArgs() const { return !TypeArgs.empty(); }
  [[nodiscard]] auto &getTypeArgs() const { return TypeArgs; }
  [[nodiscard]] auto &getParams() const { return Params; }
  [[nodiscard]] auto &getReturnType() const { return ReturnType; }
  [[nodiscard]] auto &getBody() const { return *Body; }

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===--------------------------------------------------------------------===//
  static bool classof(const Decl *D) { return D->getKind() == Kind::Method; }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//
  void emit(int Level) const override;

private:
  std::vector<std::unique_ptr<TypeArgDecl>> TypeArgs;
  std::vector<std::unique_ptr<ParamDecl>> Params;
  TypeRef ReturnType;
  std::unique_ptr<Block> Body;
};

//===----------------------------------------------------------------------===//
// VariantDecl
//===----------------------------------------------------------------------===//

class VariantDecl final : public MemberDecl {
public:
  //===--------------------------------------------------------------------===//
  // Constructors
  //===--------------------------------------------------------------------===//
  VariantDecl(SrcSpan Span, std::string Id, std::optional<TypeRef> PayloadType);

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//
  [[nodiscard]] auto hasPayload() const { return PayloadType.has_value(); }
  [[nodiscard]] auto getPayloadType() const { return *PayloadType; }

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===--------------------------------------------------------------------===//
  static bool classof(const Decl *D) { return D->getKind() == Kind::Variant; }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//
  void emit(int Level) const override;

private:
  std::optional<TypeRef> PayloadType;
};

//===----------------------------------------------------------------------===//
// AdtDecl
//===----------------------------------------------------------------------===//

class AdtDecl : public ItemDecl {
protected:
  //===--------------------------------------------------------------------===//
  // Constructors
  //===--------------------------------------------------------------------===//
  AdtDecl(Kind K, SrcSpan Span, Visibility Vis, std::string Id,
          std::vector<std::unique_ptr<TypeArgDecl>> TypeArgs,
          std::vector<std::unique_ptr<MethodDecl>> Methods)
      : ItemDecl(K, Span, Vis, Id, std::move(TypeArgs)),
        Type(TypeCtx::getAdt(std::move(Id), this, Span)),
        Methods(std::move(Methods)) {
    for (auto &M : this->Methods) {
      MethodMap.emplace(M->getId(), M.get());
      M->setParent(this);
    }

    if (hasTypeArgs()) {
      std::vector<TypeRef> TArgs;
      TArgs.reserve(getTypeArgs().size());
      for (auto &Arg : getTypeArgs()) {
        TArgs.push_back(
            TypeCtx::getGeneric(Arg->getId(), Arg.get(), Arg->getSpan()));
      }
      Type = TypeCtx::getApplied(Type, TArgs, this->getSpan());
    }
  }

public:
  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//
  [[nodiscard]] auto &getType() const { return Type; }
  [[nodiscard]] auto &getMethods() const { return Methods; }
  [[nodiscard]] auto *getMethod(const std::string &Id) const {
    auto It = MethodMap.find(Id);
    return It != MethodMap.end() ? It->second : nullptr;
  }

  static bool classof(const Decl *D) {
    return D->getKind() == Kind::Struct || D->getKind() == Kind::Enum;
  }

protected:
  TypeRef Type;
  std::vector<std::unique_ptr<MethodDecl>> Methods;
  std::unordered_map<std::string, MethodDecl *> MethodMap;
};

//===----------------------------------------------------------------------===//
// StructDecl
//===----------------------------------------------------------------------===//

class StructDecl final : public AdtDecl {
public:
  //===--------------------------------------------------------------------===//
  // Constructors
  //===--------------------------------------------------------------------===//
  StructDecl(SrcSpan Span, Visibility Vis, std::string Id,
             std::vector<std::unique_ptr<TypeArgDecl>> TypeArgs,
             std::vector<std::unique_ptr<FieldDecl>> Fields,
             std::vector<std::unique_ptr<MethodDecl>> Methods);

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//
  auto &getFields() const { return Fields; }
  auto *getField(const std::string &Id) const {
    auto It = FieldMap.find(Id);
    return It == FieldMap.end() ? nullptr : It->second;
  }

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===--------------------------------------------------------------------===//
  static bool classof(const Decl *D) { return D->getKind() == Kind::Struct; }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//
  void emit(int Level) const override;

private:
  std::vector<std::unique_ptr<FieldDecl>> Fields;
  std::unordered_map<std::string, FieldDecl *> FieldMap;
};

//===----------------------------------------------------------------------===//
// EnumDecl
//===----------------------------------------------------------------------===//

class EnumDecl final : public AdtDecl {
public:
  //===--------------------------------------------------------------------===//
  // Constructors
  //===--------------------------------------------------------------------===//
  EnumDecl(SrcSpan Span, Visibility Vis, std::string Id,
           std::vector<std::unique_ptr<TypeArgDecl>> TypeArgs,
           std::vector<std::unique_ptr<VariantDecl>> Variants,
           std::vector<std::unique_ptr<MethodDecl>> Methods);

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//
  auto &getVariants() const { return Variants; }
  auto *getVariant(const std::string &Id) const {
    auto It = VariantMap.find(Id);
    return It == VariantMap.end() ? nullptr : It->second;
  }

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===--------------------------------------------------------------------===//
  static bool classof(const Decl *D) { return D->getKind() == Kind::Enum; }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//
  void emit(int Level) const override;

private:
  std::vector<std::unique_ptr<VariantDecl>> Variants;
  std::unordered_map<std::string, VariantDecl *> VariantMap;
};

//===----------------------------------------------------------------------===//
// FunDecl
//===----------------------------------------------------------------------===//

class FunDecl final : public ItemDecl {
public:
  //===--------------------------------------------------------------------===//
  // Constructors
  //===--------------------------------------------------------------------===//
  FunDecl(SrcSpan Span, Visibility Vis, std::string Id,
          std::vector<std::unique_ptr<TypeArgDecl>> TypeArgs,
          std::vector<std::unique_ptr<ParamDecl>> Params, TypeRef ReturnType,
          std::unique_ptr<Block> Body);

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//
  [[nodiscard]] auto &getParams() const { return Params; }
  [[nodiscard]] auto &getReturnType() const { return ReturnType; }
  [[nodiscard]] auto &getBody() const { return *Body; }

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===--------------------------------------------------------------------===//
  static bool classof(const Decl *D) { return D->getKind() == Kind::Fun; }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//
  void emit(int Level) const override;

private:
  std::vector<std::unique_ptr<ParamDecl>> Params;
  TypeRef ReturnType;
  std::unique_ptr<Block> Body;
};

//===----------------------------------------------------------------------===//
// ModuleDecl
//===----------------------------------------------------------------------===//

class ModuleDecl final : public ItemDecl {
public:
  //===--------------------------------------------------------------------===//
  // Constructors
  //===--------------------------------------------------------------------===//
  ModuleDecl(SrcSpan PathSpan, Visibility Vis, std::string Id,
             std::vector<std::string> Path,
             std::vector<std::unique_ptr<ItemDecl>> Items,
             std::vector<std::unique_ptr<ImportStmt>> Imports);

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//
  const auto &getPath() const { return Path; }
  auto &getItems() { return Items; }
  auto &getPublicItems() { return PublicItems; }
  auto &getImports() { return Imports; }
  auto contains(const ItemDecl *Query) {
    for (auto &Item : Items) {
      if (Query == Item.get()) {
        return true;
      }
    }
    return false;
  }

  //===--------------------------------------------------------------------===//
  // Setters
  //===--------------------------------------------------------------------===//
  void addFrom(ModuleDecl &&Other) {
    for (auto &Decl : Other.getItems()) {
      Items.push_back(std::move(Decl));
    }
    Other.getItems().clear();
  }

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===--------------------------------------------------------------------===//
  static bool classof(const Decl *D) { return D->getKind() == Kind::Module; }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//
  void emit(int Level) const override;

private:
  std::vector<std::string> Path;
  std::vector<std::unique_ptr<ItemDecl>> Items;
  std::vector<ItemDecl *> PublicItems;
  std::vector<ImportStmt> Imports;
};

} // namespace phi
