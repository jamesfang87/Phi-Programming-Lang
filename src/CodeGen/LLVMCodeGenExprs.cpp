#include "CodeGen/LLVMCodeGen.hpp"

#include <llvm/ADT/TypeSwitch.h>
#include <llvm/IR/Constants.h>
#include <llvm/IR/DerivedTypes.h>
#include <llvm/IR/Verifier.h>
#include <variant>

using namespace phi;

//===----------------------------------------------------------------------===//
// Phase 4: Expression Codegen
//===----------------------------------------------------------------------===//

llvm::Value *CodeGen::codegenExpr(Expr *E) {
  if (!E)
    return llvm::Constant::getNullValue(Builder.getInt32Ty());

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
  if (auto *IE = llvm::dyn_cast<TupleIndex>(E))
    return codegenTupleIndex(IE);
  if (auto *ME = llvm::dyn_cast<MatchExpr>(E))
    return codegenMatchExpr(ME);
  if (auto *IC = llvm::dyn_cast<IntrinsicCall>(E))
    return codegenIntrinsicCall(IC);

  if (auto *AE = llvm::dyn_cast<ArrayIndex>(E))
    return codegenArrayIndex(AE);
  if (auto *AL = llvm::dyn_cast<ArrayLiteral>(E))
    return codegenArrayLiteral(AL);

  return llvm::Constant::getNullValue(Builder.getInt32Ty());
}

llvm::Value *CodeGen::codegenIntLiteral(IntLiteral *E) {
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

llvm::Value *CodeGen::codegenFloatLiteral(FloatLiteral *E) {
  double Val = E->getValue();
  if (E->getType().getPtr()) {
    auto *BT = llvm::dyn_cast<BuiltinTy>(E->getType().getPtr());
    if (BT && BT->getBuiltinKind() == BuiltinTy::f32) {
      return llvm::ConstantFP::get(Builder.getFloatTy(), Val);
    }
  }
  return llvm::ConstantFP::get(Builder.getDoubleTy(), Val);
}

llvm::Value *CodeGen::codegenBoolLiteral(BoolLiteral *E) {
  return Builder.getInt1(E->getValue());
}

llvm::Value *CodeGen::codegenStrLiteral(StrLiteral *E) {
  return Builder.CreateGlobalStringPtr(E->getValue());
}

llvm::Value *CodeGen::codegenCharLiteral(CharLiteral *E) {
  return Builder.getInt8(E->getValue());
}

llvm::Value *CodeGen::codegenTupleLiteral(TupleLiteral *E) {
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

llvm::Value *CodeGen::codegenDeclRef(DeclRefExpr *E) {
  auto It = NamedValues.find(E->getDecl());
  if (It != NamedValues.end()) {
    llvm::Type *Ty = getLLVMType(E->getType());
    return Builder.CreateLoad(Ty, It->second, E->getDecl()->getId());
  }
  return llvm::Constant::getNullValue(getLLVMType(E->getType()));
}

llvm::Value *CodeGen::codegenFunCall(FunCallExpr *E) {
  // Get function
  FunDecl *Callee = E->getDecl();
  if (!Callee)
    return llvm::Constant::getNullValue(getLLVMType(E->getType()));

  // Intercept println
  if (Callee->getId() == "println") {
    return generatePrintlnCall(E);
  }

  llvm::Function *Fn = nullptr;

  // Try to resolve generic function
  if (E->hasTypeArgs()) {
    std::string MonoName =
        generateMonomorphizedName(Callee->getId(), E->getTypeArgs());
    Fn = Module.getFunction(MonoName);
  } else if (!Callee->getTypeArgs().empty()) {
    // If it's a generic function but no args provided, it might have been
    // monomorphized already This happens with inferred types or when called
    // from another generic
    TypeInstantiation TI{Callee,
                         {}}; // Fallback or search in MonomorphizedNames
    // For now try searching by ID in module if we can't find it in Functions
    // cache
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

llvm::Value *CodeGen::codegenMethodCall(MethodCallExpr *E) {
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
      if (auto *RT = llvm::dyn_cast<RefTy>(BaseTy.getPtr()))
        BaseTy = RT->getPointee();
      else if (auto *PT = llvm::dyn_cast<PtrTy>(BaseTy.getPtr()))
        BaseTy = PT->getPointee();
      else
        break;
    }
    if (auto *AppTy = llvm::dyn_cast<AppliedTy>(BaseTy.getPtr())) {
      if (auto *Adt = llvm::dyn_cast<AdtTy>(AppTy->getBase().getPtr())) {
        std::string ParentName = Adt->getId();
        std::string MonoParent =
            generateMonomorphizedName(ParentName, AppTy->getArgs());
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
    if (llvm::dyn_cast<RefTy>(ParamTy.getPtr()) ||
        llvm::dyn_cast<PtrTy>(ParamTy.getPtr())) {
      // Param expects pointer/reference
      // Check if Base is Value type
      TypeRef BaseTy = E->getBase()->getType();
      if (!llvm::dyn_cast<RefTy>(BaseTy.getPtr()) &&
          !llvm::dyn_cast<PtrTy>(BaseTy.getPtr())) {
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

  if (BaseVal)
    Args.push_back(BaseVal);
  else
    Args.push_back(llvm::UndefValue::get(
        getLLVMType(E->getBase()->getType()))); // Should not happen

  // Remaining arguments
  for (auto &Arg : E->getArgs()) {
    Args.push_back(codegenExpr(Arg.get()));
  }

  return Builder.CreateCall(Fn, Args);
}

llvm::Value *CodeGen::codegenBinaryOp(BinaryOp *E) {
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
    return Builder.CreateSRem(Lhs, Rhs, "rem"); // Todo float rem?
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
  default:
    return llvm::Constant::getNullValue(Ty);
  }
}

llvm::Value *CodeGen::codegenUnaryOp(UnaryOp *E) {
  llvm::Value *Operand = codegenExpr(&E->getOperand());
  switch (E->getOp()) {
  case TokenKind::Minus:
    if (Operand->getType()->isFloatingPointTy()) {
      return Builder.CreateFNeg(Operand, "neg");
    }
    return Builder.CreateNeg(Operand, "neg");
  case TokenKind::Bang:
    return Builder.CreateNot(Operand, "not");
  default:
    return Operand;
  }
}

llvm::Value *CodeGen::codegenAdtInit(AdtInit *E) {
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

llvm::Value *CodeGen::codegenStructInit(AdtInit *E, const StructDecl *S) {
  std::string StructName = S->getId();
  if (!S->getTypeArgs().empty() && E->hasTypeArgs()) {
    StructName = generateMonomorphizedName(S->getId(), E->getTypeArgs());
  }

  llvm::StructType *StructTy = StructTypes[StructName];
  if (!StructTy)
    return llvm::Constant::getNullValue(Builder.getInt32Ty());

  // Create alloca for struct
  auto *Alloca =
      createEntryBlockAlloca(CurrentFunction, generateTempVar(), StructTy);

  // Initialize fields
  for (auto &Init : E->getInits()) {
    const std::string &FieldName = Init->getId();
    auto FieldIt = FieldIndices[StructName].find(FieldName);
    if (FieldIt != FieldIndices[StructName].end()) {
      unsigned FieldIdx = FieldIt->second;
      llvm::Value *FieldPtr =
          Builder.CreateStructGEP(StructTy, Alloca, FieldIdx);
      llvm::Value *Val = codegenExpr(Init->getInitValue());
      Builder.CreateStore(Val, FieldPtr);
    }
  }

  return Builder.CreateLoad(StructTy, Alloca);
}

llvm::Value *CodeGen::codegenEnumInit(AdtInit *E, const EnumDecl *En) {
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
  auto *Alloca =
      createEntryBlockAlloca(CurrentFunction, generateTempVar(), EnumTy);

  // Store discriminant
  auto *DiscPtr = Builder.CreateStructGEP(EnumTy, Alloca, 0);
  Builder.CreateStore(Builder.getInt32(Discriminant), DiscPtr);

  // Store payload if present
  auto PayloadIt = VariantPayloadTypes[EnumName].find(VariantName);
  if (PayloadIt != VariantPayloadTypes[EnumName].end() &&
      !E->getInits().empty()) {
    llvm::Type *PayloadTy = PayloadIt->second;
    auto *PayloadPtr = Builder.CreateStructGEP(EnumTy, Alloca, 1);
    auto *TypedPayloadPtr =
        Builder.CreateBitCast(PayloadPtr, PayloadTy->getPointerTo());
    llvm::Value *PayloadVal = codegenExpr(E->getInits()[0]->getInitValue());
    Builder.CreateStore(PayloadVal, TypedPayloadPtr);
  }

  return Builder.CreateLoad(EnumTy, Alloca);
}

llvm::Value *CodeGen::codegenFieldAccess(FieldAccessExpr *E) {
  llvm::Value *BasePtr = getLValuePtr(E->getBase());
  if (!BasePtr) {
    // Base is not an lvalue, compute it
    BasePtr = codegenExpr(E->getBase());
    // For value types, we need to store to a temp first
    llvm::Type *BaseTy = getLLVMType(E->getBase()->getType());
    auto *Alloca =
        createEntryBlockAlloca(CurrentFunction, generateTempVar(), BaseTy);
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
      auto *FieldPtr =
          Builder.CreateStructGEP(StructTy, BasePtr, FieldIt->second);
      return Builder.CreateLoad(getLLVMType(E->getType()), FieldPtr);
    }
  }

  return llvm::Constant::getNullValue(getLLVMType(E->getType()));
}

llvm::Value *CodeGen::codegenTupleIndex(TupleIndex *E) {
  llvm::Value *BasePtr = getLValuePtr(E->getBase());
  if (!BasePtr)
    return llvm::Constant::getNullValue(getLLVMType(E->getType()));

  llvm::Type *BaseTy = getLLVMType(E->getBase()->getType());
  auto *ElemPtr = Builder.CreateStructGEP(BaseTy, BasePtr, E->getIndexVal());
  return Builder.CreateLoad(getLLVMType(E->getType()), ElemPtr);
}

llvm::Value *CodeGen::codegenIntrinsicCall(IntrinsicCall *E) {
  switch (E->getIntrinsicKind()) {
  case IntrinsicCall::IntrinsicKind::Panic: {
    // Evaluate message
    llvm::Value *Msg = nullptr;
    if (!E->getArgs().empty()) {
      Msg = codegenExpr(E->getArgs()[0].get());
    }

    // Declare printf if not exists
    if (!PrintFn)
      declarePrintln();

    // Declare abort
    llvm::FunctionCallee AbortFn = Module.getOrInsertFunction(
        "abort", llvm::FunctionType::get(Builder.getVoidTy(), false));

    // Print "Panic: "
    llvm::Value *Prefix = Builder.CreateGlobalStringPtr("Panic: ");
    Builder.CreateCall(PrintFn, {Builder.CreateGlobalStringPtr("%s"), Prefix});

    // Print message if present
    if (Msg) {
      // Assuming generic print like println
      llvm::Type *MsgTy = Msg->getType();
      std::string Fmt;
      if (MsgTy->isIntegerTy(32))
        Fmt = "%d\n";
      else if (MsgTy->isDoubleTy())
        Fmt = "%f\n";
      else if (MsgTy->isPointerTy())
        Fmt = "%s\n";
      else
        Fmt = "Unknown\n";

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
    if (E->getArgs().empty())
      return llvm::Constant::getNullValue(Builder.getVoidTy());

    llvm::Value *Cond = codegenExpr(E->getArgs()[0].get());
    if (!Cond->getType()->isIntegerTy(1)) {
      Cond = Builder.CreateICmpNE(
          Cond, llvm::Constant::getNullValue(Cond->getType()));
    }

    llvm::BasicBlock *FailBB =
        llvm::BasicBlock::Create(Context, "assert.fail", CurrentFunction);
    llvm::BasicBlock *ContBB =
        llvm::BasicBlock::Create(Context, "assert.cont", CurrentFunction);

    Builder.CreateCondBr(Cond, ContBB, FailBB);

    // Fail Block
    Builder.SetInsertPoint(FailBB);

    // Declare printf/abort
    if (!PrintFn)
      declarePrintln();
    llvm::FunctionCallee AbortFn = Module.getOrInsertFunction(
        "abort", llvm::FunctionType::get(Builder.getVoidTy(), false));

    llvm::Value *Msg = Builder.CreateGlobalStringPtr("Assertion failed\n");
    if (E->getArgs().size() > 1) {
      codegenExpr(E->getArgs()[1].get());
      // Print user message... simplified for now, just print "Assertion failed"
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

llvm::Value *CodeGen::codegenMatchExpr(MatchExpr *E) {
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
      ScrutineeAlloca =
          createEntryBlockAlloca(CurrentFunction, "scrutinee", ScrutineeTy);
      Builder.CreateStore(Scrutinee, ScrutineeAlloca);
    }
  } else {
    ScrutineeAlloca =
        createEntryBlockAlloca(CurrentFunction, "scrutinee", ScrutineeTy);
    Builder.CreateStore(Scrutinee, ScrutineeAlloca);
  }

  auto *StartBB = Builder.GetInsertBlock();
  auto *MergeBB =
      llvm::BasicBlock::Create(Context, "match.end", CurrentFunction);
  auto *FailBB =
      llvm::BasicBlock::Create(Context, "match.fail", CurrentFunction);

  // PHI for result
  llvm::Type *ResultTy = getLLVMType(E->getType());
  llvm::PHINode *ResultPHI = nullptr;
  if (!ResultTy->isVoidTy()) {
    Builder.SetInsertPoint(MergeBB);
    ResultPHI =
        Builder.CreatePHI(ResultTy, E->getArms().size() + 1, "match.result");
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
      if (VariantDiscriminants.count(EnumName)) {
        IsEnum = true;
      }
    }
  }

  if (!IsEnum && !IsInteger)
    CanUseSwitch = false;

  // Check arms
  for (size_t I = 0; I < E->getArms().size(); ++I) {
    if (HasWildcard) {
      // Wildcard must be last
      CanUseSwitch = false;
      break;
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
        if (!IsInteger) {
          CanUseSwitch = false;
          break;
        }
      } else if (std::holds_alternative<PatternAtomics::Variant>(Pat)) {
        if (!IsEnum) {
          CanUseSwitch = false;
          break;
        }
      } else {
        CanUseSwitch = false;
        break;
      }
    }
    if (!CanUseSwitch)
      break;
  }

  if (CanUseSwitch && (IsEnum || IsInteger)) {
    Builder.SetInsertPoint(StartBB);

    llvm::Value *SwitchVal = nullptr;
    if (IsInteger) {
      SwitchVal = Builder.CreateLoad(ScrutineeTy, ScrutineeAlloca);
    } else {
      // Load discriminant
      auto *DiscPtr = Builder.CreateStructGEP(
          llvm::cast<llvm::StructType>(ScrutineeTy), ScrutineeAlloca, 0);
      SwitchVal = Builder.CreateLoad(Builder.getInt32Ty(), DiscPtr);
    }

    llvm::BasicBlock *DefaultBB = FailBB;

    // We will create the switch inst at the end

    // Generate bodies and Collect cases
    std::vector<std::pair<llvm::ConstantInt *, llvm::BasicBlock *>> Cases;

    for (const auto &Arm : E->getArms()) {
      llvm::BasicBlock *ArmBB = llvm::BasicBlock::Create(
          Context, "match.arm", CurrentFunction, FailBB);

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
        for (const auto &Pat : Arm.Patterns) {
          if (auto *P = std::get_if<PatternAtomics::Variant>(&Pat)) {
            if (!P->Vars.empty()) {
              // Extract
              auto PayloadIt =
                  VariantPayloadTypes[EnumName].find(P->VariantName);
              if (PayloadIt != VariantPayloadTypes[EnumName].end()) {
                llvm::Type *PayloadTy = PayloadIt->second;
                auto *PayloadPtr = Builder.CreateStructGEP(
                    llvm::cast<llvm::StructType>(ScrutineeTy), ScrutineeAlloca,
                    1);
                auto *TypedPayloadPtr = Builder.CreateBitCast(
                    PayloadPtr, PayloadTy->getPointerTo());
                llvm::Value *PayloadVal =
                    Builder.CreateLoad(PayloadTy, TypedPayloadPtr);

                if (!P->Vars.empty()) {
                  auto &BoundVar = P->Vars[0];
                  llvm::Type *VarTy = getLLVMType(BoundVar->getType());
                  auto *VarAlloca = createEntryBlockAlloca(
                      CurrentFunction, BoundVar->getId(), VarTy);
                  Builder.CreateStore(PayloadVal, VarAlloca);
                  NamedValues[BoundVar.get()] = VarAlloca;
                }
              }
            }
            break;
          }
        }
      }

      // Manually generate body to capture last expression result
      llvm::Value *BodyResult = nullptr;

      for (size_t i = 0; i < Arm.Body->getStmts().size(); ++i) {
        if (hasTerminator())
          break;
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
          llvm::Value *Result =
              BodyResult ? BodyResult
                         : llvm::Constant::getNullValue(ResultPHI->getType());
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
      if (ResultPHI)
        ResultPHI->addIncoming(llvm::UndefValue::get(ResultTy), FailBB);
      Builder.CreateBr(MergeBB);
    }

    Builder.SetInsertPoint(MergeBB);
    if (ResultPHI)
      return ResultPHI;
    return llvm::Constant::getNullValue(Builder.getInt32Ty());
  }

  // --- End Switch Optimization (Fallback to linear check) ---

  // Generate arm blocks first (to allow forward jumps)
  auto &Arms = E->getArms();
  std::vector<llvm::BasicBlock *> ArmBBs;
  for (size_t I = 0; I < Arms.size(); ++I) {
    ArmBBs.push_back(llvm::BasicBlock::Create(Context, "match.arm",
                                              CurrentFunction, FailBB));
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

llvm::Value *CodeGen::codegenMatchArm(const MatchExpr::Arm &Arm,
                                      llvm::Value *Scrutinee,
                                      llvm::BasicBlock *NextArmBB,
                                      llvm::BasicBlock *MergeBB,
                                      llvm::PHINode *ResultPHI) {
  auto *MatchBB = llvm::BasicBlock::Create(Context, "match.body",
                                           CurrentFunction, NextArmBB);

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

void CodeGen::matchPattern(const Pattern &Pat, llvm::Value *Scrutinee,
                           llvm::BasicBlock *SuccessBB,
                           llvm::BasicBlock *FailBB) {
  std::visit(
      [&](auto &&P) {
        using T = std::decay_t<decltype(P)>;
        if constexpr (std::is_same_v<T, PatternAtomics::Wildcard>) {
          matchWildcard(SuccessBB);
        } else if constexpr (std::is_same_v<T, PatternAtomics::Literal>) {
          matchLiteral(P, Scrutinee, SuccessBB, FailBB);
        } else if constexpr (std::is_same_v<T, PatternAtomics::Variant>) {
          matchVariant(P, Scrutinee, SuccessBB, FailBB);
        }
      },
      Pat);
}

void CodeGen::matchWildcard(llvm::BasicBlock *SuccessBB) {
  Builder.CreateBr(SuccessBB);
}

void CodeGen::matchLiteral(const PatternAtomics::Literal &Lit,
                           llvm::Value *Scrutinee, llvm::BasicBlock *SuccessBB,
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

void CodeGen::matchVariant(const PatternAtomics::Variant &Var,
                           llvm::Value *Scrutinee, llvm::BasicBlock *SuccessBB,
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
  auto *Cmp = Builder.CreateICmpEQ(ActualDisc, Builder.getInt32(ExpectedDisc),
                                   "variant.cmp");

  // If pattern has bindings, extract payload
  if (!Var.Vars.empty()) {
    auto *ExtractBB = llvm::BasicBlock::Create(Context, "variant.extract",
                                               CurrentFunction, SuccessBB);
    Builder.CreateCondBr(Cmp, ExtractBB, FailBB);

    Builder.SetInsertPoint(ExtractBB);

    // Load and bind payload variables
    auto PayloadIt = VariantPayloadTypes[EnumName].find(Var.VariantName);
    if (PayloadIt != VariantPayloadTypes[EnumName].end()) {
      llvm::Type *PayloadTy = PayloadIt->second;
      auto *PayloadPtr = Builder.CreateStructGEP(StructTy, Scrutinee, 1);
      auto *TypedPayloadPtr =
          Builder.CreateBitCast(PayloadPtr, PayloadTy->getPointerTo());
      llvm::Value *PayloadVal = Builder.CreateLoad(PayloadTy, TypedPayloadPtr);

      // Bind to first variable
      if (!Var.Vars.empty()) {
        auto &BoundVar = Var.Vars[0];
        llvm::Type *VarTy = getLLVMType(BoundVar->getType());
        auto *VarAlloca =
            createEntryBlockAlloca(CurrentFunction, BoundVar->getId(), VarTy);
        Builder.CreateStore(PayloadVal, VarAlloca);
        NamedValues[BoundVar.get()] = VarAlloca;
      }
    }

    Builder.CreateBr(SuccessBB);
  } else {
    Builder.CreateCondBr(Cmp, SuccessBB, FailBB);
  }
}

llvm::Value *CodeGen::getLValuePtr(Expr *E) {
  if (auto *DR = llvm::dyn_cast<DeclRefExpr>(E)) {
    auto It = NamedValues.find(DR->getDecl());
    if (It != NamedValues.end())
      return It->second;
  } else if (auto *FA = llvm::dyn_cast<FieldAccessExpr>(E)) {
    llvm::Value *BasePtr = getLValuePtr(FA->getBase());
    if (!BasePtr) {
      BasePtr = codegenExpr(FA->getBase());
      // If getting address of RValue, we might need a temp?
      // But normally getLValue implies we want an address to store into.
      // If base is RValue, we can't really store into it unless we promote to
      // stack. Here we assume it's acceptable if we have a pointer.
    }

    if (BasePtr) {
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
    }
  } else if (auto *IE = llvm::dyn_cast<TupleIndex>(E)) {
    llvm::Value *BasePtr = getLValuePtr(IE->getBase());
    if (!BasePtr)
      return nullptr;
    llvm::Type *BaseTy = getLLVMType(IE->getBase()->getType());
    return Builder.CreateStructGEP(BaseTy, BasePtr, IE->getIndexVal());
  }
  if (auto *AE = llvm::dyn_cast<ArrayIndex>(E)) {
    llvm::Value *Base = codegenExpr(AE->getBase());
    llvm::Value *DataPtr = Builder.CreateExtractValue(Base, 0);
    llvm::Value *Idx = codegenExpr(AE->getIndex());

    llvm::Type *ElemTy = getLLVMType(AE->getType());
    return Builder.CreateGEP(ElemTy, DataPtr, Idx);
  }
  return nullptr;
}

llvm::Value *CodeGen::codegenArrayLiteral(ArrayLiteral *E) {
  std::vector<llvm::Value *> ElementVals;
  llvm::Type *ElemTy = nullptr;
  for (auto &El : E->getElements()) {
    llvm::Value *V = codegenExpr(El.get());
    ElementVals.push_back(V);
    if (!ElemTy)
      ElemTy = V->getType();
  }

  if (!ElemTy) {
    TypeRef AT = E->getType();
    if (auto *ArT = llvm::dyn_cast<ArrayTy>(AT.getPtr())) {
      ElemTy = getLLVMType(ArT->getContainedTy());
    } else {
      ElemTy = Builder.getInt8Ty();
    }
  }

  uint64_t Count = ElementVals.size();

  llvm::ArrayType *ArrayTy = llvm::ArrayType::get(ElemTy, Count);
  llvm::Value *BackingArray =
      createEntryBlockAlloca(CurrentFunction, "array_literal", ArrayTy);

  for (uint64_t I = 0; I < Count; ++I) {
    llvm::Value *Ptr =
        Builder.CreateConstInBoundsGEP2_32(ArrayTy, BackingArray, 0, I);
    Builder.CreateStore(ElementVals[I], Ptr);
  }

  llvm::StructType *SliceTy =
      llvm::cast<llvm::StructType>(getLLVMType(E->getType()));
  llvm::Value *Slice = llvm::UndefValue::get(SliceTy);

  llvm::Value *DataPtr =
      Builder.CreateConstInBoundsGEP2_32(ArrayTy, BackingArray, 0, 0);

  Slice = Builder.CreateInsertValue(Slice, DataPtr, 0);
  Slice = Builder.CreateInsertValue(Slice, Builder.getInt64(Count), 1);

  return Slice;
}

llvm::Value *CodeGen::codegenArrayIndex(ArrayIndex *E) {
  llvm::Value *BaseVal = codegenExpr(E->getBase());
  llvm::Value *IdxVal = codegenExpr(E->getIndex());

  llvm::Value *DataPtr = Builder.CreateExtractValue(BaseVal, 0);

  llvm::Type *ElemTy = getLLVMType(E->getType());

  llvm::Value *Ptr = Builder.CreateGEP(ElemTy, DataPtr, IdxVal);

  return Builder.CreateLoad(ElemTy, Ptr);
}

llvm::Value *CodeGen::generatePrintlnCall(FunCallExpr *Call) {
  if (!PrintFn)
    declarePrintln();

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
