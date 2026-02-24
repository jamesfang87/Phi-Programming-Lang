#include "AST/Nodes/Decl.hpp"

#include <cassert>
#include <optional>
#include <print>
#include <string>

#include "AST/Nodes/Expr.hpp"
#include "AST/TypeSystem/Type.hpp"

namespace {

//===----------------------------------------------------------------------===//
// Utility Functions
//===----------------------------------------------------------------------===//

/// Generates indentation string for AST dumping
static std::string indent(int Level) { return std::string(Level * 2, ' '); }

/// Converts visibility enum to string
static const char *toString(phi::Visibility V) {
  return V == phi::Visibility::Public ? "public" : "private";
}

/// Converts mutability enum to string
static const char *toString(phi::Mutability M) {
  return M == phi::Mutability::Const ? "const" : "var";
}

} // anonymous namespace

namespace phi {

void TypeArgDecl::emit(int Level) const {
  std::println("{}Type Argument {} @ {}", indent(Level), getId(),
               getSpan().toString());
}

//===----------------------------------------------------------------------===//
// ParamDecl Implementation
//===----------------------------------------------------------------------===//

ParamDecl::ParamDecl(SrcSpan Span, Mutability M, std::string Id, TypeRef Type)
    : LocalDecl(Kind::Param, Span, std::move(Id), Type) {
  TheMutability = M;
}

void ParamDecl::emit(int Level) const {
  std::println("{}ParamDecl: {} ({} : {})", indent(Level), getId(),
               toString(TheMutability), Type.toString());
}

//===----------------------------------------------------------------------===//
// VarDecl Implementation
//===----------------------------------------------------------------------===//

VarDecl::VarDecl(SrcSpan Span, Mutability M, std::string Id,
                 std::optional<TypeRef> DeclType, std::unique_ptr<Expr> Init)
    : LocalDecl(Kind::Var, Span, std::move(Id),
                DeclType.value_or(TypeCtx::getVar(VarTy::Any, Span))),
      Init(std::move(Init)) {
  TheMutability = M;
}

void VarDecl::emit(int Level) const {
  std::println("{}VarDecl: {} ({} : {})", indent(Level), getId(),
               toString(TheMutability), Type.toString());

  if (Init) {
    std::println("{}Initializer:", indent(Level + 1));
    Init->emit(Level + 2);
  }
}

//===----------------------------------------------------------------------===//
// FieldDecl Implementation
//===----------------------------------------------------------------------===//

FieldDecl::FieldDecl(SrcSpan Span, uint32_t Index, Visibility Vis,
                     std::string Id, TypeRef Type, std::unique_ptr<Expr> Init)
    : MemberDecl(Kind::Field, Span, Vis, std::move(Id)), Index(Index),
      Type(Type), Init(std::move(Init)) {}

void FieldDecl::emit(int Level) const {
  std::println("{}{} FieldDecl: {} (type: {})", indent(Level),
               toString(getVisibility()), getId(), Type.toString());

  if (Init) {
    std::println("{}Initializer:", indent(Level + 1));
    Init->emit(Level + 2);
  }
}

//===----------------------------------------------------------------------===//
// MethodDecl Implementation
//===----------------------------------------------------------------------===//

MethodDecl::MethodDecl(SrcSpan Span, Visibility Vis, std::string Id,
                       std::vector<std::unique_ptr<TypeArgDecl>> TypeArgs,
                       std::vector<std::unique_ptr<ParamDecl>> Params,
                       TypeRef ReturnType, std::unique_ptr<Block> Body)
    : MemberDecl(Kind::Method, Span, Vis, std::move(Id)),
      TypeArgs(std::move(TypeArgs)), Params(std::move(Params)),
      ReturnType(ReturnType), Body(std::move(Body)) {}

void MethodDecl::emit(int Level) const {
  std::println("{}{} MethodDecl: {} -> {}", indent(Level),
               toString(getVisibility()), getId(), ReturnType.toString());

  std::println("{}Type Arguments:", indent(Level + 1));
  if (!hasTypeArgs()) {
    std::println("{}None.", indent(Level + 2));
  } else {
    for (auto &T : TypeArgs) {
      T->emit(Level + 2);
    }
  }

  std::println("{}Params:", indent(Level + 1));
  for (auto &P : Params) {
    P->emit(Level + 2);
  }

  std::println("{}Body:", indent(Level + 1));
  Body->emit(Level + 2);
}

//===----------------------------------------------------------------------===//
// VariantDecl Implementation
//===----------------------------------------------------------------------===//

VariantDecl::VariantDecl(SrcSpan Span, std::string Id,
                         std::optional<TypeRef> PayloadType)
    : MemberDecl(Kind::Variant, Span, Visibility::Public, std::move(Id)),
      PayloadType(std::move(PayloadType)) {}

void VariantDecl::emit(int Level) const {
  if (PayloadType) {
    std::println("{}VariantDecl: {} (payload: {})", indent(Level), getId(),
                 PayloadType->toString());
  } else {
    std::println("{}VariantDecl: {}", indent(Level), getId());
  }
}

//===----------------------------------------------------------------------===//
// StructDecl Implementation
//===----------------------------------------------------------------------===//

StructDecl::StructDecl(SrcSpan Span, Visibility Vis, std::string Id,
                       std::vector<std::unique_ptr<TypeArgDecl>> TypeArgs,
                       std::vector<std::unique_ptr<FieldDecl>> Fields,
                       std::vector<std::unique_ptr<MethodDecl>> Methods)
    : AdtDecl(Kind::Struct, Span, Vis, std::move(Id), std::move(TypeArgs),
              std::move(Methods)),
      Fields(std::move(Fields)) {

  for (auto &F : this->Fields) {
    FieldMap.emplace(F->getId(), F.get());
    F->setParent(this);
  }
}

void StructDecl::emit(int Level) const {
  std::println("{}StructDecl: {} (type: {})", indent(Level), getId(),
               Type.toString());

  std::println("{}Type Arguments:", indent(Level + 1));
  if (!hasTypeArgs()) {
    std::println("{}None.", indent(Level + 2));
  } else {
    for (auto &T : getTypeArgs()) {
      T->emit(Level + 2);
    }
  }

  std::println("{}Fields:", indent(Level + 1));
  for (auto &F : Fields) {
    F->emit(Level + 2);
  }

  std::println("{}Methods:", indent(Level + 1));
  for (auto &M : Methods) {
    M->emit(Level + 2);
  }
}

//===----------------------------------------------------------------------===//
// EnumDecl Implementation
//===----------------------------------------------------------------------===//

EnumDecl::EnumDecl(SrcSpan Span, Visibility Vis, std::string Id,
                   std::vector<std::unique_ptr<TypeArgDecl>> TypeArgs,
                   std::vector<std::unique_ptr<VariantDecl>> Variants,
                   std::vector<std::unique_ptr<MethodDecl>> Methods)
    : AdtDecl(Kind::Enum, Span, Vis, std::move(Id), std::move(TypeArgs),
              std::move(Methods)),
      Variants(std::move(Variants)) {

  for (auto &V : this->Variants) {
    VariantMap.emplace(V->getId(), V.get());
    V->setParent(this);
  }
}

void EnumDecl::emit(int Level) const {
  std::println("{}EnumDecl: {} (type: {})", indent(Level), getId(),
               getType().toString());

  std::println("{}Type Arguments:", indent(Level + 1));
  if (!hasTypeArgs()) {
    std::println("{}None.", indent(Level + 2));
  } else {
    for (auto &T : getTypeArgs()) {
      T->emit(Level + 2);
    }
  }

  std::println("{}Variants:", indent(Level + 1));
  for (auto &V : Variants) {
    V->emit(Level + 2);
  }

  std::println("{}Methods:", indent(Level + 1));
  for (auto &M : Methods) {
    M->emit(Level + 2);
  }
}

//===----------------------------------------------------------------------===//
// FunDecl Implementation
//===----------------------------------------------------------------------===//

FunDecl::FunDecl(SrcSpan Span, Visibility Vis, std::string Id,
                 std::vector<std::unique_ptr<TypeArgDecl>> TypeArgs,
                 std::vector<std::unique_ptr<ParamDecl>> Params,
                 TypeRef ReturnType, std::unique_ptr<Block> Body)
    : ItemDecl(Kind::Fun, Span, Vis, std::move(Id), std::move(TypeArgs)),
      Params(std::move(Params)), ReturnType(ReturnType), Body(std::move(Body)) {
}

void FunDecl::emit(int Level) const {
  std::println("{}FunctionDecl: {} -> {}", indent(Level), getId(),
               ReturnType.toString());

  std::println("{}Type Arguments:", indent(Level + 1));
  if (!hasTypeArgs()) {
    std::println("{}None.", indent(Level + 2));
  } else {
    for (auto &T : getTypeArgs()) {
      T->emit(Level + 2);
    }
  }

  std::println("{}Params:", indent(Level + 1));
  for (auto &P : Params) {
    P->emit(Level + 2);
  }

  std::println("{}Body:", indent(Level + 1));
  Body->emit(Level + 2);
}

//===----------------------------------------------------------------------===//
// ModuleDecl Implementation
//===----------------------------------------------------------------------===//

ModuleDecl::ModuleDecl(SrcSpan PathSpan, Visibility Vis, std::string Id,
                       std::vector<std::string> Path,
                       std::vector<std::unique_ptr<ItemDecl>> Items,
                       std::vector<std::unique_ptr<ImportStmt>> Imports)
    : ItemDecl(Kind::Module, PathSpan, Vis, std::move(Id), {}),
      Path(std::move(Path)), Items(std::move(Items)) {

  for (auto &Item : this->Items) {
    if (Item->getVisibility() == Visibility::Public) {
      PublicItems.push_back(Item.get());
    }
  }

  for (auto &I : Imports) {
    this->Imports.push_back(std::move(*I));
  }
}

void ModuleDecl::emit(int Level) const {
  std::println("{}ModuleDecl: {}", indent(Level), getId());

  if (!Path.empty()) {
    std::println("{}Path:", indent(Level + 1));
    for (auto &P : Path) {
      std::println("{}{}", indent(Level + 2), P);
    }
  }

  if (!Imports.empty()) {
    std::println("{}Imports:", indent(Level + 1));
    for (auto &I : Imports) {
      I.emit(Level + 2);
    }
  }

  std::println("{}Items:", indent(Level + 1));
  for (auto &D : Items) {
    D->emit(Level + 2);
  }
}

} // namespace phi
