#pragma once

#include <cstdint>
#include <memory>
#include <optional>
#include <string>
#include <unordered_map>
#include <utility>
#include <vector>

#include "AST/Nodes/Stmt.hpp"
#include "AST/TypeSystem/Context.hpp"
#include "AST/TypeSystem/Type.hpp"
#include "SrcManager/SrcLocation.hpp"

namespace phi {

// Forward declarations
class Expr;

class MethodDecl;
class StructDecl;
class EnumDecl;
class VariantDecl;

//===----------------------------------------------------------------------===//
// Decl - Base class for all declarations
//===----------------------------------------------------------------------===//

class Decl {
public:
  enum class Kind : uint8_t {
    ValueDecl,
    AdtDecl,
    FunDecl,
    VarDecl,
    ParamDecl,
    FieldDecl,
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
  [[nodiscard]] SrcSpan getSpan() const { return SrcSpan(Location); }
  [[nodiscard]] const std::string &getId() const { return Id; }
  [[nodiscard]] virtual TypeRef getType() const = 0;

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//

  virtual void emit(int level) const = 0;

protected:
  Kind DeclKind;
  const SrcLocation Location;
  const std::string Id;
};

//===----------------------------------------------------------------------===//
// ValueDecl - Base for declarations that have a type and can appear in exprs
//===----------------------------------------------------------------------===//

class ValueDecl : public Decl {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  ValueDecl(Kind K, SrcLocation Loc, std::string Id, std::optional<TypeRef> T)
      : Decl(K, std::move(Loc), std::move(Id)),
        DeclType((T.has_value())
                     ? std::move(*T)
                     : TypeCtx::getVar(VarTy::Domain::Any, SrcSpan(Location))) {
  }

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] TypeRef getType() const override { return DeclType; }

  //===--------------------------------------------------------------------===//
  // Setters
  //===--------------------------------------------------------------------===//

  void setType(TypeRef T) { DeclType = std::move(T); }

  //===--------------------------------------------------------------------===//
  // Type Queries
  //===--------------------------------------------------------------------===//

  [[nodiscard]] bool hasType() const { return !DeclType.isVar(); }
  [[nodiscard]] virtual bool isConst() const = 0;

protected:
  TypeRef DeclType;
};

class AdtDecl : public Decl {
protected:
  AdtDecl(Kind K, SrcLocation Loc, std::string Id, TypeRef T,
          std::vector<MethodDecl> Methods);

public:
  [[nodiscard]] TypeRef getType() const override { return DeclType; }

  [[nodiscard]] auto &getMethods() { return Methods; }

  [[nodiscard]] MethodDecl *getMethod(const std::string &Id) const {
    auto It = MethodMap.find(Id);
    return It != MethodMap.end() ? It->second : nullptr;
  }

  static bool classof(const Decl *D) {
    return D->getKind() == Kind::StructDecl || D->getKind() == Kind::EnumDecl;
  }

protected:
  TypeRef DeclType;
  std::vector<MethodDecl> Methods;
  std::unordered_map<std::string, MethodDecl *> MethodMap;
};

//===----------------------------------------------------------------------===//
// VarDecl - Variable declaration
//===----------------------------------------------------------------------===//

class VarDecl final : public ValueDecl {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  VarDecl(SrcLocation Loc, std::string Id, std::optional<TypeRef> T,
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

  ParamDecl(SrcLocation Loc, std::string Id, TypeRef T, bool IsConst)
      : ValueDecl(Kind::ParamDecl, std::move(Loc), std::move(Id), std::move(T)),
        IsConst(IsConst) {}

  //===--------------------------------------------------------------------===//
  // Type Queries
  //===--------------------------------------------------------------------===//

  [[nodiscard]] bool isConst() const override { return IsConst; }

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===--------------------------------------------------------------------===//

  static bool classof(const Decl *D) { return D->getKind() == Kind::ParamDecl; }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//

  void emit(int Level) const override;

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

  FieldDecl(SrcLocation Loc, std::string Id, TypeRef T,
            std::unique_ptr<Expr> Init, bool IsPrivate, uint32_t Index);

  FieldDecl(FieldDecl &&) noexcept = default;

  FieldDecl(const FieldDecl &) = delete;
  FieldDecl &operator=(const FieldDecl &) = delete;

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
  // Members
  //===--------------------------------------------------------------------===//
  TypeRef ReturnType;                             // must be before FunType
  std::vector<std::unique_ptr<ParamDecl>> Params; // must be before FunType
  std::unique_ptr<Block> Body;
  TypeRef FunType;

  //===--------------------------------------------------------------------===//
  // Protected Constructor (for subclasses)
  //===--------------------------------------------------------------------===//
  FunDecl(Kind K, SrcLocation Loc, std::string Id, TypeRef ReturnTy,
          std::vector<std::unique_ptr<ParamDecl>> Params,
          std::unique_ptr<Block> Body)
      : Decl(K, std::move(Loc), std::move(Id)), ReturnType(std::move(ReturnTy)),
        Params(std::move(Params)), Body(std::move(Body)), FunType([&]() {
          std::vector<TypeRef> ParamTys;
          ParamTys.reserve(this->Params.size());
          for (auto const &P : this->Params) {
            assert(!P->getType().isVar() && "Type of param cannot be VarTy");
            ParamTys.emplace_back(P->getType(), SrcSpan(P->getLocation()));
          }
          return TypeCtx::getFun(ParamTys, this->ReturnType,
                                 SrcSpan(Decl::getLocation()));
        }()) // immediately invoked lambda
  {
    assert(!ReturnType.isVar() && "Return Type of function cannot be VarTy");
  }

public:
  //===--------------------------------------------------------------------===//
  // Public constructor
  //===--------------------------------------------------------------------===//
  FunDecl(SrcLocation Loc, std::string Id, TypeRef ReturnTy,
          std::vector<std::unique_ptr<ParamDecl>> Params,
          std::unique_ptr<Block> Body)
      : FunDecl(Kind::FunDecl, std::move(Loc), std::move(Id),
                std::move(ReturnTy), std::move(Params), std::move(Body)) {}

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//
  [[nodiscard]] TypeRef getType() const override { return FunType; }
  [[nodiscard]] TypeRef getFunType() const { return FunType; }
  [[nodiscard]] TypeRef getReturnTy() const { return ReturnType; }
  [[nodiscard]] auto &getParams() { return Params; }
  [[nodiscard]] Block &getBody() const { return *Body; }
  [[nodiscard]] std::unique_ptr<Block> &getBodyPtr() { return Body; }

  //===--------------------------------------------------------------------===//
  // Setters
  //===--------------------------------------------------------------------===//
  void setBody(std::unique_ptr<Block> B) { Body = std::move(B); }

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
};
//===----------------------------------------------------------------------===//
// MethodDecl - Struct/Enum member function
//===----------------------------------------------------------------------===//

class MethodDecl final : public FunDecl {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  MethodDecl(SrcLocation Loc, std::string Id, TypeRef ReturnType,
             std::vector<std::unique_ptr<ParamDecl>> Params,
             std::unique_ptr<Block> Body, bool IsPrivate)
      : FunDecl(Kind::MethodDecl, std::move(Loc), std::move(Id),
                std::move(ReturnType), std::move(Params), std::move(Body)),
        IsPrivate(IsPrivate) {}

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] auto getParent() const { return Parent; }

  //===--------------------------------------------------------------------===//
  // Setters
  //===--------------------------------------------------------------------===//

  void setParent(AdtDecl *P) { Parent = P; }
  void setMangledId(std::string Mangled) { MangledId = std::move(Mangled); }

  //===--------------------------------------------------------------------===//
  // Type Queries
  //===--------------------------------------------------------------------===//

  [[nodiscard]] bool isPrivate() const { return IsPrivate; }
  [[nodiscard]] std::string getMangledId() const { return MangledId; }

  //===--------------------------------------------------------------------===//
  // LLVM-style RTTI
  //===--------------------------------------------------------------------===//

  static bool classof(const Decl *D) {
    return D->getKind() == Kind::MethodDecl;
  }

private:
  AdtDecl *Parent = nullptr;

  std::string MangledId;
  bool IsPrivate;
};

//===----------------------------------------------------------------------===//
// StructDecl - Aggregate type
//===----------------------------------------------------------------------===//

class StructDecl final : public AdtDecl {
public:
  StructDecl(SrcLocation Loc, const std::string &Id,
             std::vector<FieldDecl> Fields, std::vector<MethodDecl> Methods);

  [[nodiscard]] auto &getFields() { return Fields; }

  [[nodiscard]] FieldDecl *getField(const std::string &Id) const {
    auto it = FieldMap.find(Id);
    return it != FieldMap.end() ? it->second : nullptr;
  }

  static bool classof(const Decl *D) {
    return D->getKind() == Kind::StructDecl;
  }

  // void accept(TypeInferencer &) override;
  // bool accept(TypeChecker &) override;
  // void accept(CodeGen &) override;
  void emit(int level) const override;

private:
  std::vector<FieldDecl> Fields;
  std::unordered_map<std::string, FieldDecl *> FieldMap;
};

class VariantDecl final : public Decl {
public:
  VariantDecl(SrcLocation Loc, std::string Id, std::optional<TypeRef> T);

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] bool hasType() const { return DeclType != std::nullopt; };
  [[nodiscard]] TypeRef getType() const override { return *DeclType; }

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
  // should be optional since variants can be payload-less
  std::optional<TypeRef> DeclType;
};

class EnumDecl final : public AdtDecl {
public:
  EnumDecl(SrcLocation Loc, const std::string &Id,
           std::vector<VariantDecl> Variants, std::vector<MethodDecl> Methods);

  [[nodiscard]] auto &getVariants() { return Variants; }

  [[nodiscard]] VariantDecl *getVariant(const std::string &Id) const {
    auto It = VariantMap.find(Id);
    return It != VariantMap.end() ? It->second : nullptr;
  }

  static bool classof(const Decl *D) { return D->getKind() == Kind::EnumDecl; }

  // void accept(TypeInferencer &) override;
  // bool accept(TypeChecker &) override;
  // void accept(CodeGen &) override;
  void emit(int level) const override;

private:
  std::vector<VariantDecl> Variants;
  std::unordered_map<std::string, VariantDecl *> VariantMap;
};

} // namespace phi
