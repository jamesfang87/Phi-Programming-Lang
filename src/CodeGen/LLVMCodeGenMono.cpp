#include "CodeGen/LLVMCodeGen.hpp"

#include <cctype>
#include <llvm/ADT/TypeSwitch.h>
#include <llvm/IR/Constants.h>
#include <llvm/IR/DerivedTypes.h>
#include <llvm/IR/Verifier.h>

using namespace phi;

//===----------------------------------------------------------------------===//
// Phase 2: Monomorphization
//===----------------------------------------------------------------------===//

void CodeGen::monomorphize() {
  while (!Instantiations.empty()) {
    auto It = Instantiations.begin();
    TypeInstantiation TI = *It;
    Instantiations.erase(It);
    monomorphizeDecl(TI);
  }
}

void CodeGen::monomorphizeDecl(const TypeInstantiation &TI) {
  if (auto *S = llvm::dyn_cast<StructDecl>(TI.GenericDecl)) {
    monomorphizeStruct(S, TI.TypeArgs);
  } else if (auto *E = llvm::dyn_cast<EnumDecl>(TI.GenericDecl)) {
    monomorphizeEnum(E, TI.TypeArgs);
  } else if (auto *F = llvm::dyn_cast<FunDecl>(TI.GenericDecl)) {
    monomorphizeFunction(F, TI.TypeArgs);
  } else if (auto *M = llvm::dyn_cast<MethodDecl>(TI.GenericDecl)) {
    monomorphizeMethod(M, TI.TypeArgs);
  }
}

void CodeGen::monomorphizeStruct(const StructDecl *S,
                                 const std::vector<TypeRef> &TypeArgs) {
  std::string MonoName = generateMonomorphizedName(S->getId(), TypeArgs);
  TypeInstantiation TI{S, TypeArgs};
  MonomorphizedNames[TI] = MonoName;

  SubstitutionMap Subs = buildSubstitutionMap(S, TypeArgs);

  // Create field types with substitutions
  std::vector<TypeRef> FieldTypes;
  for (const auto &F : S->getFields()) {
    FieldTypes.push_back(substituteType(F->getType(), Subs));
  }

  // Register field indices
  unsigned Idx = 0;
  for (const auto &F : S->getFields()) {
    FieldIndices[MonoName][F->getId()] = Idx++;
  }

  // Create the struct type
  getOrCreateStructType(MonoName, FieldTypes);

  // Monomorphize methods
  for (auto &Method : S->getMethods()) {
    monomorphizeMethod(Method.get(), TypeArgs);
  }
}
void CodeGen::monomorphizeEnum(const EnumDecl *E,
                               const std::vector<TypeRef> &TypeArgs) {
  std::string MonoName = generateMonomorphizedName(E->getId(), TypeArgs);
  TypeInstantiation TI{E, TypeArgs};
  MonomorphizedNames[TI] = MonoName;

  SubstitutionMap Subs = buildSubstitutionMap(E, TypeArgs);

  // Calculate variant info with substituted types
  uint64_t MaxPayloadSize = 0;
  unsigned Discriminant = 0;
  for (const auto &V : E->getVariants()) {
    VariantDiscriminants[MonoName][V->getId()] = Discriminant++;
    if (V->hasPayload()) {
      TypeRef SubPayload = substituteType(V->getPayloadType(), Subs);
      llvm::Type *PayloadTy = getLLVMType(SubPayload);
      VariantPayloadTypes[MonoName][V->getId()] = PayloadTy;
      MaxPayloadSize = std::max(MaxPayloadSize, getTypeSize(PayloadTy));
    }
  }

  // Create enum type
  std::vector<llvm::Type *> Members = {Builder.getInt32Ty()};
  if (MaxPayloadSize > 0) {
    Members.push_back(
        llvm::ArrayType::get(Builder.getInt8Ty(), MaxPayloadSize));
  }

  auto It = StructTypes.find(MonoName);
  if (It != StructTypes.end()) {
    It->second->setBody(Members);
  } else {
    auto *ST = llvm::StructType::create(Context, Members, MonoName);
    StructTypes[MonoName] = ST;
  }

  // Monomorphize methods
  for (auto &Method : E->getMethods()) {
    monomorphizeMethod(Method.get(), TypeArgs);
  }
}

void CodeGen::monomorphizeMethod(const MethodDecl *M,
                                 const std::vector<TypeRef> &TypeArgs) {
  // For now, simpler case: methods on generic structs, but method itself not
  // generic additional args?

  std::string ParentName = M->getParent()->getId();
  std::string MonoParentName = generateMonomorphizedName(ParentName, TypeArgs);
  std::string MonoMethodName = MonoParentName + "_" + M->getId();

  // Note: we track methods by the method decl + type args of the parent
  // If the method ALSO has type args, that's a separate level of instantiation
  // we likely fall into via discoverInExpr

  TypeInstantiation TI{M, TypeArgs};
  MonomorphizedNames[TI] = MonoMethodName;

  // Build substitution map (using Struct/Enum type args)
  SubstitutionMap Subs;
  if (auto *S = llvm::dyn_cast<StructDecl>(M->getParent())) {
    Subs = buildSubstitutionMap(S, TypeArgs);
  } else if (auto *E = llvm::dyn_cast<EnumDecl>(M->getParent())) {
    Subs = buildSubstitutionMap(E, TypeArgs);
  }

  auto SavedSubs = CurrentSubs;
  CurrentSubs = Subs;

  std::vector<llvm::Type *> ParamTypes;
  for (const auto &P : M->getParams()) {
    ParamTypes.push_back(getLLVMType(P->getType()));
  }

  llvm::Type *RetTy = getLLVMType(M->getReturnType());

  CurrentSubs = SavedSubs;

  auto *FnTy = llvm::FunctionType::get(RetTy, ParamTypes, false);
  auto *Fn = llvm::Function::Create(FnTy, llvm::Function::ExternalLinkage,
                                    MonoMethodName, Module);

  unsigned Idx = 0;
  for (auto &Arg : Fn->args()) {
    Arg.setName(M->getParams()[Idx++]->getId());
  }

  MonomorphizedMethodQueue.push_back({M, TypeArgs, Fn});
}

void CodeGen::monomorphizeFunction(const FunDecl *F,
                                   const std::vector<TypeRef> &TypeArgs) {
  std::string MonoName = generateMonomorphizedName(F->getId(), TypeArgs);
  TypeInstantiation TI{F, TypeArgs};
  MonomorphizedNames[TI] = MonoName;

  // Build the function signature
  SubstitutionMap Subs = buildSubstitutionMap(F, TypeArgs);

  // Save current subs and switch to monomorphization subs
  auto SavedSubs = CurrentSubs;
  CurrentSubs = Subs;

  std::vector<llvm::Type *> ParamTypes;
  for (const auto &P : F->getParams()) {
    ParamTypes.push_back(getLLVMType(P->getType()));
  }

  llvm::Type *RetTy = getLLVMType(F->getReturnType());

  CurrentSubs = SavedSubs;

  auto *FnTy = llvm::FunctionType::get(RetTy, ParamTypes, false);
  auto *Fn = llvm::Function::Create(FnTy, llvm::Function::ExternalLinkage,
                                    MonoName, Module);

  // Name parameters
  unsigned Idx = 0;
  for (auto &Arg : Fn->args()) {
    Arg.setName(F->getParams()[Idx++]->getId());
  }

  MonomorphizedFunctionQueue.push_back({F, TypeArgs, Fn});
}

TypeRef CodeGen::substituteType(TypeRef T, const SubstitutionMap &Subs) {
  const Type *Ptr = T.getPtr();

  if (auto *GT = llvm::dyn_cast<GenericTy>(Ptr)) {
    auto It = Subs.find(GT->getDecl());
    if (It != Subs.end()) {
      return It->second;
    }
    for (const auto &Pair : Subs) {
      if (Pair.first->getId() == GT->getId()) {
        return Pair.second;
      }
    }
    return T;
  }

  if (auto *ApT = llvm::dyn_cast<AppliedTy>(Ptr)) {
    std::vector<TypeRef> SubArgs;
    bool Changed = false;
    for (const auto &Arg : ApT->getArgs()) {
      TypeRef SubArg = substituteType(Arg, Subs);
      if (SubArg.getPtr() != Arg.getPtr())
        Changed = true;
      SubArgs.push_back(SubArg);
    }

    // Record potential new instantiation with substituted args
    if (auto *AdtBase = llvm::dyn_cast<AdtTy>(ApT->getBase().getPtr())) {
      if (AdtBase->getDecl()) {
        recordInstantiation(AdtBase->getDecl(), SubArgs);
      }
    }

    // Only create a new AppliedTy if something actually changed
    if (Changed) {
      return TypeRef(new AppliedTy(ApT->getBase(), SubArgs), T.getSpan());
    }
    return T;
  }

  if (auto *PT = llvm::dyn_cast<PtrTy>(Ptr)) {
    return TypeRef(new PtrTy(substituteType(PT->getPointee(), Subs)),
                   T.getSpan());
  }

  if (auto *RT = llvm::dyn_cast<RefTy>(Ptr)) {
    return TypeRef(new RefTy(substituteType(RT->getPointee(), Subs)),
                   T.getSpan());
  }

  if (auto *AT = llvm::dyn_cast<ArrayTy>(Ptr)) {
    return TypeRef(new ArrayTy(substituteType(AT->getContainedTy(), Subs)),
                   T.getSpan());
  }

  if (auto *TT = llvm::dyn_cast<TupleTy>(Ptr)) {
    std::vector<TypeRef> SubElems;
    for (const auto &Elem : TT->getElementTys()) {
      SubElems.push_back(substituteType(Elem, Subs));
    }
    return TypeRef(new TupleTy(SubElems), T.getSpan());
  }

  if (auto *FT = llvm::dyn_cast<FunTy>(Ptr)) {
    std::vector<TypeRef> SubParams;
    for (const auto &Param : FT->getParamTys()) {
      SubParams.push_back(substituteType(Param, Subs));
    }
    return TypeRef(
        new FunTy(SubParams, substituteType(FT->getReturnTy(), Subs)),
        T.getSpan());
  }

  return T;
}

CodeGen::SubstitutionMap
CodeGen::buildSubstitutionMap(const FunDecl *Decl,
                              const std::vector<TypeRef> &TypeArgs) {
  SubstitutionMap Subs;
  const auto &TypeParams = Decl->getTypeArgs();
  for (size_t I = 0; I < TypeParams.size() && I < TypeArgs.size(); ++I) {
    Subs.insert({TypeParams[I].get(), TypeArgs[I]});
  }
  return Subs;
}

CodeGen::SubstitutionMap
CodeGen::buildSubstitutionMap(const MethodDecl *Decl,
                              const std::vector<TypeRef> &TypeArgs) {
  SubstitutionMap Subs;
  const auto &TypeParams = Decl->getTypeArgs();
  for (size_t I = 0; I < TypeParams.size() && I < TypeArgs.size(); ++I) {
    Subs.insert({TypeParams[I].get(), TypeArgs[I]});
  }
  return Subs;
}

CodeGen::SubstitutionMap
CodeGen::buildSubstitutionMap(const AdtDecl *Decl,
                              const std::vector<TypeRef> &TypeArgs) {
  SubstitutionMap Subs;
  const auto &TypeParams = Decl->getTypeArgs();
  for (size_t I = 0; I < TypeParams.size() && I < TypeArgs.size(); ++I) {
    Subs.insert({TypeParams[I].get(), TypeArgs[I]});
  }
  return Subs;
}

std::string
CodeGen::generateMonomorphizedName(const std::string &BaseName,
                                   const std::vector<TypeRef> &TypeArgs) {
  std::string Result = BaseName;
  for (const auto &Arg : TypeArgs) {
    Result += "_" + Arg.toString();
  }
  // Clean up any special characters
  for (char &C : Result) {
    if (!std::isalnum(C) && C != '_') {
      C = '_';
    }
  }
  return Result;
}

void CodeGen::generateMonomorphizedBodies() {
  while (!MonomorphizedFunctionQueue.empty() ||
         !MonomorphizedMethodQueue.empty()) {
    // Process functions
    std::vector<MonomorphizedFun> Funs = std::move(MonomorphizedFunctionQueue);
    MonomorphizedFunctionQueue.clear();
    for (auto &MF : Funs) {
      if (GeneratedMonomorphizedBodies.contains(MF.Fn->getName().str()))
        continue;

      GeneratedMonomorphizedBodies.insert(MF.Fn->getName().str());

      // Save/Restore substitution map
      auto SavedSubs = CurrentSubs;
      CurrentSubs = buildSubstitutionMap(MF.Fun, MF.Args);

      codegenFunctionBody(const_cast<FunDecl *>(MF.Fun), MF.Fn);

      CurrentSubs = SavedSubs;
    }

    // Process methods
    std::vector<MonomorphizedMethod> Methods =
        std::move(MonomorphizedMethodQueue);
    MonomorphizedMethodQueue.clear();
    for (auto &MM : Methods) {
      if (GeneratedMonomorphizedBodies.contains(MM.Fn->getName().str()))
        continue;

      GeneratedMonomorphizedBodies.insert(MM.Fn->getName().str());

      auto SavedSubs = CurrentSubs;
      CurrentSubs = buildSubstitutionMap(MM.Method->getParent(), MM.Args);

      codegenMethodBody(const_cast<MethodDecl *>(MM.Method), MM.Fn);

      CurrentSubs = SavedSubs;
    }
  }
}
