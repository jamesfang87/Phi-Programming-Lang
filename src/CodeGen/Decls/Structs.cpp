#include "CodeGen/CodeGen.hpp"

using namespace phi;

void CodeGen::declareHeader(StructDecl &D) {
  llvm::StructType::create(Context, "struct." + D.getId());
}

void CodeGen::visit(StructDecl &D) {
  auto *Type = static_cast<llvm::StructType *>(D.getType().toLLVM(Context));

  std::vector<llvm::Type *> FieldTypes;
  for (auto &&Field : D.getFields()) {
    llvm::Type *T = Field->getType().toLLVM(Context);
    FieldTypes.emplace_back(T);
  }

  Type->setBody(FieldTypes);
}
