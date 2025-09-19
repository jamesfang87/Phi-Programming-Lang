#include "AST/Decl.hpp"

#include <cstdint>
#include <print>
#include <string>
#include <vector>

#include "AST/Expr.hpp"
#include "CodeGen/CodeGen.hpp"
#include "Sema/TypeChecker.hpp"
#include "Sema/TypeInference/Infer.hpp"

namespace {

//===----------------------------------------------------------------------===//
// Utility Functions
//===----------------------------------------------------------------------===//

/// Generates indentation string for AST dumping
/// @param Level Current indentation Level
/// @return String of spaces for indentation
std::string indent(int Level) { return std::string(Level * 2, ' '); }

} // anonymous namespace

namespace phi {

//===----------------------------------------------------------------------===//
// VarDecl Implementation
//===----------------------------------------------------------------------===//

// Constructors & Destructors
VarDecl::VarDecl(SrcLocation Loc, std::string Id, std::optional<Type> DeclType,
                 bool IsConst, std::unique_ptr<Expr> Init)
    : ValueDecl(Kind::VarDecl, std::move(Loc), std::move(Id),
                std::move(DeclType)),
      IsConst(IsConst), Init(std::move(Init)) {}

VarDecl::~VarDecl() = default;

// Visitor Methods
void VarDecl::accept(TypeInferencer &I) { I.visit(*this); }
bool VarDecl::accept(TypeChecker &C) { return C.visit(*this); }
void VarDecl::accept(CodeGen &G) { G.visit(*this); }

// Utility Methods
void VarDecl::emit(int Level) const {
  std::string typeStr =
      DeclType.has_value() ? DeclType.value().toString() : "<unresolved>";
  std::println("{}VarDecl: {} (type: {})", indent(Level), Id, typeStr);
  if (Init) {
    std::println("{}Initializer:", indent(Level));
    Init->emit(Level + 1);
  }
}

//===----------------------------------------------------------------------===//
// ParamDecl Implementation
//===----------------------------------------------------------------------===//

// Visitor Methods
void ParamDecl::accept(TypeInferencer &I) { I.visit(*this); }
bool ParamDecl::accept(TypeChecker &C) { return C.visit(*this); }
void ParamDecl::accept(CodeGen &G) { G.visit(*this); }

// Utility Methods
void ParamDecl::emit(int Level) const {
  std::string typeStr =
      DeclType.has_value() ? DeclType.value().toString() : "<unresolved>";
  std::println("{}ParamDecl: {} (type: {})", indent(Level), Id, typeStr);
}

//===----------------------------------------------------------------------===//
// FieldDecl Implementation
//===----------------------------------------------------------------------===//

// Constructors & Destructors
FieldDecl::FieldDecl(SrcLocation Loc, std::string Id, Type DeclType,
                     std::unique_ptr<Expr> Init, bool IsPrivate, uint32_t Index)
    : ValueDecl(Kind::FieldDecl, std::move(Loc), std::move(Id),
                std::move(DeclType)),
      IsPrivate(IsPrivate), Init(std::move(Init)), Index(Index) {}

FieldDecl::~FieldDecl() = default;

// Visitor Methods
void FieldDecl::accept(TypeInferencer &I) { I.visit(*this); }
bool FieldDecl::accept(TypeChecker &C) { return C.visit(*this); }
void FieldDecl::accept(CodeGen &G) { G.visit(*this); }

// Utility Methods
void FieldDecl::emit(int Level) const {
  std::string typeStr =
      DeclType.has_value() ? DeclType.value().toString() : "<unresolved>";
  std::string visibility = isPrivate() ? "private" : "public";
  std::println("{}{} FieldDecl: {} (type: {})", indent(Level), visibility, Id,
               typeStr);
}

//===----------------------------------------------------------------------===//
// FunDecl Implementation
//===----------------------------------------------------------------------===//

// Visitor Methods
void FunDecl::accept(TypeInferencer &I) { I.visit(*this); }
bool FunDecl::accept(TypeChecker &C) { return C.visit(*this); }
void FunDecl::accept(CodeGen &G) { G.visit(*this); }

// Utility Methods
void FunDecl::emit(int Level) const {
  std::println("{}Function {} at {}:{}. Returns {}", indent(Level), Id,
               Location.line, Location.col, ReturnType.toString());
  // Dump parameters
  for (auto &p : Params) {
    p->emit(Level + 1);
  }
  // Dump function body
  Body->emit(Level + 1);
}

//===----------------------------------------------------------------------===//
// MethodDecl Implementation
//===----------------------------------------------------------------------===//

// Visitor Methods
void MethodDecl::accept(TypeInferencer &I) { I.visit(*this); }
bool MethodDecl::accept(TypeChecker &C) { return C.visit(*this); }
void MethodDecl::accept(CodeGen &G) { G.visit(*this); }

//===----------------------------------------------------------------------===//
// StructDecl Implementation
//===----------------------------------------------------------------------===//

// Constructors & Destructors
StructDecl::StructDecl(SrcLocation Loc, std::string Id,
                       std::vector<std::unique_ptr<FieldDecl>> Fields,
                       std::vector<MethodDecl> Methods)
    : Decl(Kind::StructDecl, Loc, Id),
      DeclType(Type::makeCustom(std::move(Id), std::move(Loc))),
      Fields(std::move(Fields)), Methods(std::move(Methods)) {
  std::vector<Type> ContainedTypes;
  for (auto &Field : this->Fields) {
    FieldMap[Field->getId()] = Field.get();
    Field->setParent(this);
    ContainedTypes.push_back(Field->getType());
  }

  for (auto &Method : this->Methods) {
    MethodMap[Method.getId()] = &Method;
    Method.setParent(this);
  }
}

// Visitor Methods
void StructDecl::accept(TypeInferencer &I) { I.visit(*this); }
bool StructDecl::accept(TypeChecker &C) { return C.visit(*this); }
void StructDecl::accept(CodeGen &G) { G.visit(*this); }

// Utility Methods
void StructDecl::emit(int Level) const {
  std::println("{}StructDecl: {} (type: {})", indent(Level), Id,
               DeclType.toString());

  std::println("{}Fields:", indent(Level));
  for (auto &f : Fields) {
    f->emit(Level + 1);
  }

  std::println("{}Methods:", indent(Level));
  for (auto &m : Methods) {
    m.emit(Level + 1);
  }
}

} // namespace phi
