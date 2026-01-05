#pragma once

#include <cassert>
#include <cstdint>
#include <string>
#include <utility>
#include <vector>

#include <llvm/Support/Casting.h>

#include "SrcManager/SrcSpan.hpp"

namespace phi {

class BuiltinTy;
class AdtTy;
class TupleTy;
class FunTy;
class PtrTy;
class RefTy;
class VarTy;
class ErrTy;

class Type {
public:
  enum class TypeKind : uint8_t {
    Builtin,
    Adt,
    Tuple,
    Fun,
    Ptr,
    Ref,
    Var,
    Err,
  };

  explicit Type(TypeKind K) : TheKind(K) {}
  virtual ~Type() = default;

  [[nodiscard]] TypeKind getKind() const { return TheKind; }
  [[nodiscard]] virtual std::string toString() const = 0;

  [[nodiscard]] bool isBuiltin() const { return llvm::isa<BuiltinTy>(this); }
  [[nodiscard]] bool isAdt() const { return llvm::isa<AdtTy>(this); }
  [[nodiscard]] bool isTuple() const { return llvm::isa<TupleTy>(this); }
  [[nodiscard]] bool isFun() const { return llvm::isa<FunTy>(this); }
  [[nodiscard]] bool isPtr() const { return llvm::isa<PtrTy>(this); }
  [[nodiscard]] bool isRef() const { return llvm::isa<RefTy>(this); }
  [[nodiscard]] bool isVar() const { return llvm::isa<VarTy>(this); }
  [[nodiscard]] bool isErr() const { return llvm::isa<ErrTy>(this); }
  [[nodiscard]] Type *getUnderlying();

private:
  TypeKind TheKind;
};

class TypeRef {
public:
  TypeRef(Type *T, SrcSpan Span) : Ptr(T), Span(std::move(Span)) {}
  TypeRef(const TypeRef &T, SrcSpan Span)
      : Ptr(T.getPtr()), Span(std::move(Span)) {}

  [[nodiscard]] Type *getPtr() const { return Ptr; }
  [[nodiscard]] SrcSpan getSpan() const { return Span; }

  [[nodiscard]] std::string toString() const { return Ptr->toString(); }
  [[nodiscard]] bool isBuiltin() const { return llvm::isa<BuiltinTy>(Ptr); }
  [[nodiscard]] bool isAdt() const { return llvm::isa<AdtTy>(Ptr); }
  [[nodiscard]] bool isTuple() const { return llvm::isa<TupleTy>(Ptr); }
  [[nodiscard]] bool isFun() const { return llvm::isa<FunTy>(Ptr); }
  [[nodiscard]] bool isPtr() const { return llvm::isa<PtrTy>(Ptr); }
  [[nodiscard]] bool isRef() const { return llvm::isa<RefTy>(Ptr); }
  [[nodiscard]] bool isVar() const { return llvm::isa<VarTy>(Ptr); }
  [[nodiscard]] bool isErr() const { return llvm::isa<ErrTy>(Ptr); }
  [[nodiscard]] TypeRef getUnderlying();

private:
  Type *Ptr; // cannot be nullptr
  SrcSpan Span;
};

class BuiltinTy final : public Type {
public:
  enum Kind : uint8_t {
    i8,
    i16,
    i32,
    i64,
    u8,
    u16,
    u32,
    u64,
    f32,
    f64,
    String,
    Char,
    Bool,
    Range,
    Null,
  };

  explicit BuiltinTy(Kind K) : Type(TypeKind::Builtin), TheKind(K) {}

  [[nodiscard]] Kind getBuiltinKind() const { return TheKind; }
  [[nodiscard]] std::string toString() const override;

  static bool classof(const Type *T) {
    return T->getKind() == TypeKind::Builtin;
  }

private:
  const Kind TheKind;
};

class AdtTy final : public Type {
public:
  explicit AdtTy(std::string Id, class AdtDecl *D = nullptr)
      : Type(TypeKind::Adt), Id(std::move(Id)), Decl(D) {}

  [[nodiscard]] const std::string &getId() const { return Id; }
  [[nodiscard]] const AdtDecl *getDecl() const { return Decl; };
  [[nodiscard]] std::string toString() const override;

  void setDecl(AdtDecl *D) const {
    assert(D != nullptr);
    Decl = D;
  }

  static bool classof(const Type *T) { return T->getKind() == TypeKind::Adt; }

private:
  const std::string Id;
  mutable const AdtDecl *Decl;
};

class TupleTy final : public Type {
public:
  explicit TupleTy(std::vector<TypeRef> E)
      : Type(TypeKind::Tuple), ElementTys(std::move(E)) {}

  [[nodiscard]] auto getElementTys() const { return ElementTys; }
  [[nodiscard]] std::string toString() const override;

  static bool classof(const Type *T) { return T->getKind() == TypeKind::Tuple; }

private:
  const std::vector<TypeRef> ElementTys;
};

class FunTy final : public Type {
public:
  FunTy(std::vector<TypeRef> Params, TypeRef Ret)
      : Type(TypeKind::Fun), ReturnTy(std::move(Ret)),
        ParamTys(std::move(Params)) {}

  [[nodiscard]] auto getReturnTy() const { return ReturnTy; }
  [[nodiscard]] auto getParamTys() const { return ParamTys; }
  [[nodiscard]] std::string toString() const override;

  static bool classof(const Type *T) { return T->getKind() == TypeKind::Fun; }

private:
  TypeRef ReturnTy;
  const std::vector<TypeRef> ParamTys;
};

class PtrTy final : public Type {
public:
  explicit PtrTy(TypeRef P) : Type(TypeKind::Ptr), Pointee(std::move(P)) {}

  [[nodiscard]] auto getPointee() const { return Pointee; }
  [[nodiscard]] std::string toString() const override;

  static bool classof(const Type *T) { return T->getKind() == TypeKind::Ptr; }

private:
  TypeRef Pointee;
};

class RefTy final : public Type {
public:
  explicit RefTy(TypeRef P) : Type(TypeKind::Ref), Pointee(std::move(P)) {}

  [[nodiscard]] auto getPointee() const { return Pointee; }
  [[nodiscard]] std::string toString() const override;

  static bool classof(const Type *T) { return T->getKind() == TypeKind::Ref; }

private:
  TypeRef Pointee;
};

class VarTy final : public Type {
public:
  enum Domain : uint8_t {
    Any,
    Int,
    Float,
    Adt,
  };

  explicit VarTy(uint64_t N, Domain D)
      : Type(TypeKind::Var), N(N), TheDomain(D) {}

  [[nodiscard]] auto getN() const { return N; }
  [[nodiscard]] auto getDomain() const { return TheDomain; }
  [[nodiscard]] std::string toString() const override;

  [[nodiscard]] bool occursIn(TypeRef Other) const;
  [[nodiscard]] bool accepts(TypeRef T) const;
  [[nodiscard]] std::optional<Domain> unifyDomain(const VarTy *Var) const;
  void setDomain(Domain New) { TheDomain = New; }

  static bool classof(const Type *T) { return T->getKind() == TypeKind::Var; }

private:
  const uint64_t N;
  Domain TheDomain;
};

class ErrTy final : public Type {
public:
  ErrTy() : Type(TypeKind::Err) {}

  [[nodiscard]] std::string toString() const override;

  static bool classof(const Type *T) { return T->getKind() == TypeKind::Err; }
};

struct TupleKey {
  std::vector<TypeRef> Elements;

  bool operator==(const TupleKey &Other) const noexcept {
    if (Elements.size() != Other.Elements.size())
      return false;
    for (size_t i = 0; i < Elements.size(); ++i)
      if (Elements[i].getPtr() != Other.Elements[i].getPtr())
        return false;
    return true;
  }
};

struct FunKey {
  const std::vector<TypeRef> Params;
  TypeRef Ret;

  bool operator==(const FunKey &Other) const noexcept {
    if (Ret.getPtr() != Other.Ret.getPtr())
      return false;
    if (Params.size() != Other.Params.size())
      return false;
    for (size_t i = 0; i < Params.size(); ++i)
      if (Params[i].getPtr() != Other.Params[i].getPtr())
        return false;
    return true;
  }
};

struct TupleKeyHash {
  std::size_t operator()(TupleKey const &k) const noexcept;
};

struct FunKeyHash {
  std::size_t operator()(FunKey const &k) const noexcept;
};

} // namespace phi
