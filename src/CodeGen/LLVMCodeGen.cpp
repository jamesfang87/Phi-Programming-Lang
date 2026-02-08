#include "CodeGen/LLVMCodeGen.hpp"

#include <llvm/ADT/TypeSwitch.h>
#include <llvm/IR/Constants.h>
#include <llvm/IR/DerivedTypes.h>
#include <llvm/IR/Verifier.h>
#include <llvm/IR/Verifier.h>
#include <llvm/TargetParser/Host.h>

#include <system_error>

using namespace phi;

#include <llvm/Support/raw_ostream.h>

//===----------------------------------------------------------------------===//
// Constructor & Main Entry Points
//===----------------------------------------------------------------------===//

LLVMCodeGen::LLVMCodeGen(std::vector<ModuleDecl *> Mods, std::string_view SourcePath)
    : Ast(std::move(Mods)), SourcePath(SourcePath), Context(), Builder(Context),
      Module(std::string(SourcePath), Context) {
  Module.setTargetTriple(llvm::sys::getDefaultTargetTriple());
}

void LLVMCodeGen::generate() {
  // Phase 1: Discover all generic instantiations
  discoverInstantiations();

  // Phase 2: Monomorphize generic types and functions
  monomorphize();

  // Phase 3: Desugar method calls to functi  monomorphize();
  desugar();
  
  // Phase 4: IR Generation
  for (auto *M : Ast) {
    codegenModule(M);
  }

  // Final Phase: Generate bodies for monomorphized functions
  generateMonomorphizedBodies();
}

void LLVMCodeGen::outputIR(const std::string &Filename) {
  std::error_code EC;
  llvm::raw_fd_ostream File(Filename, EC);
  if (EC)
    throw std::runtime_error("Could not open file: " + EC.message());
  Module.print(File, nullptr);
}

//===----------------------------------------------------------------------===//
// Type Conversion
//===----------------------------------------------------------------------===//

llvm::Type *LLVMCodeGen::getLLVMType(TypeRef T) {
  return getLLVMType(T.getPtr());
}

llvm::Type *LLVMCodeGen::getLLVMType(const Type *T) {
  // Check substitution map for generic types
  if (auto *GT = llvm::dyn_cast<GenericTy>(T)) {
    auto It = CurrentSubs.find(GT->getDecl());
    if (It != CurrentSubs.end()) {
      return getLLVMType(It->second);
    }
  }

  // Check cache first
  auto It = TypeCache.find(T);
  if (It != TypeCache.end())
    return It->second;

  if (!T) return Builder.getVoidTy();

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
      Result = llvm::StructType::get(Context, {Builder.getInt64Ty(), Builder.getInt64Ty()});
      break;
    }
  } else if (auto *TT = llvm::dyn_cast<TupleTy>(T)) {
    std::vector<llvm::Type *> ElemTypes;
    for (const auto &Elem : TT->getElementTys()) {
      ElemTypes.push_back(getLLVMType(Elem));
    }
    Result = llvm::StructType::get(Context, ElemTypes);
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
            AllFound = false; break;
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

  TypeCache[T] = Result;
  return Result;
}

llvm::StructType *LLVMCodeGen::getOrCreateStructType(const StructDecl *S) {
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

llvm::StructType *LLVMCodeGen::getOrCreateStructType(
    const std::string &Name, const std::vector<TypeRef> &FieldTypes) {
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

llvm::StructType *LLVMCodeGen::getOrCreateEnumType(const EnumDecl *E) {
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
    Members.push_back(llvm::ArrayType::get(Builder.getInt8Ty(), MaxPayloadSize));
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

llvm::StructType *LLVMCodeGen::getOrCreateEnumType(
    const std::string &Name, const std::vector<VariantDecl *> &Variants) {
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
    Members.push_back(llvm::ArrayType::get(Builder.getInt8Ty(), MaxPayloadSize));
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

uint64_t LLVMCodeGen::getTypeSize(llvm::Type *T) {
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
// Phase 1: Discovery
//===----------------------------------------------------------------------===//

void LLVMCodeGen::discoverInstantiations() {
  for (auto *M : Ast) {
    discoverInModule(M);
  }
}

void LLVMCodeGen::discoverInModule(ModuleDecl *M) {
  for (auto &Item : M->getItems()) {
    if (auto *F = llvm::dyn_cast<FunDecl>(Item.get())) {
      discoverInFunction(F);
    } else if (auto *S = llvm::dyn_cast<StructDecl>(Item.get())) {
      for (auto &Method : S->getMethods()) {
        discoverInMethod(Method.get());
      }
    } else if (auto *E = llvm::dyn_cast<EnumDecl>(Item.get())) {
      for (auto &Method : E->getMethods()) {
        discoverInMethod(Method.get());
      }
    }
  }
}

void LLVMCodeGen::discoverInFunction(FunDecl *F) {
  discoverInBlock(&F->getBody());
}

void LLVMCodeGen::discoverInMethod(MethodDecl *M) {
  discoverInBlock(&M->getBody());
}

void LLVMCodeGen::discoverInBlock(Block *B) {
  for (auto &S : B->getStmts()) {
    discoverInStmt(S.get());
  }
}

void LLVMCodeGen::discoverInStmt(Stmt *S) {
  if (auto *DS = llvm::dyn_cast<DeclStmt>(S)) {
    if (DS->getDecl().hasInit()) {
      discoverInExpr(&DS->getDecl().getInit());
    }
  } else if (auto *RS = llvm::dyn_cast<ReturnStmt>(S)) {
    if (RS->hasExpr()) {
      discoverInExpr(&RS->getExpr());
    }
  } else if (auto *IS = llvm::dyn_cast<IfStmt>(S)) {
    discoverInExpr(&IS->getCond());
    discoverInBlock(&IS->getThen());
    if (IS->hasElse()) {
      discoverInBlock(&IS->getElse());
    }
  } else if (auto *WS = llvm::dyn_cast<WhileStmt>(S)) {
    discoverInExpr(&WS->getCond());
    discoverInBlock(&WS->getBody());
  } else if (auto *FS = llvm::dyn_cast<ForStmt>(S)) {
    discoverInExpr(&FS->getRange());
    discoverInBlock(&FS->getBody());
  } else if (auto *ES = llvm::dyn_cast<ExprStmt>(S)) {
    discoverInExpr(&ES->getExpr());
  }
}

void LLVMCodeGen::discoverInExpr(Expr *E) {
  if (!E) return;

  if (auto *AI = llvm::dyn_cast<AdtInit>(E)) {
    // Check if this is a generic type instantiation
    if (AI->hasTypeArgs()) {
      if (auto *Decl = AI->getDecl()) {
        recordInstantiation(Decl, AI->getTypeArgs());
      }
    }
    for (auto &Init : AI->getInits()) {
      discoverInExpr(Init->getInitValue());
    }
  } else if (auto *FC = llvm::dyn_cast<FunCallExpr>(E)) {
    if (FC->hasTypeArgs() && FC->getDecl()) {
      recordInstantiation(FC->getDecl(), FC->getTypeArgs());
    }
    for (auto &Arg : FC->getArgs()) {
      discoverInExpr(Arg.get());
    }
  } else if (auto *MC = llvm::dyn_cast<MethodCallExpr>(E)) {
    discoverInExpr(MC->getBase());
    for (auto &Arg : MC->getArgs()) {
      discoverInExpr(Arg.get());
    }
  } else if (auto *BO = llvm::dyn_cast<BinaryOp>(E)) {
    discoverInExpr(&BO->getLhs());
    discoverInExpr(&BO->getRhs());
  } else if (auto *UO = llvm::dyn_cast<UnaryOp>(E)) {
    discoverInExpr(&UO->getOperand());
  } else if (auto *FA = llvm::dyn_cast<FieldAccessExpr>(E)) {
    discoverInExpr(FA->getBase());
  } else if (auto *ME = llvm::dyn_cast<MatchExpr>(E)) {
    discoverInExpr(ME->getScrutinee());
    for (auto &Arm : ME->getArms()) {
      discoverInBlock(Arm.Body.get());
    }
  } else if (auto *IE = llvm::dyn_cast<IndexExpr>(E)) {
    discoverInExpr(IE->getBase());
    discoverInExpr(IE->getIndex());
  } else if (auto *TL = llvm::dyn_cast<TupleLiteral>(E)) {
    for (auto &Elem : TL->getElements()) {
      discoverInExpr(Elem.get());
    }
  }
}

void LLVMCodeGen::recordInstantiation(const NamedDecl *Decl,
                                      const std::vector<TypeRef> &TypeArgs) {
  bool HasTypeArgs = false;
  if (auto *Item = llvm::dyn_cast<ItemDecl>(Decl)) {
    HasTypeArgs = Item->hasTypeArgs();
  } else if (auto *Method = llvm::dyn_cast<MethodDecl>(Decl)) {
    HasTypeArgs = Method->hasTypeArgs();
  }

  if (!HasTypeArgs)
    return;
  
  TypeInstantiation TI{Decl, TypeArgs};
  if (MonomorphizedNames.find(TI) == MonomorphizedNames.end() &&
      PendingInstantiations.find(TI) == PendingInstantiations.end()) {
    PendingInstantiations.insert(TI);
  }
}

//===----------------------------------------------------------------------===//
// Phase 2: Monomorphization
//===----------------------------------------------------------------------===//

void LLVMCodeGen::monomorphize() {
  while (!PendingInstantiations.empty()) {
    auto It = PendingInstantiations.begin();
    TypeInstantiation TI = *It;
    PendingInstantiations.erase(It);
    monomorphizeDecl(TI);
  }
}

void LLVMCodeGen::monomorphizeDecl(const TypeInstantiation &TI) {
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

void LLVMCodeGen::monomorphizeStruct(const StructDecl *S,
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
void LLVMCodeGen::monomorphizeEnum(const EnumDecl *E,
                                   const std::vector<TypeRef> &TypeArgs) {
  std::string MonoName = generateMonomorphizedName(E->getId(), TypeArgs);
  llvm::errs() << "DEBUG: monomorphizeEnum MonoName='" << MonoName << "'\n";
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
    Members.push_back(llvm::ArrayType::get(Builder.getInt8Ty(), MaxPayloadSize));
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

void LLVMCodeGen::monomorphizeMethod(const MethodDecl *M,
                                     const std::vector<TypeRef> &TypeArgs) {
  // Methods use the parent's type args + their own (if any).
  // For now, simpler case: methods on generic structs, but method itself not generic additional args?
  // The user said: "Structs/Enums, Functions, and Methods are the only decls in this language which can have generic types."
  
  // If the method belongs to a generic type, TypeArgs here are the type args for the STRUCT/ENUM.
  // We need to generate a name that includes these args.
  
  std::string ParentName = M->getParent()->getId();
  std::string MonoParentName = generateMonomorphizedName(ParentName, TypeArgs);
  std::string MonoMethodName = MonoParentName + "_" + M->getId();
  
  // Note: we track methods by the method decl + type args of the parent
  // If the method ALSO has type args, that's a separate level of instantiation we likely fall into via discoverInExpr
  
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
  auto *Fn = llvm::Function::Create(FnTy, llvm::Function::ExternalLinkage, MonoMethodName, Module);

  unsigned Idx = 0;
  for (auto &Arg : Fn->args()) {
    Arg.setName(M->getParams()[Idx++]->getId());
  }

  MonomorphizedMethodQueue.push_back({M, TypeArgs, Fn});
}

void LLVMCodeGen::monomorphizeFunction(const FunDecl *F,
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
  auto *Fn = llvm::Function::Create(FnTy, llvm::Function::ExternalLinkage, MonoName, Module);

  // Name parameters
  unsigned Idx = 0;
  for (auto &Arg : Fn->args()) {
    Arg.setName(F->getParams()[Idx++]->getId());
  }

  MonomorphizedFunctionQueue.push_back({F, TypeArgs, Fn});
}

TypeRef LLVMCodeGen::substituteType(TypeRef T, const SubstitutionMap &Subs) {
  const Type *Ptr = T.getPtr();
  
  if (auto *GT = llvm::dyn_cast<GenericTy>(Ptr)) {
    auto It = Subs.find(GT->getDecl());
    if (It != Subs.end()) {
      return It->second;
    }
    return T;
  }
  
  if (auto *ApT = llvm::dyn_cast<AppliedTy>(Ptr)) {
    std::vector<TypeRef> SubArgs;
    for (const auto &Arg : ApT->getArgs()) {
      SubArgs.push_back(substituteType(Arg, Subs));
    }
    // Record potential new instantiation
    if (auto *AdtBase = llvm::dyn_cast<AdtTy>(ApT->getBase().getPtr())) {
      if (AdtBase->getDecl()) {
        recordInstantiation(AdtBase->getDecl(), SubArgs);
      }
    }
    return T; // Return original for now, lookup will use MonomorphizedNames
  }
  
  if (auto *PT = llvm::dyn_cast<PtrTy>(Ptr)) {
    return TypeRef(new PtrTy(substituteType(PT->getPointee(), Subs)), T.getSpan());
  }
  
  if (auto *RT = llvm::dyn_cast<RefTy>(Ptr)) {
    return TypeRef(new RefTy(substituteType(RT->getPointee(), Subs)), T.getSpan());
  }
  
  if (auto *TT = llvm::dyn_cast<TupleTy>(Ptr)) {
    std::vector<TypeRef> SubElems;
    for (const auto &Elem : TT->getElementTys()) {
      SubElems.push_back(substituteType(Elem, Subs));
    }
    return TypeRef(new TupleTy(SubElems), T.getSpan());
  }
  
  return T;
}

LLVMCodeGen::SubstitutionMap
LLVMCodeGen::buildSubstitutionMap(const FunDecl *Decl,
                                  const std::vector<TypeRef> &TypeArgs) {
  SubstitutionMap Subs;
  const auto &TypeParams = Decl->getTypeArgs();
  for (size_t I = 0; I < TypeParams.size() && I < TypeArgs.size(); ++I) {
    Subs.insert({TypeParams[I].get(), TypeArgs[I]});
  }
  return Subs;
}

LLVMCodeGen::SubstitutionMap
LLVMCodeGen::buildSubstitutionMap(const MethodDecl *Decl,
                                  const std::vector<TypeRef> &TypeArgs) {
  SubstitutionMap Subs;
  const auto &TypeParams = Decl->getTypeArgs();
  for (size_t I = 0; I < TypeParams.size() && I < TypeArgs.size(); ++I) {
    Subs.insert({TypeParams[I].get(), TypeArgs[I]});
  }
  return Subs;
}

LLVMCodeGen::SubstitutionMap
LLVMCodeGen::buildSubstitutionMap(const AdtDecl *Decl,
                                  const std::vector<TypeRef> &TypeArgs) {
  SubstitutionMap Subs;
  const auto &TypeParams = Decl->getTypeArgs();
  for (size_t I = 0; I < TypeParams.size() && I < TypeArgs.size(); ++I) {
    Subs.insert({TypeParams[I].get(), TypeArgs[I]});
  }
  return Subs;
}

std::string LLVMCodeGen::generateMonomorphizedName(
    const std::string &BaseName, const std::vector<TypeRef> &TypeArgs) {
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

//===----------------------------------------------------------------------===//
// Phase 3: Desugaring
//===----------------------------------------------------------------------===//

void LLVMCodeGen::desugar() {
  for (auto *M : Ast) {
    desugarModule(M);
  }
}

void LLVMCodeGen::desugarModule(ModuleDecl *M) {
  for (auto &Item : M->getItems()) {
    if (auto *F = llvm::dyn_cast<FunDecl>(Item.get())) {
      desugarFunction(F);
    } else if (auto *S = llvm::dyn_cast<StructDecl>(Item.get())) {
      for (auto &Method : S->getMethods()) {
        desugarMethod(Method.get());
      }
    } else if (auto *E = llvm::dyn_cast<EnumDecl>(Item.get())) {
      for (auto &Method : E->getMethods()) {
        desugarMethod(Method.get());
      }
    }
  }
}

void LLVMCodeGen::desugarFunction(FunDecl *F) {
  desugarBlock(&F->getBody());
}

void LLVMCodeGen::desugarMethod(MethodDecl *M) {
  desugarBlock(&M->getBody());
}

void LLVMCodeGen::desugarBlock(Block *B) {
  for (auto &S : B->getStmts()) {
    desugarStmt(S.get());
  }
}

void LLVMCodeGen::desugarStmt(Stmt *S) {
  if (auto *DS = llvm::dyn_cast<DeclStmt>(S)) {
    if (DS->getDecl().hasInit()) {
      desugarExpr(&DS->getDecl().getInit());
    }
  } else if (auto *RS = llvm::dyn_cast<ReturnStmt>(S)) {
    if (RS->hasExpr()) {
      desugarExpr(&RS->getExpr());
    }
  } else if (auto *IS = llvm::dyn_cast<IfStmt>(S)) {
    desugarExpr(&IS->getCond());
    desugarBlock(&IS->getThen());
    if (IS->hasElse()) {
      desugarBlock(&IS->getElse());
    }
  } else if (auto *WS = llvm::dyn_cast<WhileStmt>(S)) {
    desugarExpr(&WS->getCond());
    desugarBlock(&WS->getBody());
  } else if (auto *FS = llvm::dyn_cast<ForStmt>(S)) {
    desugarExpr(&FS->getRange());
    desugarBlock(&FS->getBody());
  } else if (auto *ES = llvm::dyn_cast<ExprStmt>(S)) {
    desugarExpr(&ES->getExpr());
  }
}

void LLVMCodeGen::desugarExpr(Expr *E) {
  if (!E) return;

  // Recursively process child expressions first
  if (auto *AI = llvm::dyn_cast<AdtInit>(E)) {
    for (auto &Init : AI->getInits()) {
      desugarExpr(Init->getInitValue());
    }
  } else if (auto *FC = llvm::dyn_cast<FunCallExpr>(E)) {
    for (auto &Arg : FC->getArgs()) {
      desugarExpr(Arg.get());
    }
  } else if (auto *MC = llvm::dyn_cast<MethodCallExpr>(E)) {
    desugarExpr(MC->getBase());
    for (auto &Arg : MC->getArgs()) {
      desugarExpr(Arg.get());
    }
    // Note: actual method->function transformation happens in codegen
  } else if (auto *BO = llvm::dyn_cast<BinaryOp>(E)) {
    desugarExpr(&BO->getLhs());
    desugarExpr(&BO->getRhs());
  } else if (auto *UO = llvm::dyn_cast<UnaryOp>(E)) {
    desugarExpr(&UO->getOperand());
  } else if (auto *FA = llvm::dyn_cast<FieldAccessExpr>(E)) {
    desugarExpr(FA->getBase());
  } else if (auto *ME = llvm::dyn_cast<MatchExpr>(E)) {
    desugarExpr(ME->getScrutinee());
    for (auto &Arm : ME->getArms()) {
      desugarBlock(Arm.Body.get());
      if (Arm.Return) desugarExpr(Arm.Return);
    }
  } else if (auto *IE = llvm::dyn_cast<IndexExpr>(E)) {
    desugarExpr(IE->getBase());
    desugarExpr(IE->getIndex());
  } else if (auto *TL = llvm::dyn_cast<TupleLiteral>(E)) {
    for (auto &Elem : TL->getElements()) {
      desugarExpr(Elem.get());
    }
  }
}

//===----------------------------------------------------------------------===//
// Helpers
//===----------------------------------------------------------------------===//

llvm::AllocaInst *LLVMCodeGen::createEntryBlockAlloca(llvm::Function *Fn,
                                                      const std::string &Name,
                                                      llvm::Type *Ty) {
  llvm::IRBuilder<> TmpBuilder(&Fn->getEntryBlock(),
                               Fn->getEntryBlock().begin());
  return TmpBuilder.CreateAlloca(Ty, nullptr, Name);
}

std::string LLVMCodeGen::generateTempVar() {
  return "tmp_" + std::to_string(TmpVarCounter++);
}

bool LLVMCodeGen::hasTerminator() const {
  return Builder.GetInsertBlock()->getTerminator() != nullptr;
}

llvm::Value *LLVMCodeGen::getLValuePtr(Expr *E) {
  if (auto *DR = llvm::dyn_cast<DeclRefExpr>(E)) {
    auto It = NamedValues.find(DR->getDecl());
    if (It != NamedValues.end())
      return It->second;
  } else if (auto *FA = llvm::dyn_cast<FieldAccessExpr>(E)) {
    llvm::Value *BasePtr = getLValuePtr(FA->getBase());
    if (!BasePtr)
      BasePtr = codegenExpr(FA->getBase());
    
    const FieldDecl *Field = FA->getField();
    std::string StructName;
    if (auto *AT = llvm::dyn_cast<AdtTy>(FA->getBase()->getType().getPtr())) {
      StructName = AT->getId();
    }
    
    auto It = FieldIndices.find(StructName);
    if (It != FieldIndices.end()) {
      auto FieldIt = It->second.find(Field->getId());
      if (FieldIt != It->second.end()) {
        llvm::Type *StructTy = StructTypes[StructName];
        return Builder.CreateStructGEP(StructTy, BasePtr, FieldIt->second);
      }
    }
  } else if (auto *IE = llvm::dyn_cast<IndexExpr>(E)) {
    llvm::Value *BasePtr = getLValuePtr(IE->getBase());
    if (!BasePtr)
      return nullptr;
    llvm::Value *Idx = codegenExpr(IE->getIndex());
    llvm::Type *BaseTy = getLLVMType(IE->getBase()->getType());
    return Builder.CreateGEP(BaseTy, BasePtr, {Builder.getInt32(0), Idx});
  }
  return nullptr;
}

llvm::Value *LLVMCodeGen::loadValue(llvm::Value *Ptr, llvm::Type *Ty) {
  return Builder.CreateLoad(Ty, Ptr);
}

void LLVMCodeGen::storeValue(llvm::Value *Val, llvm::Value *Ptr) {
  Builder.CreateStore(Val, Ptr);
}

void LLVMCodeGen::declarePrintln() {
  // Declare printf for println support
  auto *PrintfTy = llvm::FunctionType::get(
      Builder.getInt32Ty(), {Builder.getPtrTy()}, true);
  llvm::FunctionCallee PrintfCallee = Module.getOrInsertFunction("printf", PrintfTy);
  if (auto *F = llvm::dyn_cast<llvm::Function>(PrintfCallee.getCallee())) {
    PrintFn = F;
  }
}

llvm::Value *LLVMCodeGen::generatePrintlnCall(FunCallExpr *Call) {
  std::vector<llvm::Value *> Args;
  
  if (Call->getArgs().empty()) {
    // Just print newline
    llvm::Value *Fmt = Builder.CreateGlobalStringPtr("\n");
    Args.push_back(Fmt);
  } else {
    // Simple format based on first arg type
    auto &FirstArg = Call->getArgs()[0];
    llvm::Value *ArgVal = codegenExpr(FirstArg.get());
    TypeRef ArgTy = FirstArg->getType();
    
    std::string Fmt;
    if (ArgTy.getPtr()->isBuiltin()) {
      auto *BT = llvm::cast<BuiltinTy>(ArgTy.getPtr());
      switch (BT->getBuiltinKind()) {
      case BuiltinTy::i32:
      case BuiltinTy::i64:
        Fmt = "%lld\n";
        break;
      case BuiltinTy::f32:
      case BuiltinTy::f64:
        Fmt = "%f\n";
        break;
      case BuiltinTy::Bool:
        Fmt = "%d\n";
        break;
      case BuiltinTy::String:
        Fmt = "%s\n";
        break;
      default:
        Fmt = "%d\n";
      }
    } else {
      Fmt = "%d\n";
    }
    
    Args.push_back(Builder.CreateGlobalStringPtr(Fmt));
    Args.push_back(ArgVal);
  }
  
  return Builder.CreateCall(PrintFn, Args);
}

//===----------------------------------------------------------------------===//
// Phase 4: Declaration Codegen
//===----------------------------------------------------------------------===//

void LLVMCodeGen::codegenModule(ModuleDecl *M) {
  // Pass 1: Declare struct types
  declareStructTypes(M);

  // Pass 2: Declare enum types
  declareEnumTypes(M);

  // Pass 3: Declare function signatures
  declareFunctions(M);

  // Pass 4: Generate function bodies
  generateFunctionBodies(M);
}

void LLVMCodeGen::declareStructTypes(ModuleDecl *M) {
  for (auto &Item : M->getItems()) {
    if (auto *S = llvm::dyn_cast<StructDecl>(Item.get())) {
      if (!S->hasTypeArgs()) {
        getOrCreateStructType(S);
      }
    }
  }
}

void LLVMCodeGen::declareEnumTypes(ModuleDecl *M) {
  for (auto &Item : M->getItems()) {
    if (auto *E = llvm::dyn_cast<EnumDecl>(Item.get())) {
      if (!E->hasTypeArgs()) {
        getOrCreateEnumType(E);
      }
    }
  }
}

void LLVMCodeGen::declareFunctions(ModuleDecl *M) {
  for (auto &Item : M->getItems()) {
    if (auto *F = llvm::dyn_cast<FunDecl>(Item.get())) {
      if (!F->hasTypeArgs()) {
        codegenFunctionDecl(F);
      }
    } else if (auto *S = llvm::dyn_cast<StructDecl>(Item.get())) {
      if (!S->hasTypeArgs()) {
        for (auto &Method : S->getMethods()) {
          std::string MangledName = S->getId() + "_" + Method->getId();
          codegenMethodDecl(Method.get(), MangledName);
        }
      }
    } else if (auto *E = llvm::dyn_cast<EnumDecl>(Item.get())) {
      if (!E->hasTypeArgs()) {
        for (auto &Method : E->getMethods()) {
          std::string MangledName = E->getId() + "_" + Method->getId();
          codegenMethodDecl(Method.get(), MangledName);
        }
      }
    }
  }
}

void LLVMCodeGen::generateFunctionBodies(ModuleDecl *M) {
  for (auto &Item : M->getItems()) {
    if (auto *F = llvm::dyn_cast<FunDecl>(Item.get())) {
      if (!F->hasTypeArgs()) {
        auto It = Functions.find(F);
        if (It != Functions.end()) {
          codegenFunctionBody(F, It->second);
        }
      }
    } else if (auto *S = llvm::dyn_cast<StructDecl>(Item.get())) {
      if (!S->hasTypeArgs()) {
        for (auto &Method : S->getMethods()) {
          auto It = Methods.find(Method.get());
          if (It != Methods.end()) {
            codegenMethodBody(Method.get(), It->second);
          }
        }
      }
    } else if (auto *E = llvm::dyn_cast<EnumDecl>(Item.get())) {
      if (!E->hasTypeArgs()) {
        for (auto &Method : E->getMethods()) {
          auto It = Methods.find(Method.get());
          if (It != Methods.end()) {
            codegenMethodBody(Method.get(), It->second);
          }
        }
      }
    }
  }
}

llvm::Function *LLVMCodeGen::codegenFunctionDecl(FunDecl *F) {
  // Build parameter types
  std::vector<llvm::Type *> ParamTypes;
  for (const auto &P : F->getParams()) {
    ParamTypes.push_back(getLLVMType(P->getType()));
  }

  // Get return type
  llvm::Type *RetTy = getLLVMType(F->getReturnType());

  // Create function type
  auto *FnTy = llvm::FunctionType::get(RetTy, ParamTypes, false);

  // Create function
  auto *Fn = llvm::Function::Create(FnTy, llvm::Function::ExternalLinkage,
                                    F->getId(), Module);

  // Name parameters
  unsigned Idx = 0;
  for (auto &Arg : Fn->args()) {
    Arg.setName(F->getParams()[Idx++]->getId());
  }

  Functions[F] = Fn;
  return Fn;
}

llvm::Function *LLVMCodeGen::codegenMethodDecl(MethodDecl *M,
                                                const std::string &MangledName) {
  // Build parameter types (including 'this' as first param)
  std::vector<llvm::Type *> ParamTypes;
  for (const auto &P : M->getParams()) {
    ParamTypes.push_back(getLLVMType(P->getType()));
  }

  // Get return type
  llvm::Type *RetTy = getLLVMType(M->getReturnType());

  // Create function type
  auto *FnTy = llvm::FunctionType::get(RetTy, ParamTypes, false);

  // Create function
  auto *Fn = llvm::Function::Create(FnTy, llvm::Function::ExternalLinkage,
                                    MangledName, Module);

  // Name parameters
  unsigned Idx = 0;
  for (auto &Arg : Fn->args()) {
    Arg.setName(M->getParams()[Idx++]->getId());
  }

  Methods[M] = Fn;
  return Fn;
}

void LLVMCodeGen::codegenFunctionBody(FunDecl *F, llvm::Function *Fn) {
  CurrentFunction = Fn;

  // Create entry block
  auto *EntryBB = llvm::BasicBlock::Create(Context, "entry", Fn);
  Builder.SetInsertPoint(EntryBB);

  // Handle println intrinsic
  if (F->getId().find("println") == 0 && Fn->arg_size() == 1) {
    // Get printf
    llvm::FunctionType *PrintfTy = llvm::FunctionType::get(
        Builder.getInt32Ty(), {Builder.getPtrTy()}, true);
    llvm::FunctionCallee Printf = Module.getOrInsertFunction("printf", PrintfTy);

    // Get argument
    llvm::Argument *Arg = Fn->getArg(0);
    llvm::Type *ArgTy = Arg->getType();

    // Determine format string
    std::string Fmt;
    if (ArgTy->isIntegerTy(32)) {
      Fmt = "%d\n";
    } else if (ArgTy->isDoubleTy()) {
      Fmt = "%f\n";
    } else if (ArgTy->isIntegerTy(1)) {
       Fmt = "%d\n";
    } else {
      Fmt = "Unknown type\n";
    }

    // Create global string
    llvm::Value *FmtStr = Builder.CreateGlobalStringPtr(Fmt, "fmt");

    // Call printf
    Builder.CreateCall(Printf, {FmtStr, Arg});
    Builder.CreateRetVoid();
    CurrentFunction = nullptr;
    return;
  }

  // Clear named values for this function
  NamedValues.clear();

  // Allocate and store parameters
  unsigned Idx = 0;
  for (auto &Arg : Fn->args()) {
    auto *P = F->getParams()[Idx++].get();
    llvm::Type *Ty = Arg.getType();
    auto *Alloca = createEntryBlockAlloca(Fn, P->getId(), Ty);
    Builder.CreateStore(&Arg, Alloca);
    NamedValues[P] = Alloca;
  }

  // Generate body
  codegenBlock(&F->getBody());

  // Add default return if no terminator
  if (!hasTerminator()) {
    llvm::Type *RetTy = Fn->getReturnType();
    if (RetTy->isVoidTy()) {
      Builder.CreateRetVoid();
    } else {
      Builder.CreateRet(llvm::Constant::getNullValue(RetTy));
    }
  }

  CurrentFunction = nullptr;
}

void LLVMCodeGen::codegenMethodBody(MethodDecl *M, llvm::Function *Fn) {
  CurrentFunction = Fn;

  auto *EntryBB = llvm::BasicBlock::Create(Context, "entry", Fn);
  Builder.SetInsertPoint(EntryBB);

  NamedValues.clear();

  unsigned Idx = 0;
  for (auto &Arg : Fn->args()) {
    auto *P = M->getParams()[Idx++].get();
    llvm::Type *Ty = Arg.getType();
    auto *Alloca = createEntryBlockAlloca(Fn, P->getId(), Ty);
    Builder.CreateStore(&Arg, Alloca);
    NamedValues[P] = Alloca;
  }

  codegenBlock(&M->getBody());

  if (!hasTerminator()) {
    llvm::Type *RetTy = Fn->getReturnType();
    if (RetTy->isVoidTy()) {
      Builder.CreateRetVoid();
    } else {
      Builder.CreateRet(llvm::Constant::getNullValue(RetTy));
    }
  }

  CurrentFunction = nullptr;
}

//===----------------------------------------------------------------------===//
// Phase 4: Statement Codegen
//===----------------------------------------------------------------------===//

void LLVMCodeGen::codegenBlock(Block *B) {
  for (auto &S : B->getStmts()) {
    if (hasTerminator())
      break;
    codegenStmt(S.get());
  }
}

void LLVMCodeGen::codegenStmt(Stmt *S) {
  if (auto *DS = llvm::dyn_cast<DeclStmt>(S)) {
    codegenDeclStmt(DS);
  } else if (auto *RS = llvm::dyn_cast<ReturnStmt>(S)) {
    codegenReturnStmt(RS);
  } else if (auto *IS = llvm::dyn_cast<IfStmt>(S)) {
    codegenIfStmt(IS);
  } else if (auto *WS = llvm::dyn_cast<WhileStmt>(S)) {
    codegenWhileStmt(WS);
  } else if (auto *FS = llvm::dyn_cast<ForStmt>(S)) {
    codegenForStmt(FS);
  } else if (auto *BS = llvm::dyn_cast<BreakStmt>(S)) {
    codegenBreakStmt(BS);
  } else if (auto *CS = llvm::dyn_cast<ContinueStmt>(S)) {
    codegenContinueStmt(CS);
  } else if (auto *ES = llvm::dyn_cast<ExprStmt>(S)) {
    codegenExprStmt(ES);
  }
}

void LLVMCodeGen::codegenDeclStmt(DeclStmt *S) {
  auto &Decl = S->getDecl();
  llvm::Type *Ty = getLLVMType(Decl.getType());
  auto *Alloca = createEntryBlockAlloca(CurrentFunction, Decl.getId(), Ty);
  NamedValues[&Decl] = Alloca;

  if (Decl.hasInit()) {
    llvm::Value *InitVal = codegenExpr(&Decl.getInit());
    Builder.CreateStore(InitVal, Alloca);
  }
}

void LLVMCodeGen::codegenReturnStmt(ReturnStmt *S) {
  if (S->hasExpr()) {
    llvm::Value *RetVal = codegenExpr(&S->getExpr());
    Builder.CreateRet(RetVal);
  } else {
    Builder.CreateRetVoid();
  }
}

void LLVMCodeGen::codegenIfStmt(IfStmt *S) {
  llvm::Value *Cond = codegenExpr(&S->getCond());
  
  // Convert to i1 if needed
  if (!Cond->getType()->isIntegerTy(1)) {
    Cond = Builder.CreateICmpNE(Cond, 
        llvm::Constant::getNullValue(Cond->getType()), "ifcond");
  }

  auto *ThenBB = llvm::BasicBlock::Create(Context, "then", CurrentFunction);
  auto *ElseBB = llvm::BasicBlock::Create(Context, "else", CurrentFunction);
  auto *MergeBB = llvm::BasicBlock::Create(Context, "ifcont", CurrentFunction);

  Builder.CreateCondBr(Cond, ThenBB, ElseBB);

  // Then block
  Builder.SetInsertPoint(ThenBB);
  codegenBlock(&S->getThen());
  if (!hasTerminator())
    Builder.CreateBr(MergeBB);

  // Else block
  Builder.SetInsertPoint(ElseBB);
  if (S->hasElse()) {
    codegenBlock(&S->getElse());
  }
  if (!hasTerminator())
    Builder.CreateBr(MergeBB);

  Builder.SetInsertPoint(MergeBB);
}

void LLVMCodeGen::codegenWhileStmt(WhileStmt *S) {
  auto *CondBB = llvm::BasicBlock::Create(Context, "while.cond", CurrentFunction);
  auto *BodyBB = llvm::BasicBlock::Create(Context, "while.body", CurrentFunction);
  auto *AfterBB = llvm::BasicBlock::Create(Context, "while.end", CurrentFunction);

  Builder.CreateBr(CondBB);

  // Condition
  Builder.SetInsertPoint(CondBB);
  llvm::Value *Cond = codegenExpr(&S->getCond());
  if (!Cond->getType()->isIntegerTy(1)) {
    Cond = Builder.CreateICmpNE(Cond,
        llvm::Constant::getNullValue(Cond->getType()), "whilecond");
  }
  Builder.CreateCondBr(Cond, BodyBB, AfterBB);

  // Body
  Builder.SetInsertPoint(BodyBB);
  LoopStack.emplace_back(CondBB, AfterBB);
  codegenBlock(&S->getBody());
  LoopStack.pop_back();
  if (!hasTerminator())
    Builder.CreateBr(CondBB);

  Builder.SetInsertPoint(AfterBB);
}

void LLVMCodeGen::codegenForStmt(ForStmt *S) {
  // For now, simplified: for x in start..end
  auto *InitBB = llvm::BasicBlock::Create(Context, "for.init", CurrentFunction);
  auto *CondBB = llvm::BasicBlock::Create(Context, "for.cond", CurrentFunction);
  auto *BodyBB = llvm::BasicBlock::Create(Context, "for.body", CurrentFunction);
  auto *IncBB = llvm::BasicBlock::Create(Context, "for.inc", CurrentFunction);
  auto *AfterBB = llvm::BasicBlock::Create(Context, "for.end", CurrentFunction);

  Builder.CreateBr(InitBB);

  // Init: allocate loop variable
  Builder.SetInsertPoint(InitBB);
  auto &LoopVar = S->getLoopVar();
  llvm::Type *Ty = getLLVMType(LoopVar.getType());
  auto *LoopVarAlloca = createEntryBlockAlloca(CurrentFunction, LoopVar.getId(), Ty);
  NamedValues[&LoopVar] = LoopVarAlloca;

  // Get range bounds
  if (auto *RL = llvm::dyn_cast<RangeLiteral>(&S->getRange())) {
    llvm::Value *Start = codegenExpr(&RL->getStart());
    llvm::Value *End = codegenExpr(&RL->getEnd());
    Builder.CreateStore(Start, LoopVarAlloca);
    Builder.CreateBr(CondBB);

    // Condition
    Builder.SetInsertPoint(CondBB);
    llvm::Value *Current = Builder.CreateLoad(Ty, LoopVarAlloca);
    llvm::Value *Cond = Builder.CreateICmpSLT(Current, End, "forcond");
    Builder.CreateCondBr(Cond, BodyBB, AfterBB);

    // Body
    Builder.SetInsertPoint(BodyBB);
    LoopStack.emplace_back(IncBB, AfterBB);
    codegenBlock(&S->getBody());
    LoopStack.pop_back();
    if (!hasTerminator())
      Builder.CreateBr(IncBB);

    // Increment
    Builder.SetInsertPoint(IncBB);
    Current = Builder.CreateLoad(Ty, LoopVarAlloca);
    llvm::Value *Next = Builder.CreateAdd(Current, Builder.getInt64(1), "inc");
    Builder.CreateStore(Next, LoopVarAlloca);
    Builder.CreateBr(CondBB);
  } else {
    // Fallback: just execute body once
    Builder.CreateBr(CondBB);
    Builder.SetInsertPoint(CondBB);
    Builder.CreateBr(BodyBB);
    Builder.SetInsertPoint(BodyBB);
    codegenBlock(&S->getBody());
    if (!hasTerminator())
      Builder.CreateBr(AfterBB);
  }

  Builder.SetInsertPoint(AfterBB);
}

void LLVMCodeGen::codegenBreakStmt(BreakStmt * /*S*/) {
  if (!LoopStack.empty()) {
    Builder.CreateBr(LoopStack.back().AfterBB);
  }
}

void LLVMCodeGen::codegenContinueStmt(ContinueStmt * /*S*/) {
  if (!LoopStack.empty()) {
    Builder.CreateBr(LoopStack.back().CondBB);
  }
}

void LLVMCodeGen::codegenExprStmt(ExprStmt *S) {
  codegenExpr(&S->getExpr());
}

//===----------------------------------------------------------------------===//
// Phase 4: Expression Codegen
//===----------------------------------------------------------------------===//

llvm::Value *LLVMCodeGen::codegenExpr(Expr *E) {
  if (!E) return llvm::Constant::getNullValue(Builder.getInt32Ty());

  if (auto *IL = llvm::dyn_cast<IntLiteral>(E))
    return codegenIntLiteral(IL);
  if (auto *FL = llvm::dyn_cast<FloatLiteral>(E))
    return codegenFloatLiteral(FL);
  if (auto *BL = llvm::dyn_cast<BoolLiteral>(E))
    return codegenBoolLiteral(BL);
  if (auto *SL = llvm::dyn_cast<StrLiteral>(E))
    return codegenStrLiteral(SL);
  if (auto *CL = llvm::dyn_cast<CharLiteral>(E))
    return codegenCharLiteral(CL);
  if (auto *TL = llvm::dyn_cast<TupleLiteral>(E))
    return codegenTupleLiteral(TL);
  if (auto *DR = llvm::dyn_cast<DeclRefExpr>(E))
    return codegenDeclRef(DR);
  if (auto *FC = llvm::dyn_cast<FunCallExpr>(E))
    return codegenFunCall(FC);
  if (auto *MC = llvm::dyn_cast<MethodCallExpr>(E))
    return codegenMethodCall(MC);
  if (auto *BO = llvm::dyn_cast<BinaryOp>(E))
    return codegenBinaryOp(BO);
  if (auto *UO = llvm::dyn_cast<UnaryOp>(E))
    return codegenUnaryOp(UO);
  if (auto *AI = llvm::dyn_cast<AdtInit>(E))
    return codegenAdtInit(AI);
  if (auto *FA = llvm::dyn_cast<FieldAccessExpr>(E))
    return codegenFieldAccess(FA);
  if (auto *IE = llvm::dyn_cast<IndexExpr>(E))
    return codegenIndexExpr(IE);
  if (auto *ME = llvm::dyn_cast<MatchExpr>(E))
    return codegenMatchExpr(ME);
  if (auto *IC = llvm::dyn_cast<IntrinsicCall>(E))
    return codegenIntrinsicCall(IC);

  return llvm::Constant::getNullValue(Builder.getInt32Ty());
}

llvm::Value *LLVMCodeGen::codegenIntLiteral(IntLiteral *E) {
  int64_t Val = E->getValue();
  // Check type and return appropriate sized int
  if (E->getType().getPtr()) {
    auto *BT = llvm::dyn_cast<BuiltinTy>(E->getType().getPtr());
    if (BT) {
      switch (BT->getBuiltinKind()) {
      case BuiltinTy::i8:
      case BuiltinTy::u8:
        return Builder.getInt8(Val);
      case BuiltinTy::i16:
      case BuiltinTy::u16:
        return Builder.getInt16(Val);
      case BuiltinTy::i32:
      case BuiltinTy::u32:
        return Builder.getInt32(Val);
      default:
        break;
      }
    }
  }
  return Builder.getInt64(Val);
}

llvm::Value *LLVMCodeGen::codegenFloatLiteral(FloatLiteral *E) {
  double Val = E->getValue();
  if (E->getType().getPtr()) {
    auto *BT = llvm::dyn_cast<BuiltinTy>(E->getType().getPtr());
    if (BT && BT->getBuiltinKind() == BuiltinTy::f32) {
      return llvm::ConstantFP::get(Builder.getFloatTy(), Val);
    }
  }
  return llvm::ConstantFP::get(Builder.getDoubleTy(), Val);
}

llvm::Value *LLVMCodeGen::codegenBoolLiteral(BoolLiteral *E) {
  return Builder.getInt1(E->getValue());
}

llvm::Value *LLVMCodeGen::codegenStrLiteral(StrLiteral *E) {
  return Builder.CreateGlobalStringPtr(E->getValue());
}

llvm::Value *LLVMCodeGen::codegenCharLiteral(CharLiteral *E) {
  return Builder.getInt8(E->getValue());
}

llvm::Value *LLVMCodeGen::codegenTupleLiteral(TupleLiteral *E) {
  std::vector<llvm::Value *> Elements;
  std::vector<llvm::Type *> ElemTypes;
  for (auto &Elem : E->getElements()) {
    llvm::Value *V = codegenExpr(Elem.get());
    Elements.push_back(V);
    ElemTypes.push_back(V->getType());
  }

  llvm::StructType *TupleTy = llvm::StructType::get(Context, ElemTypes);
  llvm::Value *Tuple = llvm::UndefValue::get(TupleTy);
  for (unsigned I = 0; I < Elements.size(); ++I) {
    Tuple = Builder.CreateInsertValue(Tuple, Elements[I], I);
  }
  return Tuple;
}

llvm::Value *LLVMCodeGen::codegenDeclRef(DeclRefExpr *E) {
  auto It = NamedValues.find(E->getDecl());
  if (It != NamedValues.end()) {
    llvm::Type *Ty = getLLVMType(E->getType());
    return Builder.CreateLoad(Ty, It->second, E->getDecl()->getId());
  }
  return llvm::Constant::getNullValue(getLLVMType(E->getType()));
}

llvm::Value *LLVMCodeGen::codegenFunCall(FunCallExpr *E) {
  // Get function
  FunDecl *Callee = E->getDecl();
  if (!Callee)
    return llvm::Constant::getNullValue(getLLVMType(E->getType()));

  // Intercept println
  if (Callee->getId() == "println") {
      if (E->getArgs().empty()) return llvm::Constant::getNullValue(Builder.getVoidTy());
      
      llvm::Value *Val = codegenExpr(E->getArgs()[0].get());
      llvm::Type *ValTy = Val->getType();
      
      std::string Fmt;
      if (ValTy->isIntegerTy(32)) Fmt = "%d\n";
      else if (ValTy->isIntegerTy(64)) Fmt = "%ld\n";
      else if (ValTy->isDoubleTy()) Fmt = "%f\n";
      else if (ValTy->isIntegerTy(1)) Fmt = "%d\n"; // bool
      else if (ValTy->isPointerTy()) Fmt = "%s\n"; // string or other ptr
      else Fmt = "Unknown\n";
      
      llvm::Value *FmtStr = Builder.CreateGlobalStringPtr(Fmt, "fmt");
      
      llvm::FunctionType *PrintfTy = llvm::FunctionType::get(Builder.getInt32Ty(), {Builder.getPtrTy()}, true);
      llvm::FunctionCallee Printf = Module.getOrInsertFunction("printf", PrintfTy);
      return Builder.CreateCall(Printf, {FmtStr, Val});
  }

  llvm::Function *Fn = nullptr;
  
  // Try to resolve generic function
  if (E->hasTypeArgs()) {
    std::string MonoName = generateMonomorphizedName(Callee->getId(), E->getTypeArgs());
    Fn = Module.getFunction(MonoName);
  } else if (!Callee->getTypeArgs().empty()) {
    // If it's a generic function but no args provided, it might have been monomorphized already
    // This happens with inferred types or when called from another generic
    TypeInstantiation TI{Callee, {}}; // Fallback or search in MonomorphizedNames
    // For now try searching by ID in module if we can't find it in Functions cache
  }

  if (!Fn) {
    auto It = Functions.find(Callee);
    if (It != Functions.end()) {
      Fn = It->second;
    }
  }

  if (!Fn) {
    // Fallback: search by name
    Fn = Module.getFunction(Callee->getId());
  }

  if (!Fn) {
      return llvm::Constant::getNullValue(getLLVMType(E->getType()));
  }

  // Generate arguments
  std::vector<llvm::Value *> Args;
  for (auto &Arg : E->getArgs()) {
    Args.push_back(codegenExpr(Arg.get()));
  }

  return Builder.CreateCall(Fn, Args);
}

llvm::Value *LLVMCodeGen::codegenMethodCall(MethodCallExpr *E) {
  // Transform: obj.method(args) -> method(obj, args)
  MethodDecl *Method = &E->getMethod();
  
  llvm::Function *Fn = nullptr;
  if (E->hasTypeArgs() || !Method->getTypeArgs().empty()) {
     // For methods, desugar phase might have already handled this 
     // but if not, try resolving name
     std::string MonoName = Method->getParent()->getId() + "_" + Method->getId();
     if (E->hasTypeArgs()) {
         MonoName = generateMonomorphizedName(MonoName, E->getTypeArgs());
     }
     Fn = Module.getFunction(MonoName);
  }

  // Generic Struct Method Resolution
  if (!Fn && E->getBase()) {
      TypeRef BaseTy = E->getBase()->getType();
      while (true) {
          if (auto *RT = llvm::dyn_cast<RefTy>(BaseTy.getPtr())) BaseTy = RT->getPointee();
          else if (auto *PT = llvm::dyn_cast<PtrTy>(BaseTy.getPtr())) BaseTy = PT->getPointee();
          else break;
      }
      if (auto *AppTy = llvm::dyn_cast<AppliedTy>(BaseTy.getPtr())) {
          if (auto *Adt = llvm::dyn_cast<AdtTy>(AppTy->getBase().getPtr())) {
               std::string ParentName = Adt->getId();
               std::string MonoParent = generateMonomorphizedName(ParentName, AppTy->getArgs());
               std::string MonoMethod = MonoParent + "_" + Method->getId();
               Fn = Module.getFunction(MonoMethod);
          }
      }
  }

  if (!Fn) {
    auto It = Methods.find(Method);
    if (It != Methods.end()) {
      Fn = It->second;
    }
  }

  if (!Fn)
    return llvm::Constant::getNullValue(getLLVMType(E->getType()));

  std::vector<llvm::Value *> Args;
  // First argument is the receiver (self)
  llvm::Value *BaseVal = nullptr;
  bool PassAddress = false;
  
  if (!Method->getParams().empty()) {
       TypeRef ParamTy = Method->getParams()[0]->getType();
       if (llvm::dyn_cast<RefTy>(ParamTy.getPtr()) || llvm::dyn_cast<PtrTy>(ParamTy.getPtr())) {
           // Param expects pointer/reference
           // Check if Base is Value type
           TypeRef BaseTy = E->getBase()->getType();
           if (!llvm::dyn_cast<RefTy>(BaseTy.getPtr()) && !llvm::dyn_cast<PtrTy>(BaseTy.getPtr())) {
               PassAddress = true;
           }
       }
  }

  if (PassAddress) {
       // Optimization: If DeclRef, get Alloca directly
       if (auto *DRE = llvm::dyn_cast<DeclRefExpr>(E->getBase())) {
           auto It = NamedValues.find(DRE->getDecl());
           if (It != NamedValues.end()) {
               BaseVal = It->second;
           }
       }
       
       if (!BaseVal) {
           // Create temporary for RValue
           llvm::Value *Val = codegenExpr(E->getBase());
           if (Val) {
               llvm::Type *Ty = Val->getType();
               auto *Temp = createEntryBlockAlloca(CurrentFunction, "temp_this", Ty);
               Builder.CreateStore(Val, Temp);
               BaseVal = Temp;
           }
       }
  } else {
       BaseVal = codegenExpr(E->getBase());
  }

  if (BaseVal) Args.push_back(BaseVal);
  else Args.push_back(llvm::UndefValue::get(getLLVMType(E->getBase()->getType()))); // Should not happen

  // Remaining arguments
  for (auto &Arg : E->getArgs()) {
    Args.push_back(codegenExpr(Arg.get()));
  }

  return Builder.CreateCall(Fn, Args);
}

llvm::Value *LLVMCodeGen::codegenBinaryOp(BinaryOp *E) {
  // Handle assignment specially
  if (E->getOp() == TokenKind::Equals) {
    llvm::Value *Rhs = codegenExpr(&E->getRhs());
    llvm::Value *LhsPtr = getLValuePtr(&E->getLhs());
    if (LhsPtr) {
      Builder.CreateStore(Rhs, LhsPtr);
      return Rhs;
    }
    return Rhs;
  }

  llvm::Value *Lhs = codegenExpr(&E->getLhs());
  llvm::Value *Rhs = codegenExpr(&E->getRhs());

  llvm::Type *Ty = Lhs->getType();
  bool IsFloat = Ty->isFloatingPointTy();

  switch (E->getOp()) {
  case TokenKind::Plus:
    return IsFloat ? Builder.CreateFAdd(Lhs, Rhs, "add")
                   : Builder.CreateAdd(Lhs, Rhs, "add");
  case TokenKind::Minus:
    return IsFloat ? Builder.CreateFSub(Lhs, Rhs, "sub")
                   : Builder.CreateSub(Lhs, Rhs, "sub");
  case TokenKind::Star:
    return IsFloat ? Builder.CreateFMul(Lhs, Rhs, "mul")
                   : Builder.CreateMul(Lhs, Rhs, "mul");
  case TokenKind::Slash:
    return IsFloat ? Builder.CreateFDiv(Lhs, Rhs, "div")
                   : Builder.CreateSDiv(Lhs, Rhs, "div");
  case TokenKind::Percent:
    return Builder.CreateSRem(Lhs, Rhs, "mod");
  case TokenKind::DoubleEquals:
    return IsFloat ? Builder.CreateFCmpOEQ(Lhs, Rhs, "eq")
                   : Builder.CreateICmpEQ(Lhs, Rhs, "eq");
  case TokenKind::BangEquals:
    return IsFloat ? Builder.CreateFCmpONE(Lhs, Rhs, "ne")
                   : Builder.CreateICmpNE(Lhs, Rhs, "ne");
  case TokenKind::OpenCaret:
    return IsFloat ? Builder.CreateFCmpOLT(Lhs, Rhs, "lt")
                   : Builder.CreateICmpSLT(Lhs, Rhs, "lt");
  case TokenKind::LessEqual:
    return IsFloat ? Builder.CreateFCmpOLE(Lhs, Rhs, "le")
                   : Builder.CreateICmpSLE(Lhs, Rhs, "le");
  case TokenKind::CloseCaret:
    return IsFloat ? Builder.CreateFCmpOGT(Lhs, Rhs, "gt")
                   : Builder.CreateICmpSGT(Lhs, Rhs, "gt");
  case TokenKind::GreaterEqual:
    return IsFloat ? Builder.CreateFCmpOGE(Lhs, Rhs, "ge")
                   : Builder.CreateICmpSGE(Lhs, Rhs, "ge");
  case TokenKind::DoubleAmp:
    return Builder.CreateAnd(Lhs, Rhs, "and");
  case TokenKind::DoublePipe:
    return Builder.CreateOr(Lhs, Rhs, "or");
  default:
    return Lhs;
  }
}

llvm::Value *LLVMCodeGen::codegenUnaryOp(UnaryOp *E) {
  llvm::Value *Operand = codegenExpr(&E->getOperand());

  switch (E->getOp()) {
  case TokenKind::Minus:
    if (Operand->getType()->isFloatingPointTy())
      return Builder.CreateFNeg(Operand, "neg");
    return Builder.CreateNeg(Operand, "neg");
  case TokenKind::Bang:
    return Builder.CreateNot(Operand, "not");
  case TokenKind::Amp:
    // Return address of operand
    return getLValuePtr(&E->getOperand());
  case TokenKind::Star:
    // Load from pointer
    return Builder.CreateLoad(getLLVMType(E->getType()), Operand, "deref");
  default:
    return Operand;
  }
}

llvm::Value *LLVMCodeGen::codegenAdtInit(AdtInit *E) {
  const AdtDecl *Decl = E->getDecl();
  if (!Decl) {
    if (auto *AT = llvm::dyn_cast<AdtTy>(E->getType().getPtr())) {
      Decl = AT->getDecl();
    }
  }

  if (!Decl)
    return llvm::Constant::getNullValue(getLLVMType(E->getType()));

  if (auto *S = llvm::dyn_cast<StructDecl>(Decl)) {
    return codegenStructInit(E, S);
  } else if (auto *En = llvm::dyn_cast<EnumDecl>(Decl)) {
    return codegenEnumInit(E, En);
  }
  return llvm::Constant::getNullValue(getLLVMType(E->getType()));
}

llvm::Value *LLVMCodeGen::codegenStructInit(AdtInit *E, const StructDecl *S) {
  std::string StructName = S->getId();
  if (!S->getTypeArgs().empty() && E->hasTypeArgs()) {
    StructName = generateMonomorphizedName(S->getId(), E->getTypeArgs());
  }

  llvm::StructType *StructTy = StructTypes[StructName];
  if (!StructTy)
    return llvm::Constant::getNullValue(Builder.getInt32Ty());

  // Create alloca for struct
  auto *Alloca = createEntryBlockAlloca(CurrentFunction, generateTempVar(), StructTy);

  // Initialize fields
  for (auto &Init : E->getInits()) {
    const std::string &FieldName = Init->getId();
    auto FieldIt = FieldIndices[StructName].find(FieldName);
    if (FieldIt != FieldIndices[StructName].end()) {
      unsigned FieldIdx = FieldIt->second;
      llvm::Value *FieldPtr = Builder.CreateStructGEP(StructTy, Alloca, FieldIdx);
      llvm::Value *Val = codegenExpr(Init->getInitValue());
      Builder.CreateStore(Val, FieldPtr);
    }
  }

  return Builder.CreateLoad(StructTy, Alloca);
}

llvm::Value *LLVMCodeGen::codegenEnumInit(AdtInit *E, const EnumDecl *En) {
  std::string EnumName = En->getId();
  if (!En->getTypeArgs().empty() && E->hasTypeArgs()) {
    EnumName = generateMonomorphizedName(En->getId(), E->getTypeArgs());
  }

  llvm::StructType *EnumTy = StructTypes[EnumName];
  if (!EnumTy)
    return llvm::Constant::getNullValue(Builder.getInt32Ty());

  // Get variant name
  std::string VariantName;
  if (E->hasActiveVariant()) {
    VariantName = E->getActiveVariantName();
  } else if (!E->isAnonymous()) {
    VariantName = E->getTypeName();
  } else if (!E->getInits().empty()) {
    VariantName = E->getInits()[0]->getId();
  }

  auto DiscIt = VariantDiscriminants[EnumName].find(VariantName);
  if (DiscIt == VariantDiscriminants[EnumName].end())
    return llvm::Constant::getNullValue(Builder.getInt32Ty());

  unsigned Discriminant = DiscIt->second;

  // Create alloca for enum
  auto *Alloca = createEntryBlockAlloca(CurrentFunction, generateTempVar(), EnumTy);

  // Store discriminant
  auto *DiscPtr = Builder.CreateStructGEP(EnumTy, Alloca, 0);
  Builder.CreateStore(Builder.getInt32(Discriminant), DiscPtr);

  // Store payload if present
  auto PayloadIt = VariantPayloadTypes[EnumName].find(VariantName);
  if (PayloadIt != VariantPayloadTypes[EnumName].end() &&
      !E->getInits().empty()) {
    llvm::Type *PayloadTy = PayloadIt->second;
    auto *PayloadPtr = Builder.CreateStructGEP(EnumTy, Alloca, 1);
    auto *TypedPayloadPtr = Builder.CreateBitCast(PayloadPtr, PayloadTy->getPointerTo());
    llvm::Value *PayloadVal = codegenExpr(E->getInits()[0]->getInitValue());
    Builder.CreateStore(PayloadVal, TypedPayloadPtr);
  }

  return Builder.CreateLoad(EnumTy, Alloca);
}

llvm::Value *LLVMCodeGen::codegenFieldAccess(FieldAccessExpr *E) {
  llvm::Value *BasePtr = getLValuePtr(E->getBase());
  if (!BasePtr) {
    // Base is not an lvalue, compute it
    BasePtr = codegenExpr(E->getBase());
    // For value types, we need to store to a temp first
    llvm::Type *BaseTy = getLLVMType(E->getBase()->getType());
    auto *Alloca = createEntryBlockAlloca(CurrentFunction, generateTempVar(), BaseTy);
    Builder.CreateStore(BasePtr, Alloca);
    BasePtr = Alloca;
  }

  std::string StructName;
  const Type *BaseTyPtr = E->getBase()->getType().getPtr();
  if (auto *RT = llvm::dyn_cast<RefTy>(BaseTyPtr)) {
    BaseTyPtr = RT->getPointee().getPtr();
  }
  
  if (auto *AT = llvm::dyn_cast<AdtTy>(BaseTyPtr)) {
    StructName = AT->getId();
  }

  const FieldDecl *Field = E->getField();
  auto It = FieldIndices.find(StructName);
  if (It != FieldIndices.end()) {
    auto FieldIt = It->second.find(Field->getId());
    if (FieldIt != It->second.end()) {
      llvm::Type *StructTy = StructTypes[StructName];
      auto *FieldPtr = Builder.CreateStructGEP(StructTy, BasePtr, FieldIt->second);
      return Builder.CreateLoad(getLLVMType(E->getType()), FieldPtr);
    }
  }

  return llvm::Constant::getNullValue(getLLVMType(E->getType()));
}

llvm::Value *LLVMCodeGen::codegenIndexExpr(IndexExpr *E) {
  llvm::Value *BasePtr = getLValuePtr(E->getBase());
  llvm::Value *Idx = codegenExpr(E->getIndex());
  if (!BasePtr)
    return llvm::Constant::getNullValue(getLLVMType(E->getType()));

  llvm::Type *BaseTy = getLLVMType(E->getBase()->getType());
  auto *ElemPtr = Builder.CreateGEP(BaseTy, BasePtr, {Builder.getInt32(0), Idx});
  return Builder.CreateLoad(getLLVMType(E->getType()), ElemPtr);
}

llvm::Value *LLVMCodeGen::codegenIntrinsicCall(IntrinsicCall *E) {
  switch (E->getIntrinsicKind()) {
  case IntrinsicCall::IntrinsicKind::Panic: {
    // Evaluate message
    llvm::Value *Msg = nullptr;
    if (!E->getArgs().empty()) {
      Msg = codegenExpr(E->getArgs()[0].get());
    }

    // Declare printf if not exists
    if (!PrintFn) declarePrintln();
    
    // Declare abort
    llvm::FunctionCallee AbortFn = Module.getOrInsertFunction("abort", 
        llvm::FunctionType::get(Builder.getVoidTy(), false));

    // Print "Panic: "
    llvm::Value *Prefix = Builder.CreateGlobalStringPtr("Panic: ");
    Builder.CreateCall(PrintFn, {Builder.CreateGlobalStringPtr("%s"), Prefix});

    // Print message if present
    if (Msg) {
      // Assuming generic print like println
       llvm::Type *MsgTy = Msg->getType();
       std::string Fmt;
       if (MsgTy->isIntegerTy(32)) Fmt = "%d\n";
       else if (MsgTy->isDoubleTy()) Fmt = "%f\n";
       else if (MsgTy->isPointerTy()) Fmt = "%s\n";
       else Fmt = "Unknown\n";
       
       llvm::Value *FmtStr = Builder.CreateGlobalStringPtr(Fmt);
       Builder.CreateCall(PrintFn, {FmtStr, Msg});
    } else {
       Builder.CreateCall(PrintFn, {Builder.CreateGlobalStringPtr("\n")});
    }

    // Call abort
    Builder.CreateCall(AbortFn);
    
    // Return undef as we are aborting
    return llvm::UndefValue::get(getLLVMType(E->getType()));
  }
  case IntrinsicCall::IntrinsicKind::Assert: {
    // Assert(cond, msg)
    if (E->getArgs().empty()) return llvm::Constant::getNullValue(Builder.getVoidTy());
    
    llvm::Value *Cond = codegenExpr(E->getArgs()[0].get());
    if (!Cond->getType()->isIntegerTy(1)) {
        Cond = Builder.CreateICmpNE(Cond, llvm::Constant::getNullValue(Cond->getType()));
    }

    llvm::BasicBlock *FailBB = llvm::BasicBlock::Create(Context, "assert.fail", CurrentFunction);
    llvm::BasicBlock *ContBB = llvm::BasicBlock::Create(Context, "assert.cont", CurrentFunction);

    Builder.CreateCondBr(Cond, ContBB, FailBB);

    // Fail Block
    Builder.SetInsertPoint(FailBB);
    
    // Declare printf/abort
    if (!PrintFn) declarePrintln();
    llvm::FunctionCallee AbortFn = Module.getOrInsertFunction("abort", 
        llvm::FunctionType::get(Builder.getVoidTy(), false));

    llvm::Value *Msg = Builder.CreateGlobalStringPtr("Assertion failed\n");
    if (E->getArgs().size() > 1) {
        // Evaluate custom message
        // Note: strictly this should be evaluated before branch? 
        // Or we evaluate it only on failure (lazy). 
        // C assert evaluates macro args? No, standard assert is eval'd before?
        // Actually usually assert(cond && "msg").
        // Here we evaluate in fail block.
        // But codegenExpr might generate side effects / code in current block?
        // `codegenExpr` appends to `Builder`'s current block.
        // So we are good.
        // Evaluate custom message for side effects (though likely none)
        codegenExpr(E->getArgs()[1].get());
        
        // Print user message... simplified for now, just print "Assertion failed"
        // If we want to print UserMsg we need format logic again
    }
    
    Builder.CreateCall(PrintFn, {Builder.CreateGlobalStringPtr("%s"), Msg});
    Builder.CreateCall(AbortFn);
    Builder.CreateUnreachable(); // Terminate fail block

    // Continue Block
    Builder.SetInsertPoint(ContBB);
    return llvm::Constant::getNullValue(Builder.getVoidTy());
  }
  case IntrinsicCall::IntrinsicKind::Unreachable: {
    Builder.CreateUnreachable();
    return llvm::UndefValue::get(getLLVMType(E->getType()));
  }
  case IntrinsicCall::IntrinsicKind::TypeOf: {
    // Compile-time only, or return string of type name?
    // For now return void/null
    return llvm::Constant::getNullValue(Builder.getVoidTy()); 
  }
  }
  return llvm::Constant::getNullValue(Builder.getVoidTy());
}

//===----------------------------------------------------------------------===//
// Phase 4: Match Expression Codegen
//===----------------------------------------------------------------------===//

llvm::Value *LLVMCodeGen::codegenMatchExpr(MatchExpr *E) {
  llvm::Value *Scrutinee = codegenExpr(E->getScrutinee());
  TypeRef ASTType = E->getScrutinee()->getType();
  while (true) {
      if (auto *RT = llvm::dyn_cast<RefTy>(ASTType.getPtr())) {
          ASTType = RT->getPointee();
      } else if (auto *PT = llvm::dyn_cast<PtrTy>(ASTType.getPtr())) {
          ASTType = PT->getPointee();
      } else {
          break;
      }
  }
  llvm::Type *ScrutineeTy = getLLVMType(ASTType);
  llvm::Value *ScrutineeAlloca = Scrutinee;

  // Handles pointer to struct (opaque)
  if (Scrutinee->getType()->isPointerTy()) {
       if (llvm::isa<llvm::StructType>(ScrutineeTy)) {
          ScrutineeAlloca = Scrutinee;
       } else {
          // If ScrutineeTy is NOT StructType (e.g. integer hidden in ptr? Unlikely).
          // Allow fallback.
          ScrutineeAlloca = createEntryBlockAlloca(CurrentFunction, "scrutinee", ScrutineeTy);
          Builder.CreateStore(Scrutinee, ScrutineeAlloca);
       }
  } else {
      ScrutineeAlloca = createEntryBlockAlloca(CurrentFunction, "scrutinee", ScrutineeTy);
      Builder.CreateStore(Scrutinee, ScrutineeAlloca);
  }

  auto *StartBB = Builder.GetInsertBlock();
  auto *MergeBB = llvm::BasicBlock::Create(Context, "match.end", CurrentFunction);
  auto *FailBB = llvm::BasicBlock::Create(Context, "match.fail", CurrentFunction);

  // PHI for result
  llvm::Type *ResultTy = getLLVMType(E->getType());
  llvm::PHINode *ResultPHI = nullptr;
  if (!ResultTy->isVoidTy()) {
    Builder.SetInsertPoint(MergeBB);
    ResultPHI = Builder.CreatePHI(ResultTy, E->getArms().size() + 1, "match.result");
  }

  // --- Attempt Switch Optimization ---
  bool CanUseSwitch = true;
  bool HasWildcard = false;
  
  // Check scrutinee type
  bool IsEnum = false;
  bool IsInteger = ScrutineeTy->isIntegerTy();
  std::string EnumName;
  
  if (auto *ST = llvm::dyn_cast<llvm::StructType>(ScrutineeTy)) {
    if (ST->hasName()) {
       EnumName = ST->getName().str();
       llvm::errs() << "DEBUG: codegenMatchExpr EnumName='" << EnumName << "'\n";
       if (VariantDiscriminants.count(EnumName)) {
           IsEnum = true;
       }
    }
  }

  if (!IsEnum && !IsInteger) CanUseSwitch = false;

  // Check arms
  for (size_t I = 0; I < E->getArms().size(); ++I) {
      if (HasWildcard) { 
          // Wildcard must be last
          CanUseSwitch = false; break; 
      }
      
      const auto &Arm = E->getArms()[I];
      if (Arm.Patterns.empty()) {
          HasWildcard = true; // Fallback "else"
          continue;
      }
      
      for (const auto &Pat : Arm.Patterns) {
          if (std::holds_alternative<PatternAtomics::Wildcard>(Pat)) {
              HasWildcard = true;
              // If mixed with other patterns or not last arm, abort switch
              if (Arm.Patterns.size() > 1 || I != E->getArms().size() - 1) {
                  CanUseSwitch = false;
              }
              break;
          }
          if (std::holds_alternative<PatternAtomics::Literal>(Pat)) {
              if (!IsInteger) { CanUseSwitch = false; break; }
          } else if (std::holds_alternative<PatternAtomics::Variant>(Pat)) {
              if (!IsEnum) { CanUseSwitch = false; break; }
          } else {
              CanUseSwitch = false; break;
          }
      }
      if (!CanUseSwitch) break;
  }

  if (CanUseSwitch && (IsEnum || IsInteger)) {
      Builder.SetInsertPoint(StartBB);
      
      llvm::Value *SwitchVal = nullptr;
      if (IsInteger) {
          SwitchVal = Builder.CreateLoad(ScrutineeTy, ScrutineeAlloca);
      } else {
          // Load discriminant
          auto *DiscPtr = Builder.CreateStructGEP(llvm::cast<llvm::StructType>(ScrutineeTy), ScrutineeAlloca, 0);
          SwitchVal = Builder.CreateLoad(Builder.getInt32Ty(), DiscPtr);
      }

      llvm::BasicBlock *DefaultBB = FailBB;
      
      // We will create the switch inst at the end
      
      // Generate bodies and Collect cases
      std::vector<std::pair<llvm::ConstantInt*, llvm::BasicBlock*>> Cases;
      
      for (const auto &Arm : E->getArms()) {
          llvm::BasicBlock *ArmBB = llvm::BasicBlock::Create(Context, "match.arm", CurrentFunction, FailBB);
          
          bool IsWildcardArm = false;
           // Add cases
          if (Arm.Patterns.empty()) {
             IsWildcardArm = true;
          } else {
              for (const auto &Pat : Arm.Patterns) {
                  if (std::holds_alternative<PatternAtomics::Wildcard>(Pat)) {
                      IsWildcardArm = true; 
                      continue;
                  }
                  
                  llvm::ConstantInt *CaseVal = nullptr;
                  std::string VariantName;
                  
                  if (auto *P = std::get_if<PatternAtomics::Literal>(&Pat)) {
                      // Integer literal
                      // We need to interpret the literal value. 
                      // PatternAtomics::Literal has an Expr. We need to evaluate it?
                      // Ideally patterns should have constant values resolved.
                      // codegenExpr on literal returns a ConstantInt if it is a literal.
                      // Let's rely on codegenExpr returning Constant.
                      llvm::Value *V = codegenExpr(P->Value.get());
                      if (auto *CI = llvm::dyn_cast<llvm::ConstantInt>(V)) {
                          CaseVal = CI;
                      } else {
                          CanUseSwitch = false; // Should not happen for int literal
                      }
                  } else if (auto *P = std::get_if<PatternAtomics::Variant>(&Pat)) {
                      VariantName = P->VariantName;
                      unsigned Disc = VariantDiscriminants[EnumName][VariantName];
                      CaseVal = Builder.getInt32(Disc);
                  }
                  
                  if (CaseVal) {
                      Cases.push_back({CaseVal, ArmBB});
                  }
              }
          }
          
          if (IsWildcardArm) {
              DefaultBB = ArmBB;
          }
          
          // Generate Body
          Builder.SetInsertPoint(ArmBB);
          
          // Pattern bindings extraction (for Enum Variants)
          if (IsEnum && !Arm.Patterns.empty()) {
               // Assuming single pattern for binding or multiple with same bindings?
               // If multiple patterns, we can't easily bind unless they are identical.
               // For now assume if Binding exists, there is only 1 pattern or strict logic.
               // Let's just look at the first pattern that is a Variant with vars.
               for (const auto &Pat : Arm.Patterns) {
                   if (auto *P = std::get_if<PatternAtomics::Variant>(&Pat)) {
                       if (!P->Vars.empty()) {
                           // Extract
                           auto PayloadIt = VariantPayloadTypes[EnumName].find(P->VariantName);
                           if (PayloadIt != VariantPayloadTypes[EnumName].end()) {
                               llvm::Type *PayloadTy = PayloadIt->second;
                               auto *PayloadPtr = Builder.CreateStructGEP(llvm::cast<llvm::StructType>(ScrutineeTy), ScrutineeAlloca, 1);
                               auto *TypedPayloadPtr = Builder.CreateBitCast(PayloadPtr, PayloadTy->getPointerTo());
                               llvm::Value *PayloadVal = Builder.CreateLoad(PayloadTy, TypedPayloadPtr);

                               if (!P->Vars.empty()) {
                                   auto &BoundVar = P->Vars[0];
                                   llvm::Type *VarTy = getLLVMType(BoundVar->getType());
                                   auto *VarAlloca = createEntryBlockAlloca(CurrentFunction, BoundVar->getId(), VarTy);
                                   Builder.CreateStore(PayloadVal, VarAlloca);
                                   NamedValues[BoundVar.get()] = VarAlloca;
                               }
                           }
                       }
                       // Only handle first pattern bindings for now (limitation of this patch)
                       break; 
                   }
               }
          }

          // Manually generate body to capture last expression result
          llvm::Value *BodyResult = nullptr;

          for (size_t i = 0; i < Arm.Body->getStmts().size(); ++i) {
             if (hasTerminator()) break;
             auto &S = Arm.Body->getStmts()[i];
             // Optimization: capture last expression result
             if (ResultPHI && i == Arm.Body->getStmts().size() - 1) {
                  if (auto *ES = llvm::dyn_cast<ExprStmt>(S.get())) {
                       BodyResult = codegenExpr(&ES->getExpr());
                       continue;
                  }
             }
             codegenStmt(S.get());
          }

          if (!hasTerminator()) {
             if (ResultPHI) {
                 llvm::Value *Result = BodyResult ? BodyResult : llvm::Constant::getNullValue(ResultPHI->getType());
                 if (Result->getType() != ResultPHI->getType()) {
                     Result = llvm::UndefValue::get(ResultPHI->getType());
                 }
                 ResultPHI->addIncoming(Result, Builder.GetInsertBlock());
             }
             Builder.CreateBr(MergeBB);
          }
      }
      
      // Emit Switch
      Builder.SetInsertPoint(StartBB);
      auto *SI = Builder.CreateSwitch(SwitchVal, DefaultBB, Cases.size());
      for (auto &Pair : Cases) {
          SI->addCase(Pair.first, Pair.second);
      }
      
      // Handle Failure (if DefaultBB is FailBB)
      if (DefaultBB == FailBB) {
          Builder.SetInsertPoint(FailBB);
          if (ResultPHI) ResultPHI->addIncoming(llvm::UndefValue::get(ResultTy), FailBB);
          Builder.CreateBr(MergeBB);
      }
      
      Builder.SetInsertPoint(MergeBB);
      if (ResultPHI) return ResultPHI;
      return llvm::Constant::getNullValue(Builder.getInt32Ty());
  }
  
  // --- End Switch Optimization (Fallback to linear check) ---

  // Generate arm blocks first (to allow forward jumps)
  auto &Arms = E->getArms();
  std::vector<llvm::BasicBlock *> ArmBBs;
  for (size_t I = 0; I < Arms.size(); ++I) {
    ArmBBs.push_back(llvm::BasicBlock::Create(Context, "match.arm", CurrentFunction, FailBB));
  }
  
  // Jump to first arm
  Builder.SetInsertPoint(StartBB);
  if (!ArmBBs.empty()) {
    Builder.CreateBr(ArmBBs[0]);
  } else {
    Builder.CreateBr(FailBB);
  }

  // Generate code for each arm
  for (size_t I = 0; I < Arms.size(); ++I) {
    auto *NextArmBB = (I + 1 < Arms.size()) ? ArmBBs[I + 1] : FailBB;

    Builder.SetInsertPoint(ArmBBs[I]);
    codegenMatchArm(Arms[I], ScrutineeAlloca, NextArmBB, MergeBB, ResultPHI);
  }

  // Handle failure case (no pattern matched)
  Builder.SetInsertPoint(FailBB);
  if (ResultPHI) {
    ResultPHI->addIncoming(llvm::UndefValue::get(ResultTy), FailBB);
  }
  Builder.CreateBr(MergeBB);

  Builder.SetInsertPoint(MergeBB);
  if (ResultPHI)
    return static_cast<llvm::Value *>(ResultPHI);
  return llvm::Constant::getNullValue(Builder.getInt32Ty());
}

llvm::Value *LLVMCodeGen::codegenMatchArm(const MatchExpr::Arm &Arm,
                                           llvm::Value *Scrutinee,
                                           llvm::BasicBlock *NextArmBB,
                                           llvm::BasicBlock *MergeBB,
                                           llvm::PHINode *ResultPHI) {
  auto *MatchBB = llvm::BasicBlock::Create(Context, "match.body", CurrentFunction, NextArmBB);

  // Pattern match - use first pattern (pattern alternations not yet supported)
  if (!Arm.Patterns.empty()) {
    matchPattern(Arm.Patterns[0], Scrutinee, MatchBB, NextArmBB);
  } else {
    // No patterns, unconditionally branch to match body
    Builder.CreateBr(MatchBB);
  }

  // Generate arm body
  Builder.SetInsertPoint(MatchBB);
  codegenBlock(Arm.Body.get());

  // Add result to PHI if needed
  if (ResultPHI && !hasTerminator()) {
    // Get last expression value from block
    llvm::Value *Result = llvm::Constant::getNullValue(ResultPHI->getType());
    if (!Arm.Body->getStmts().empty()) {
      auto &LastStmt = Arm.Body->getStmts().back();
      if (auto *ES = llvm::dyn_cast<ExprStmt>(LastStmt.get())) {
        Result = codegenExpr(&ES->getExpr());
      }
    }
    if (Result->getType() != ResultPHI->getType()) {
      Result = llvm::UndefValue::get(ResultPHI->getType());
    }
    ResultPHI->addIncoming(Result, Builder.GetInsertBlock());
  }

  if (!hasTerminator())
    Builder.CreateBr(MergeBB);

  return nullptr;
}

void LLVMCodeGen::matchPattern(const Pattern &Pat, llvm::Value *Scrutinee,
                                llvm::BasicBlock *SuccessBB, llvm::BasicBlock *FailBB) {
  std::visit([&](auto &&P) {
    using T = std::decay_t<decltype(P)>;
    if constexpr (std::is_same_v<T, PatternAtomics::Wildcard>) {
      matchWildcard(SuccessBB);
    } else if constexpr (std::is_same_v<T, PatternAtomics::Literal>) {
      matchLiteral(P, Scrutinee, SuccessBB, FailBB);
    } else if constexpr (std::is_same_v<T, PatternAtomics::Variant>) {
      matchVariant(P, Scrutinee, SuccessBB, FailBB);
    }
  }, Pat);
}

void LLVMCodeGen::matchWildcard(llvm::BasicBlock *SuccessBB) {
  Builder.CreateBr(SuccessBB);
}

void LLVMCodeGen::matchLiteral(const PatternAtomics::Literal &Lit,
                                llvm::Value *Scrutinee,
                                llvm::BasicBlock *SuccessBB,
                                llvm::BasicBlock *FailBB) {
  llvm::Value *PatVal = codegenExpr(Lit.Value.get());
  llvm::Type *ScrutineeTy = nullptr;
  if (auto *AllocaInst = llvm::dyn_cast<llvm::AllocaInst>(Scrutinee)) {
    ScrutineeTy = AllocaInst->getAllocatedType();
  } else {
    ScrutineeTy = PatVal->getType();
  }
  llvm::Value *ScrutineeVal = Builder.CreateLoad(ScrutineeTy, Scrutinee);
  llvm::Value *Cmp = Builder.CreateICmpEQ(ScrutineeVal, PatVal, "match.cmp");
  Builder.CreateCondBr(Cmp, SuccessBB, FailBB);
}

void LLVMCodeGen::matchVariant(const PatternAtomics::Variant &Var,
                                llvm::Value *Scrutinee,
                                llvm::BasicBlock *SuccessBB,
                                llvm::BasicBlock *FailBB) {
  // Get scrutinee type to find enum name
  llvm::Type *ScrutineeTy = nullptr;
  if (auto *AllocaInst = llvm::dyn_cast<llvm::AllocaInst>(Scrutinee)) {
    ScrutineeTy = AllocaInst->getAllocatedType();
  }
  if (!ScrutineeTy || !llvm::isa<llvm::StructType>(ScrutineeTy)) {
    Builder.CreateBr(FailBB);
    return;
  }

  auto *StructTy = llvm::cast<llvm::StructType>(ScrutineeTy);
  std::string EnumName = StructTy->hasName() ? StructTy->getName().str() : "";

  // Get discriminant for this variant
  auto EnumIt = VariantDiscriminants.find(EnumName);
  if (EnumIt == VariantDiscriminants.end()) {
    Builder.CreateBr(FailBB);
    return;
  }

  auto VarIt = EnumIt->second.find(Var.VariantName);
  if (VarIt == EnumIt->second.end()) {
    Builder.CreateBr(FailBB);
    return;
  }

  unsigned ExpectedDisc = VarIt->second;

  // Load discriminant from scrutinee
  auto *DiscPtr = Builder.CreateStructGEP(StructTy, Scrutinee, 0);
  auto *ActualDisc = Builder.CreateLoad(Builder.getInt32Ty(), DiscPtr);

  // Compare
  auto *Cmp = Builder.CreateICmpEQ(ActualDisc, Builder.getInt32(ExpectedDisc), "variant.cmp");

  // If pattern has bindings, extract payload
  if (!Var.Vars.empty()) {
    auto *ExtractBB = llvm::BasicBlock::Create(Context, "variant.extract", CurrentFunction, SuccessBB);
    Builder.CreateCondBr(Cmp, ExtractBB, FailBB);

    Builder.SetInsertPoint(ExtractBB);

    // Load and bind payload variables
    auto PayloadIt = VariantPayloadTypes[EnumName].find(Var.VariantName);
    if (PayloadIt != VariantPayloadTypes[EnumName].end()) {
      llvm::Type *PayloadTy = PayloadIt->second;
      auto *PayloadPtr = Builder.CreateStructGEP(StructTy, Scrutinee, 1);
      auto *TypedPayloadPtr = Builder.CreateBitCast(PayloadPtr, PayloadTy->getPointerTo());
      llvm::Value *PayloadVal = Builder.CreateLoad(PayloadTy, TypedPayloadPtr);

      // Bind to first variable
      if (!Var.Vars.empty()) {
        auto &BoundVar = Var.Vars[0];
        llvm::Type *VarTy = getLLVMType(BoundVar->getType());
        auto *VarAlloca = createEntryBlockAlloca(CurrentFunction, BoundVar->getId(), VarTy);
        Builder.CreateStore(PayloadVal, VarAlloca);
        NamedValues[BoundVar.get()] = VarAlloca;
      }
    }

    Builder.CreateBr(SuccessBB);
  } else {
    Builder.CreateCondBr(Cmp, SuccessBB, FailBB);
  }
}

void LLVMCodeGen::generateMonomorphizedBodies() {
  while (!MonomorphizedFunctionQueue.empty() || !MonomorphizedMethodQueue.empty()) {
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
    std::vector<MonomorphizedMethod> Methods = std::move(MonomorphizedMethodQueue);
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
} // End generateMonomorphizedBodies



 // namespace phi
