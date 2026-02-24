#include "CodeGen/LLVMCodeGen.hpp"

#include <llvm/ADT/TypeSwitch.h>
#include <llvm/IR/Constants.h>
#include <llvm/IR/DerivedTypes.h>
#include <llvm/IR/Verifier.h>
#include <llvm/TargetParser/Host.h>

#include <system_error>

using namespace phi;

#include <llvm/Support/raw_ostream.h>

//===----------------------------------------------------------------------===//
// Constructor & Main Entry Points
//===----------------------------------------------------------------------===//

CodeGen::CodeGen(std::vector<ModuleDecl *> Mods, std::string_view SourcePath)
    : Ast(std::move(Mods)), SourcePath(SourcePath), Context(), Builder(Context),
      Module(std::string(SourcePath), Context) {
  Module.setTargetTriple(llvm::sys::getDefaultTargetTriple());
}

void CodeGen::generate() {
  // Phase 1: Discover all generic instantiations
  discoverInstantiations();

  // Phase 2: Monomorphize generic types and functions
  monomorphize();

  // Phase 3: Desugar
  desugar();

  // Phase 4: IR Generation
  for (auto *M : Ast) {
    codegenModule(M);
  }

  // Final Phase: Generate bodies for monomorphized functions
  generateMonomorphizedBodies();
}

void CodeGen::outputIR(const std::string &Filename) {
  std::error_code EC;
  llvm::raw_fd_ostream File(Filename, EC);
  if (EC)
    throw std::runtime_error("Could not open file: " + EC.message());
  Module.print(File, nullptr);
}

//===----------------------------------------------------------------------===//
// Type Conversion
//===----------------------------------------------------------------------===//

llvm::Type *CodeGen::getLLVMType(TypeRef T) { return getLLVMType(T.getPtr()); }

bool CodeGen::hasGenericType(TypeRef T) { return hasGenericType(T.getPtr()); }

bool CodeGen::hasGenericType(const Type *T) {
  if (llvm::dyn_cast<GenericTy>(T))
    return true;
  if (auto *ApT = llvm::dyn_cast<AppliedTy>(T)) {
    for (const auto &Arg : ApT->getArgs()) {
      if (hasGenericType(Arg))
        return true;
    }
  } else if (auto *PT = llvm::dyn_cast<PtrTy>(T)) {
    return hasGenericType(PT->getPointee());
  } else if (auto *RT = llvm::dyn_cast<RefTy>(T)) {
    return hasGenericType(RT->getPointee());
  } else if (auto *AT = llvm::dyn_cast<ArrayTy>(T)) {
    return hasGenericType(AT->getContainedTy());
  } else if (auto *TT = llvm::dyn_cast<TupleTy>(T)) {
    for (const auto &Elem : TT->getElementTys()) {
      if (hasGenericType(Elem))
        return true;
    }
  } else if (auto *FT = llvm::dyn_cast<FunTy>(T)) {
    for (const auto &Param : FT->getParamTys()) {
      if (hasGenericType(Param))
        return true;
    }
    return hasGenericType(FT->getReturnTy());
  }
  return false;
}

llvm::Type *CodeGen::getLLVMType(const Type *T) {

  // Check substitution map for generic types
  if (auto *GT = llvm::dyn_cast<GenericTy>(T)) {
    auto It = CurrentSubs.find(GT->getDecl());
    if (It != CurrentSubs.end()) {
      return getLLVMType(It->second);
    }
  }

  bool IsGenericDependent = hasGenericType(T);

  // Check cache first (only if strict concrete type)
  if (!IsGenericDependent) {
    auto It = TypeCache.find(T);
    if (It != TypeCache.end())
      return It->second;
  }

  if (!T)
    return Builder.getVoidTy();

  llvm::Type *Result = nullptr;

  if (auto *BT = llvm::dyn_cast<BuiltinTy>(T)) {
    switch (BT->getBuiltinKind()) {
    case BuiltinTy::i8:
    case BuiltinTy::u8:
      Result = Builder.getInt8Ty();
      break;
    case BuiltinTy::i16:
    case BuiltinTy::u16:
      Result = Builder.getInt16Ty();
      break;
    case BuiltinTy::i32:
    case BuiltinTy::u32:
      Result = Builder.getInt32Ty();
      break;
    case BuiltinTy::i64:
    case BuiltinTy::u64:
      Result = Builder.getInt64Ty();
      break;
    case BuiltinTy::f32:
      Result = Builder.getFloatTy();
      break;
    case BuiltinTy::f64:
      Result = Builder.getDoubleTy();
      break;
    case BuiltinTy::Bool:
      Result = Builder.getInt1Ty();
      break;
    case BuiltinTy::Char:
      Result = Builder.getInt8Ty();
      break;
    case BuiltinTy::String:
      Result = Builder.getPtrTy();
      break;
    case BuiltinTy::Null:
      Result = Builder.getVoidTy();
      break;
    case BuiltinTy::Range:
      // Range as {i64, i64}
      Result = llvm::StructType::get(
          Context, {Builder.getInt64Ty(), Builder.getInt64Ty()});
      break;
    }
  } else if (auto *TT = llvm::dyn_cast<TupleTy>(T)) {
    std::vector<llvm::Type *> ElemTypes;
    for (const auto &Elem : TT->getElementTys()) {
      ElemTypes.push_back(getLLVMType(Elem));
    }
    Result = llvm::StructType::get(Context, ElemTypes);
  } else if (auto *AT = llvm::dyn_cast<ArrayTy>(T)) {
    // Array as slice: { T*, i64 }
    llvm::Type *ElemTy = getLLVMType(AT->getContainedTy());
    Result = llvm::StructType::get(
        Context, {ElemTy->getPointerTo(), Builder.getInt64Ty()});
  } else if (llvm::isa<PtrTy>(T)) {
    Result = Builder.getPtrTy();
  } else if (llvm::isa<RefTy>(T)) {
    Result = Builder.getPtrTy();
  } else if (auto *AT = llvm::dyn_cast<AdtTy>(T)) {
    std::string Name = AT->getId();

    // Attempt monomorphization if generic
    if (auto *D = AT->getDecl()) {
      if (D->hasTypeArgs() && !CurrentSubs.empty()) {
        std::vector<TypeRef> Args;
        bool AllFound = true;
        for (const auto &Param : D->getTypeArgs()) {
          auto It = CurrentSubs.find(Param.get());
          if (It != CurrentSubs.end()) {
            Args.push_back(It->second);
          } else {
            AllFound = false;
            break;
          }
        }
        if (AllFound) {
          Name = generateMonomorphizedName(Name, Args);
        }
      }
    }

    auto StructIt = StructTypes.find(Name);
    if (StructIt != StructTypes.end() && !StructIt->second->isOpaque()) {
      Result = StructIt->second;
    } else {
      if (auto *S = llvm::dyn_cast<StructDecl>(AT->getDecl())) {
        Result = getOrCreateStructType(S);
      } else if (auto *En = llvm::dyn_cast<EnumDecl>(AT->getDecl())) {
        Result = getOrCreateEnumType(En);
      } else {
        // Fallback to forward declaration
        if (StructIt != StructTypes.end()) {
          Result = StructIt->second;
        } else {
          Result = llvm::StructType::create(Context, Name);
          StructTypes[Name] = llvm::cast<llvm::StructType>(Result);
        }
      }
    }
  } else if (auto *ApT = llvm::dyn_cast<AppliedTy>(T)) {
    // If we have current substitutions, try to substitute!
    if (!CurrentSubs.empty()) {
      TypeRef Ref(const_cast<Type *>(T), SrcSpan(SrcLocation{}, SrcLocation{}));
      TypeRef SubRef = substituteType(Ref, CurrentSubs);

      if (SubRef.getPtr() != T) {
        return getLLVMType(SubRef);
      }
    }

    // Look up monomorphized name
    std::string BaseName;
    if (auto *AdtBase = llvm::dyn_cast<AdtTy>(ApT->getBase().getPtr())) {
      BaseName = AdtBase->getId();
    }
    std::string MonoName = generateMonomorphizedName(BaseName, ApT->getArgs());
    auto StructIt = StructTypes.find(MonoName);
    if (StructIt != StructTypes.end()) {
      Result = StructIt->second;
    } else {
      Result = llvm::StructType::create(Context, MonoName);
      StructTypes[MonoName] = llvm::cast<llvm::StructType>(Result);
    }
  } else if (auto *FT = llvm::dyn_cast<FunTy>(T)) {
    std::vector<llvm::Type *> ParamTypes;
    for (const auto &P : FT->getParamTys()) {
      ParamTypes.push_back(getLLVMType(P));
    }
    auto *RetTy = getLLVMType(FT->getReturnTy());
    Result = llvm::FunctionType::get(RetTy, ParamTypes, false)->getPointerTo();
  } else if (auto *GT = llvm::dyn_cast<GenericTy>(T)) {
    auto It = CurrentSubs.find(GT->getDecl());
    if (It != CurrentSubs.end()) {
      Result = getLLVMType(It->second);
    } else {
      Result = Builder.getVoidTy();
    }
  } else {
    // Default to void for unknown types
    Result = Builder.getVoidTy();
  }

  if (!Result) {
    Result = Builder.getVoidTy();
  }
  
  if (!IsGenericDependent) {
    TypeCache[T] = Result;
  }
  return Result;
}

llvm::StructType *CodeGen::getOrCreateStructType(const StructDecl *S) {
  const std::string &Name = S->getId();
  auto It = StructTypes.find(Name);
  if (It != StructTypes.end() && !It->second->isOpaque())
    return It->second;

  std::vector<llvm::Type *> FieldTypes;
  unsigned Idx = 0;
  for (const auto &F : S->getFields()) {
    FieldTypes.push_back(getLLVMType(F->getType()));
    FieldIndices[Name][F->getId()] = Idx++;
  }

  llvm::StructType *ST;
  if (It != StructTypes.end()) {
    ST = It->second;
    ST->setBody(FieldTypes);
  } else {
    ST = llvm::StructType::create(Context, FieldTypes, Name);
    StructTypes[Name] = ST;
  }
  return ST;
}

llvm::StructType *
CodeGen::getOrCreateStructType(const std::string &Name,
                               const std::vector<TypeRef> &FieldTypes) {
  auto It = StructTypes.find(Name);
  if (It != StructTypes.end() && !It->second->isOpaque())
    return It->second;

  std::vector<llvm::Type *> LLVMTypes;
  for (const auto &T : FieldTypes) {
    LLVMTypes.push_back(getLLVMType(T));
  }

  llvm::StructType *ST;
  if (It != StructTypes.end()) {
    ST = It->second;
    ST->setBody(LLVMTypes);
  } else {
    ST = llvm::StructType::create(Context, LLVMTypes, Name);
    StructTypes[Name] = ST;
  }
  return ST;
}

llvm::StructType *CodeGen::getOrCreateEnumType(const EnumDecl *E) {
  const std::string &Name = E->getId();
  auto It = StructTypes.find(Name);
  if (It != StructTypes.end() && !It->second->isOpaque())
    return It->second;

  // Find largest payload size
  uint64_t MaxPayloadSize = 0;
  unsigned Discriminant = 0;
  for (const auto &V : E->getVariants()) {
    VariantDiscriminants[Name][V->getId()] = Discriminant++;
    if (V->hasPayload()) {
      llvm::Type *PayloadTy = getLLVMType(V->getPayloadType());
      VariantPayloadTypes[Name][V->getId()] = PayloadTy;
      uint64_t Size = getTypeSize(PayloadTy);
      MaxPayloadSize = std::max(MaxPayloadSize, Size);
    }
  }

  // Enum layout: { i32 discriminant, [max_size x i8] }
  std::vector<llvm::Type *> Members = {Builder.getInt32Ty()};
  if (MaxPayloadSize > 0) {
    Members.push_back(
        llvm::ArrayType::get(Builder.getInt8Ty(), MaxPayloadSize));
  }

  llvm::StructType *ST;
  if (It != StructTypes.end()) {
    ST = It->second;
    ST->setBody(Members);
  } else {
    ST = llvm::StructType::create(Context, Members, Name);
    StructTypes[Name] = ST;
  }
  return ST;
}

llvm::StructType *
CodeGen::getOrCreateEnumType(const std::string &Name,
                             const std::vector<VariantDecl *> &Variants) {
  auto It = StructTypes.find(Name);
  if (It != StructTypes.end() && !It->second->isOpaque())
    return It->second;

  uint64_t MaxPayloadSize = 0;
  unsigned Discriminant = 0;
  for (const auto *V : Variants) {
    VariantDiscriminants[Name][V->getId()] = Discriminant++;
    if (V->hasPayload()) {
      llvm::Type *PayloadTy = getLLVMType(V->getPayloadType());
      VariantPayloadTypes[Name][V->getId()] = PayloadTy;
      uint64_t Size = getTypeSize(PayloadTy);
      MaxPayloadSize = std::max(MaxPayloadSize, Size);
    }
  }

  std::vector<llvm::Type *> Members = {Builder.getInt32Ty()};
  if (MaxPayloadSize > 0) {
    Members.push_back(
        llvm::ArrayType::get(Builder.getInt8Ty(), MaxPayloadSize));
  }

  llvm::StructType *ST;
  if (It != StructTypes.end()) {
    ST = It->second;
    ST->setBody(Members);
  } else {
    ST = llvm::StructType::create(Context, Members, Name);
    StructTypes[Name] = ST;
  }
  return ST;
}

uint64_t CodeGen::getTypeSize(llvm::Type *T) {
  if (T->isIntegerTy())
    return T->getIntegerBitWidth() / 8;
  if (T->isFloatTy())
    return 4;
  if (T->isDoubleTy())
    return 8;
  if (T->isPointerTy())
    return 8;
  if (auto *ST = llvm::dyn_cast<llvm::StructType>(T)) {
    uint64_t Size = 0;
    for (unsigned I = 0; I < ST->getNumElements(); ++I) {
      Size += getTypeSize(ST->getElementType(I));
    }
    return Size;
  }
  if (auto *AT = llvm::dyn_cast<llvm::ArrayType>(T)) {
    return AT->getNumElements() * getTypeSize(AT->getElementType());
  }
  return 8; // Default
}

//===----------------------------------------------------------------------===//
// Helpers
//===----------------------------------------------------------------------===//

llvm::AllocaInst *CodeGen::createEntryBlockAlloca(llvm::Function *Fn,
                                                  const std::string &Name,
                                                  llvm::Type *Ty) {
  llvm::IRBuilder<> TmpBuilder(&Fn->getEntryBlock(),
                               Fn->getEntryBlock().begin());
  return TmpBuilder.CreateAlloca(Ty, nullptr, Name);
}

std::string CodeGen::generateTempVar() {
  return "tmp_" + std::to_string(TmpVarCounter++);
}

bool CodeGen::hasTerminator() const {
  return Builder.GetInsertBlock()->getTerminator() != nullptr;
}

llvm::Value *CodeGen::loadValue(llvm::Value *Ptr, llvm::Type *Ty) {
  return Builder.CreateLoad(Ty, Ptr);
}

void CodeGen::storeValue(llvm::Value *Val, llvm::Value *Ptr) {
  Builder.CreateStore(Val, Ptr);
}

void CodeGen::declarePrintln() {
  // Declare printf for println support
  auto *PrintfTy =
      llvm::FunctionType::get(Builder.getInt32Ty(), {Builder.getPtrTy()}, true);
  llvm::FunctionCallee PrintfCallee =
      Module.getOrInsertFunction("printf", PrintfTy);
  if (auto *F = llvm::dyn_cast<llvm::Function>(PrintfCallee.getCallee())) {
    PrintFn = F;
  }
}
