#include "AST/Nodes/Stmt.hpp"

#include <cstddef>
#include <cstdint>
#include <print>
#include <string>
#include <utility>
#include <vector>

#include "AST/Nodes/Expr.hpp"
#include "AST/TypeSystem/Context.hpp"
// #include "CodeGen/CodeGen.hpp"
// #include "Sema/TypeChecker.hpp"
// #include "Sema/TypeInference/Infer.hpp"

namespace {

//===----------------------------------------------------------------------===//
// Utility Functions
//===----------------------------------------------------------------------===//

/// Generates indentation string for AST dumping
/// @param Level Current indentation Level
/// @return String of spaces for indentation
std::string indent(const int Level) {
  return {static_cast<char>(static_cast<long>(Level) * 2), ' '};
}

} // anonymous namespace

namespace phi {

AdtDecl::AdtDecl(Kind K, SrcLocation Loc, std::string Id, TypeRef DeclType,
                 std::vector<MethodDecl> Methods)
    : Decl(K, std::move(Loc), std::move(Id)), DeclType(std::move(DeclType)),
      Methods(std::move(Methods)) {
  assert(DeclType.isAdt() && "Type of an AdtDecl must be AdtTy");
  for (auto &M : this->Methods)
    MethodMap.emplace(M.getId(), &M);
}

//===----------------------------------------------------------------------===//
// VarDecl Implementation
//===----------------------------------------------------------------------===//

VarDecl::VarDecl(SrcLocation Loc, std::string Id,
                 std::optional<TypeRef> DeclType, bool IsConst,
                 std::unique_ptr<Expr> Init)
    : ValueDecl(Kind::VarDecl, std::move(Loc), std::move(Id),
                std::move(DeclType)),
      IsConst(IsConst), Init(std::move(Init)) {}

VarDecl::~VarDecl() = default;

// Visitor Methods
// void VarDecl::accept(TypeInferencer &I) { I.visit(*this); }
// bool VarDecl::accept(TypeChecker &C) { return C.visit(*this); }
// void VarDecl::accept(CodeGen &G) { G.visit(*this); }

// Utility Methods
void VarDecl::emit(int Level) const {
  std::string TypeStr = DeclType.toString();
  std::println("{}VarDecl: {} (type: {})", indent(Level), Id, TypeStr);
  if (Init) {
    std::println("{}Initializer:", indent(Level));
    Init->emit(Level + 1);
  }
}

//===----------------------------------------------------------------------===//
// ParamDecl Implementation
//===----------------------------------------------------------------------===//

// Visitor Methods
// void ParamDecl::accept(TypeInferencer &I) { I.visit(*this); }
// bool ParamDecl::accept(TypeChecker &C) { return C.visit(*this); }
// void ParamDecl::accept(CodeGen &G) { G.visit(*this); }

// Utility Methods
void ParamDecl::emit(int Level) const {
  std::string TypeStr = DeclType.toString();
  std::println("{}ParamDecl: {} (type: {})", indent(Level), Id, TypeStr);
}

//===----------------------------------------------------------------------===//
// FieldDecl Implementation
//===----------------------------------------------------------------------===//

FieldDecl::FieldDecl(SrcLocation Loc, std::string Id, TypeRef DeclType,
                     std::unique_ptr<Expr> Init, bool IsPrivate, uint32_t Index)
    : ValueDecl(Kind::FieldDecl, std::move(Loc), std::move(Id),
                std::move(DeclType)),
      IsPrivate(IsPrivate), Init(std::move(Init)), Index(Index) {}

FieldDecl::~FieldDecl() = default;

// Visitor Methods
// void FieldDecl::accept(TypeInferencer &I) { I.visit(*this); }
// bool FieldDecl::accept(TypeChecker &C) { return C.visit(*this); }
// void FieldDecl::accept(CodeGen &G) { G.visit(*this); }

// Utility Methods
void FieldDecl::emit(int Level) const {
  std::string TypeStr = DeclType.toString();
  std::string Visibility = isPrivate() ? "private" : "public";
  std::println("{}{} FieldDecl: {} (type: {})", indent(Level), Visibility, Id,
               TypeStr);
}

//===----------------------------------------------------------------------===//
// FunDecl Implementation
//===----------------------------------------------------------------------===//

// Visitor Methods
// void FunDecl::accept(TypeInferencer &I) { I.visit(*this); }
// bool FunDecl::accept(TypeChecker &C) { return C.visit(*this); }
// void FunDecl::accept(CodeGen &G) { G.visit(*this); }

// Utility Methods
void FunDecl::emit(int Level) const {
  std::println("{}Function {}. Type: {}", indent(Level), Id,
               FunType.toString());
  for (auto &p : Params) {
    p->emit(Level + 1);
  }
  Body->emit(Level + 1);
}

//===----------------------------------------------------------------------===//
// MethodDecl Implementation
//===----------------------------------------------------------------------===//

// Visitor Methods
// void MethodDecl::accept(TypeInferencer &I) { I.visit(*this); }
// bool MethodDecl::accept(TypeChecker &C) { return C.visit(*this); }
// void MethodDecl::accept(CodeGen &G) { G.visit(*this); }

//===----------------------------------------------------------------------===//
// StructDecl Implementation
//===----------------------------------------------------------------------===//

StructDecl::StructDecl(SrcLocation Loc, const std::string &Id,
                       std::vector<FieldDecl> Fields,
                       std::vector<MethodDecl> Methods)
    : AdtDecl(Kind::StructDecl, std::move(Loc), Id,
              TypeCtx::getAdt(Id, this, SrcSpan(Loc)), std::move(Methods)),
      Fields(std::move(Fields)) {
  std::vector<TypeRef> ContainedTypes;
  for (auto &Field : this->Fields) {
    FieldMap[Field.getId()] = &Field;
    Field.setParent(this);
    ContainedTypes.push_back(Field.getType());
  }

  for (auto &Method : this->Methods) {
    MethodMap[Method.getId()] = &Method;
    Method.setParent(this);
  }
}

// Visitor Methods
// void StructDecl::accept(TypeInferencer &I) { I.visit(*this); }
// bool StructDecl::accept(TypeChecker &C) { return C.visit(*this); }
// void StructDecl::accept(CodeGen &G) { G.visit(*this); }

// Utility Methods
void StructDecl::emit(int Level) const {
  std::println("{}StructDecl: {} (type: {})", indent(Level), Id,
               DeclType.toString());

  std::println("{}Fields:", indent(Level));
  for (auto &F : Fields) {
    F.emit(Level + 1);
  }

  std::println("{}Methods:", indent(Level));
  for (auto &M : Methods) {
    M.emit(Level + 1);
  }
}

//===----------------------------------------------------------------------===//
// VariantDecl Implementation
//===----------------------------------------------------------------------===//

VariantDecl::VariantDecl(SrcLocation Loc, std::string Id,
                         std::optional<TypeRef> DeclType)
    : Decl(Kind::VariantDecl, std::move(Loc), std::move(Id)),
      DeclType(std::move(DeclType)) {}

// Visitor Methods
// void VariantDecl::accept(TypeInferencer &I) { I.visit(*this); }
// bool VariantDecl::accept(TypeChecker &C) { return C.visit(*this); }
// void VariantDecl::accept(CodeGen &G) { G.visit(*this); }

// Utility Methods
void VariantDecl::emit(int Level) const {
  std::string T = (hasType()) ? DeclType->toString() : "No Type";
  std::println("{}VariantDecl: {} (type: {})", indent(Level), Id, T);
}

//===----------------------------------------------------------------------===//
// EnumDecl Implementation
//===----------------------------------------------------------------------===//

EnumDecl::EnumDecl(SrcLocation Loc, const std::string &Id,
                   std::vector<VariantDecl> Variants,
                   std::vector<MethodDecl> Methods)
    : AdtDecl(Kind::EnumDecl, std::move(Loc), Id,
              TypeCtx::getAdt(Id, this, SrcSpan(Loc)), std::move(Methods)),
      Variants(std::move(Variants)) {
  std::vector<Type *> ContainedTypes;
  for (auto &&Variant : this->Variants) {
    VariantMap[Variant.getId()] = &Variant;
  }

  for (auto &Method : this->Methods) {
    MethodMap[Method.getId()] = &Method;
    Method.setParent(this);
  }
}

// Visitor Methods
// void EnumDecl::accept(TypeInferencer &I) { I.visit(*this); }
// bool EnumDecl::accept(TypeChecker &C) { return C.visit(*this); }
// void EnumDecl::accept(CodeGen &G) { G.visit(*this); }

// Utility Methods
void EnumDecl::emit(int Level) const {
  std::println("{}EnumDecl: {} (type: {})", indent(Level), Id,
               DeclType.toString());

  std::println("{}Variants:", indent(Level));
  for (auto &V : Variants) {
    V.emit(Level + 1);
  }

  std::println("{}Methods:", indent(Level));
  for (auto &M : Methods) {
    M.emit(Level + 1);
  }
}

} // namespace phi
