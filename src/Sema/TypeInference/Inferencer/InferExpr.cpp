#include "Sema/TypeInference/Inferencer.hpp"

#include <print>
#include <string>
#include <unordered_map>
#include <vector>

#include <llvm/ADT/STLExtras.h>
#include <llvm/ADT/TypeSwitch.h>
#include <llvm/Support/raw_ostream.h> // DEBUG
#include <llvm/Support/Casting.h>

#include "AST/Nodes/Decl.hpp"
#include "AST/Nodes/Expr.hpp"
#include "AST/TypeSystem/Context.hpp"
#include "AST/TypeSystem/Type.hpp"
#include "Diagnostics/DiagnosticBuilder.hpp"
#include "Lexer/TokenKind.hpp"

namespace phi {

TypeRef TypeInferencer::visit(Expr &E) {
  return llvm::TypeSwitch<Expr *, TypeRef>(&E)
      .Case<IntLiteral>([&](IntLiteral *X) { return visit(*X); })
      .Case<FloatLiteral>([&](FloatLiteral *X) { return visit(*X); })
      .Case<StrLiteral>([&](StrLiteral *X) { return visit(*X); })
      .Case<CharLiteral>([&](CharLiteral *X) { return visit(*X); })
      .Case<BoolLiteral>([&](BoolLiteral *X) { return visit(*X); })
      .Case<RangeLiteral>([&](RangeLiteral *X) { return visit(*X); })
      .Case<TupleLiteral>([&](TupleLiteral *X) { return visit(*X); })
      .Case<ArrayLiteral>([&](ArrayLiteral *X) { return visit(*X); })
      .Case<DeclRefExpr>([&](DeclRefExpr *X) { return visit(*X); })
      .Case<FunCallExpr>([&](FunCallExpr *X) { return visit(*X); })
      .Case<BinaryOp>([&](BinaryOp *X) { return visit(*X); })
      .Case<UnaryOp>([&](UnaryOp *X) { return visit(*X); })
      .Case<MemberInit>([&](MemberInit *X) { return visit(*X); })
      .Case<FieldAccessExpr>([&](FieldAccessExpr *X) { return visit(*X); })
      .Case<MethodCallExpr>([&](MethodCallExpr *X) { return visit(*X); })
      .Case<MatchExpr>([&](MatchExpr *X) { return visit(*X); })
      .Case<AdtInit>([&](AdtInit *X) { return visit(*X); })
      .Case<IntrinsicCall>([&](IntrinsicCall *X) { return visit(*X); })
      .Case<TupleIndex>([&](TupleIndex *X) { return visit(*X); })
      .Case<ArrayIndex>([&](ArrayIndex *X) { return visit(*X); })
      .Default([&](Expr *) {
        llvm_unreachable("Unhandled Expr kind in TypeInferencer");
        return TypeCtx::getErr(E.getSpan());
      });
}

TypeRef TypeInferencer::visit(IntLiteral &E) {
  return Unifier.resolve(E.getType());
}

TypeRef TypeInferencer::visit(FloatLiteral &E) {
  return Unifier.resolve(E.getType());
}

TypeRef TypeInferencer::visit(BoolLiteral &E) {
  return TypeCtx::getBuiltin(BuiltinTy::Bool, E.getSpan());
}

TypeRef TypeInferencer::visit(CharLiteral &E) {
  return TypeCtx::getBuiltin(BuiltinTy::Char, E.getSpan());
}

TypeRef TypeInferencer::visit(StrLiteral &E) {
  return TypeCtx::getBuiltin(BuiltinTy::String, E.getSpan());
}

TypeRef TypeInferencer::visit(RangeLiteral &E) {
  auto StartT = visit(E.getStart());
  auto EndT = visit(E.getEnd());

  auto Res = Unifier.unify(StartT, EndT);
  if (!Res) {
    error("Start and end of range literal must be same type")
        .with_primary_label(E.getStart().getSpan(),
                            std::format("of type {}", toString(StartT)))
        .with_secondary_label(E.getEnd().getSpan(),
                              std::format("of type {}", toString(EndT)))
        .emit(*DiagMan);
    return TypeCtx::getErr(E.getSpan());
  }

  return TypeCtx::getBuiltin(BuiltinTy::Range, E.getSpan());
}

TypeRef TypeInferencer::visit(TupleLiteral &E) {
  std::vector<TypeRef> Types;
  for (auto &Elem : E.getElements()) {
    Types.push_back(visit(*Elem));
  }
  auto T = TypeCtx::getTuple(Types, E.getSpan());
  E.setType(T);
  return T;
}

TypeRef TypeInferencer::visit(ArrayLiteral &E) {
  TypeRef ContainedTy = E.getElements().front()->getType();
  for (auto &Elem : llvm::drop_begin(E.getElements(), 1)) {
    Unifier.unify(ContainedTy, Elem->getType());
  }

  Unifier.unify(TypeCtx::getArray(ContainedTy, E.getSpan()), E.getType());
  return Unifier.resolve(E.getType());
}

TypeRef TypeInferencer::visit(DeclRefExpr &E) {
  assert(E.getDecl());

  auto T = instantiate(E.getDecl());
  auto Res = Unifier.unify(T, E.getType());
  if (!Res) {
    return TypeCtx::getErr(E.getSpan());
  }

  return Unifier.resolve(E.getType());
}

TypeRef TypeInferencer::visit(FunCallExpr &E) {
  assert(llvm::isa<FunDecl>(E.getDecl()));

  bool Errored = false;

  auto Map = buildGenericSubstMap(E.getDecl()->getTypeArgs());
  std::vector<TypeRef> InferredTypeArgs;
  for (auto &Pair : Map) {
    InferredTypeArgs.push_back(Unifier.resolve(Pair.second));
  }
  E.setTypeArgs(InferredTypeArgs);

  if (E.getArgs().size() != E.getDecl()->getParams().size()) {
     error("Argument count mismatch")
        .with_primary_label(E.getSpan(), 
            std::format("expected {} arguments, got {}", 
                        E.getDecl()->getParams().size(), E.getArgs().size()))
        .emit(*DiagMan);
     return TypeCtx::getErr(E.getSpan());
  }

  for (auto [Arg, Param] : llvm::zip(E.getArgs(), E.getDecl()->getParams())) {
    visit(*Arg);

    auto Res = Unifier.unify(Arg->getType(),
                             substituteGenerics(Param->getType(), Map));

    if (!Res) {
      Errored = true;
      error(std::format("Mismatched type for parameter {}", Param->getId()))
          .with_primary_label(Arg->getSpan(),
                              std::format("expected type `{}` instead of `{}`",
                                          toString(Param->getType()),
                                          toString(Arg->getType())))
          .with_extra_snippet(
              E.getDecl()->getSpan(),
              std::format("{} declared here", E.getDecl()->getId()))
          .emit(*DiagMan);
    }
  }

  Errored =
      Unifier.unify(E.getType(),
                    substituteGenerics(E.getDecl()->getReturnType(), Map)) &&
      Errored;
  auto Res =
      Errored ? TypeCtx::getErr(E.getSpan()) : Unifier.resolve(E.getType());
  E.setType(Res);
  return Res;
}

TypeRef TypeInferencer::visit(BinaryOp &E) {
  auto LhsType = visit(E.getLhs());
  auto RhsType = visit(E.getRhs());

  const TokenKind K = E.getOp();

  if (K.isLogical()) {
    auto Res = Unifier.unify(
        LhsType, TypeCtx::getBuiltin(BuiltinTy::Bool, E.getLhs().getSpan()));
    if (!Res) {
      error("Operand to logical operator is not a bool")
          .with_primary_label(E.getLhs().getSpan(),
                              std::format("expected type `bool`, got type {}",
                                          toString(LhsType)))
          .emit(*DiagMan);
    }

    Res = Unifier.unify(
        RhsType, TypeCtx::getBuiltin(BuiltinTy::Bool, E.getRhs().getSpan()));
    if (!Res) {
      error("Operand to logical operator is not a bool")
          .with_primary_label(E.getRhs().getSpan(),
                              std::format("expected type `bool`, got type {}",
                                          toString(RhsType)))
          .emit(*DiagMan);
    }

    Unifier.unify(TypeCtx::getBuiltin(BuiltinTy::Bool, E.getSpan()),
                  E.getType());
    return TypeCtx::getBuiltin(BuiltinTy::Bool, E.getSpan());
  }

  auto Res = Unifier.unify(LhsType, RhsType);
  if (!Res) {
    error("Operands have different types")
        .with_primary_label(E.getLhs().getSpan(),
                            std::format("type `{}`", toString(LhsType)))
        .with_secondary_label(E.getRhs().getSpan(),
                              std::format("type `{}`", toString(RhsType)))
        .emit(*DiagMan);
    return TypeCtx::getErr(E.getSpan());
  }



  if (K.isComparison() || K.isEquality()) {
    auto Bool = TypeCtx::getBuiltin(BuiltinTy::Bool, E.getSpan());
    Unifier.unify(E.getType(), Bool);
    return Bool;
  }

  if (K == TokenKind::Equals ||
      K == TokenKind::PlusEquals ||
      K == TokenKind::SubEquals ||
      K == TokenKind::MulEquals ||
      K == TokenKind::DivEquals ||
      K == TokenKind::ModEquals) {
    auto Res = Unifier.unify(LhsType, RhsType);
    if (!Res) {
      error("Mismatched types in assignment")
          .with_primary_label(E.getLhs().getSpan(),
                              std::format("variable of type `{}`", toString(LhsType)))
          .with_secondary_label(E.getRhs().getSpan(),
                                std::format("assigned value of type `{}`", toString(RhsType)))
          .emit(*DiagMan);
      return TypeCtx::getErr(E.getSpan());
    }
    return LhsType;
  }

  assert(K.isArithmetic());
  Unifier.unify(E.getType(), LhsType);
  return Unifier.resolve(E.getType());
}

TypeRef TypeInferencer::visit(UnaryOp &E) {
  auto OperandT = visit(E.getOperand());

  switch (E.getOp()) {
  case phi::TokenKind::Bang: {
    auto Bool = TypeCtx::getBuiltin(BuiltinTy::Bool, E.getSpan());
    auto Res = Unifier.unify(Bool, OperandT);
    if (!Res) {
      error("Condition in while statement is not a bool")
          .with_primary_label(E.getOperand().getSpan(),
                              std::format("expected type `bool`, got type {}",
                                          toString(OperandT)))
          .emit(*DiagMan);

      return TypeCtx::getErr(E.getSpan());
    }
    Unifier.unify(Bool, E.getType());
    return Bool;
  }
  case phi::TokenKind::Minus: {
    // Unary minus
    Unifier.unify(E.getType(), OperandT);
    return Unifier.resolve(E.getType());
  }
  case phi::TokenKind::DoublePlus:
  case phi::TokenKind::DoubleMinus: {
    // Increment/Decrement
    // Operand must be numeric (or unify with result)
    // For now assume same type
    Unifier.unify(E.getType(), OperandT);
    return Unifier.resolve(E.getType());
  }
  case phi::TokenKind::Star: {
    // Wait, Deref returns Pointee.
    // PtrTy -> Pointee
    if (auto Ptr = llvm::dyn_cast<PtrTy>(OperandT.getPtr())) {
        E.setType(Ptr->getPointee());
        return Ptr->getPointee();
    }
    // If operand is not ptr, can't infer yet or error?
    // Assuming simple case for now
    return TypeCtx::getErr(E.getSpan());
  }
  case phi::TokenKind::Amp: {
    return TypeCtx::getRef(OperandT, E.getSpan());
  }
  case phi::TokenKind::Try:
    // TODO:
  default:
    return E.getType();
  }
}

TypeRef TypeInferencer::visit(AdtInit &E) {
  if (E.isAnonymous()) {
    return E.getType();
  }

  // if !E.isAnonymous(), then we already know the type of E
  // after name resolution
  auto Map = buildGenericSubstMap(E.getDecl()->getTypeArgs());

  // If explicit type arguments are provided, unify them with the generic parameters
  if (!E.getTypeArgs().empty()) {
    if (E.getTypeArgs().size() != E.getDecl()->getTypeArgs().size()) {
       error("Generic argument count mismatch")
           .with_primary_label(E.getSpan(), 
               std::format("expected {} generic arguments, got {}", 
                           E.getDecl()->getTypeArgs().size(), E.getTypeArgs().size()))
           .emit(*DiagMan);
    } else {
        for (auto [Explicit, GenericParam] : 
             llvm::zip(E.getTypeArgs(), E.getDecl()->getTypeArgs())) {
          auto It = Map.find(GenericParam.get());
          assert(It != Map.end());
          Unifier.unify(It->second, Explicit);
        }
    }
  }

  bool Errored = false;
  for (auto &Init : E.getInits()) {
    visit(*Init);

    assert(!E.isAnonymous());
    llvm::TypeSwitch<AdtDecl *>(E.getDecl())
        .Case<StructDecl>([&](StructDecl *D) {
          auto *Field = D->getField(Init->getId());
          auto Declared = substituteGenerics(Field->getType(), Map);
          auto Got = Init->getInitValue()->getType();
          if (!Unifier.unify(Declared, Got)) {
              error("Mismatched types in struct initialization")
                  .with_primary_label(Init->getInitValue()->getSpan(),
                                      std::format("expected `{}`, got `{}`",
                                                  toString(Declared), toString(Got)))
                  .emit(*DiagMan);
              Errored = true;
          }
        })
        .Case<EnumDecl>([&](EnumDecl *D) {
          auto *Variant = D->getVariant(Init->getId());
          if (Variant->hasPayload()) {
            auto Declared = substituteGenerics(Variant->getPayloadType(), Map);
            auto Got = Init->getInitValue()->getType();
            if (!Unifier.unify(Declared, Got)) {
                error("Mismatched types in enum variant payload")
                    .with_primary_label(Init->getInitValue()->getSpan(),
                                        std::format("expected `{}`, got `{}`",
                                                    toString(Declared), toString(Got)))
                    .emit(*DiagMan);
                Errored = true;
            }
          }
        });
  }

  if (Errored) {
      return TypeCtx::getErr(E.getSpan());
  }

  if (!E.getDecl()->hasTypeArgs())
    return E.getType();

  std::vector<TypeRef> InferredTypeArgs;
  for (auto &Pair : Map) {
    InferredTypeArgs.push_back(Unifier.resolve(Pair.second));
  }

  E.setTypeArgs(InferredTypeArgs);
  auto T = TypeCtx::getApplied(E.getType(), InferredTypeArgs, E.getSpan());
  E.setType(T);
  return T;
}

TypeRef TypeInferencer::visit(MemberInit &E) {
  if (!E.getInitValue()) {
    return TypeCtx::getBuiltin(BuiltinTy::Null, E.getSpan());
  }

  auto T = visit(*E.getInitValue());
  return Unifier.resolve(T);
}

TypeRef TypeInferencer::visit(FieldAccessExpr &E) {
  // 1. Infer base type
  auto BaseT = Unifier.resolve(visit(*E.getBase()));
  auto UnderlyingBaseT = BaseT.getUnderlying();

  // 2. Only structs / ADTs can have fields
  if (!UnderlyingBaseT.isAdt() && !UnderlyingBaseT.isVar()) {
    error("Cannot access field on non-ADT type")
        .with_primary_label(
            E.getBase()->getSpan(),
            std::format("type `{}` has no fields", toString(UnderlyingBaseT)))
        .emit(*DiagMan);
    return TypeCtx::getErr(E.getSpan());
  }

  // 3. Resolve ADT
  if (UnderlyingBaseT.isVar()) {
    return E.getType();
  }

  auto *Adt = llvm::cast<AdtTy>(UnderlyingBaseT.getPtr());
  auto *Decl = Adt->getDecl();
  if (!Decl) {
    // Missing declaration should be an error
    error("Cannot access field on unknown type")
        .with_primary_label(E.getBase()->getSpan(),
                            std::format("unknown ADT `{}`", Adt->getId()))
        .emit(*DiagMan);
    return TypeCtx::getErr(E.getSpan());
  }

  auto *Struct = llvm::dyn_cast<StructDecl>(Decl);
  if (!Struct) {
    error("Cannot perform field access on enums")
        .with_primary_label(E.getBase()->getSpan(),
                            std::format("this is an enum `{}`", Adt->getId()))
        .emit(*DiagMan);
    return TypeCtx::getErr(E.getSpan());
  }

  auto *Field = Struct->getField(E.getFieldId());
  if (!Field) {
    error(std::format("Field `{}` not found in `{}`", E.getFieldId(),
                      Adt->getId()))
        .with_primary_label(E.getBase()->getSpan(),
                            std::format("type `{}` has no field `{}`",
                                        Adt->getId(), E.getFieldId()))
        .emit(*DiagMan);
    return TypeCtx::getErr(E.getSpan());
  }
  E.setField(Field);
  assert(E.getField());

  auto T = BaseT.removeIndir();
  if (T.isAdt()) {
    Unifier.unify(E.getType(), Field->getType());
    return Field->getType();
  }

  auto *App = llvm::dyn_cast<AppliedTy>(T.getPtr());
  std::unordered_map<const TypeArgDecl *, TypeRef> Map;
  for (const auto &[Arg, Inst] :
       llvm::zip(Struct->getTypeArgs(), App->getArgs())) {
    Map.emplace(Arg.get(), Inst);
  }
  auto FieldT = substituteGenerics(Field->getType(), Map);
  Unifier.unify(E.getType(), FieldT);
  return FieldT;
}

TypeRef TypeInferencer::visit(MethodCallExpr &E) {
  // 1. Infer base type
  auto BaseT = visit(*E.getBase());
  auto UnderlyingBaseT = BaseT.getUnderlying();

  // 2. Only ADTs can have methods
  if (!UnderlyingBaseT.isAdt() && !UnderlyingBaseT.isVar()) {
    error("Cannot call method on non-ADT type")
        .with_primary_label(
            E.getBase()->getSpan(),
            std::format("type `{}` has no methods", toString(UnderlyingBaseT)))
        .emit(*DiagMan);
    return TypeCtx::getErr(E.getSpan());
  }

  if (UnderlyingBaseT.isVar()) {
    // Fresh return type for method on type variable
    for (auto &Arg : E.getArgs())
      visit(*Arg); // just visit args, we can't check types yet
    return E.getType();
  }

  auto *Adt = llvm::cast<AdtTy>(UnderlyingBaseT.getPtr());
  auto *Decl = Adt->getDecl();
  if (!Decl) {
    error("Cannot call method on unknown type")
        .with_primary_label(E.getBase()->getSpan(),
                            std::format("unknown ADT `{}`", Adt->getId()))
        .emit(*DiagMan);
    return TypeCtx::getErr(E.getSpan());
  }

  std::string Id = llvm::dyn_cast<DeclRefExpr>(&E.getCallee())->getId();
  auto *Method = Decl->getMethod(Id);
  if (!Method) {
    error(std::format("Method `{}` not found in `{}`", Id, Adt->getId()))
        .with_primary_label(
            E.getBase()->getSpan(),
            std::format("type `{}` has no method `{}`", Adt->getId(), Id))
        .emit(*DiagMan);
    return TypeCtx::getErr(E.getSpan());
  }
  E.setMethod(Method);

  bool Errored = false;

  auto BaseNoIndir = BaseT.removeIndir();
  std::unordered_map<const TypeArgDecl *, TypeRef> Map;
  if (auto *App = llvm::dyn_cast<AppliedTy>(BaseNoIndir.getPtr())) {
    for (const auto &[Arg, Inst] :
         llvm::zip(Decl->getTypeArgs(), App->getArgs())) {
      Map.emplace(Arg.get(), Inst);
    }
  }

  auto Temp = buildGenericSubstMap(Method->getTypeArgs());
  std::vector<TypeRef> InferredTypeArgs;
  for (auto &Pair : Temp) {
    InferredTypeArgs.push_back(Unifier.resolve(Pair.second));
  }
  Map.merge(Temp);
  E.setTypeArgs(InferredTypeArgs);

  // 6. Parameter arity check (method params include 'self' as first param)
  auto &Params = Method->getParams();
  if (Params.size() != E.getArgs().size() + 1) {
    error("Argument count mismatch")
        .with_primary_label(E.getBase()->getSpan(),
                            std::format("expected {} argument(s), got {}",
                                        Params.size(), E.getArgs().size() + 1))
        .emit(*DiagMan);
    Errored = true;
  } else {
    Unifier.unify(BaseNoIndir, Params.front()->getType().removeIndir());

    for (auto [Arg, Param] :
         llvm::zip(E.getArgs(), llvm::drop_begin(Params, 1))) {
      // visit arg to compute its type
      auto ArgT = visit(*Arg);
      auto ParamT = substituteGenerics(Param->getType(), Map);

      if (!Unifier.unify(ArgT, ParamT)) {
        error(std::format("Mismatched type for parameter `{}`", Param->getId()))
            .with_primary_label(Arg->getSpan(),
                                std::format("expected type `{}` but got `{}`",
                                            toString(ParamT), toString(ArgT)))
            .emit(*DiagMan);
        Errored = true;
      }
    }
  }

  Errored = Unifier.unify(E.getType(),
                          substituteGenerics(Method->getReturnType(), Map)) &&
            Errored;
  auto Res =
      Errored ? TypeCtx::getErr(E.getSpan()) : Unifier.resolve(E.getType());
  E.setType(Res);
  return Res;
}

TypeRef TypeInferencer::visit(MatchExpr &E) {
  auto ScrutineeT = visit(*E.getScrutinee());

  auto visitPattern = [&](const Pattern &Pat) -> TypeRef {
    return std::visit(
        [&](auto const &Pattern) -> TypeRef {
          using T = std::decay_t<decltype(Pattern)>;

          if constexpr (std::is_same_v<T, PatternAtomics::Literal>) {
            assert(Pattern.Value && "Literal pattern has no expression value");
            auto LiteralType = visit(*Pattern.Value);
            return Unifier.resolve(LiteralType);
          }

          else if constexpr (std::is_same_v<T, PatternAtomics::Variant>) {
            for (auto &Var : Pattern.Vars) {
              visit(*Var);
            }
            return TypeCtx::getVar(VarTy::Any, E.getSpan());
          }

          else {
            return E.getType();
          }
        },
        Pat);
  };

  for (const MatchExpr::Arm &Arm : E.getArms()) {
    for (auto &Pattern : Arm.Patterns) {
      auto PatternT = visitPattern(Pattern);
      Unifier.unify(ScrutineeT, PatternT);
    }

    visit(*Arm.Body);

    // does visiting twice do anything bad?
    auto ArmT = visit(*Arm.Return);
    Unifier.unify(E.getType(), ArmT);
  }

  return Unifier.resolve(E.getType());
}

TypeRef TypeInferencer::visit(IntrinsicCall &E) {
  for (auto &Arg : E.getArgs()) {
    visit(*Arg);
  }

  switch (E.getIntrinsicKind()) {
  case IntrinsicCall::IntrinsicKind::Panic:
  case IntrinsicCall::IntrinsicKind::Assert:
  case IntrinsicCall::IntrinsicKind::Unreachable:
  case IntrinsicCall::IntrinsicKind::TypeOf: // TODO: Implement TypeOf
    E.setType(TypeCtx::getErr(E.getSpan()));
    return TypeCtx::getErr(E.getSpan());
  }
  llvm_unreachable("Unhandled intrinsic kind");
}

TypeRef TypeInferencer::visit(TupleIndex &E) {
  auto BaseT = visit(*E.getBase());
  auto IndexT = visit(*E.getIndex());

  auto ExpectedIndexT =
      TypeCtx::getBuiltin(BuiltinTy::u64, E.getIndex()->getSpan());
  if (!Unifier.unify(IndexT, ExpectedIndexT)) {
    error("Index must be an integer type")
        .with_primary_label(
            E.getIndex()->getSpan(),
            std::format("expected integer type, found `{}`", toString(IndexT)))
        .emit(*DiagMan);
    return TypeCtx::getErr(E.getSpan());
  }

  // For tuples, the index must be a compile-time constant integer literal
  // Check if the index expression is an integer literal
  auto *IndexLit = llvm::dyn_cast<IntLiteral>(E.getIndex());
  if (!IndexLit) {
    error("Tuple index must be an integer literal")
        .with_primary_label(E.getIndex()->getSpan(),
                            "expected compile-time constant")
        .emit(*DiagMan);
    return TypeCtx::getErr(E.getSpan());
  }

  // Get the constant index value
  int64_t Index = IndexLit->getValue();

  // Check if base is a tuple type
  if (auto Tup = llvm::dyn_cast<TupleTy>(BaseT.getPtr())) {
    const auto &Elements = Tup->getElementTys();

    // Check bounds
    if (Index < 0 || static_cast<size_t>(Index) >= Elements.size()) {
      error(std::format("Tuple index out of bounds: the tuple has {} elements "
                        "but the index is {}",
                        Elements.size(), Index))
          .with_primary_label(E.getIndex()->getSpan(), "index out of bounds")
          .with_primary_label(
              E.getBase()->getSpan(),
              std::format("tuple has type `{}`", toString(BaseT)))
          .emit(*DiagMan);
      return TypeCtx::getErr(E.getSpan());
    }

    // Return the type of the element at the given index
    auto ElemT = Elements[Index];
    E.setType(ElemT);
    return ElemT;
  }

  // Error: base is not a tuple
  error("Cannot index into non-tuple type")
      .with_primary_label(
          E.getBase()->getSpan(),
          std::format("type `{}` cannot be indexed", toString(BaseT)))
      .emit(*DiagMan);

  E.setType(TypeCtx::getErr(E.getSpan()));
  return TypeCtx::getErr(E.getSpan());
}

TypeRef TypeInferencer::visit(ArrayIndex &E) {
  auto BaseT = visit(*E.getBase());
  auto IndexT = visit(*E.getIndex());

  auto ExpectedIndexT =
      TypeCtx::getBuiltin(BuiltinTy::u64, E.getIndex()->getSpan());
  if (!Unifier.unify(IndexT, ExpectedIndexT)) {
    error("Index must be an integer type")
        .with_primary_label(
            E.getIndex()->getSpan(),
            std::format("expected integer type, found `{}`", toString(IndexT)))
        .emit(*DiagMan);
    return TypeCtx::getErr(E.getSpan());
  }

  // Check if base is a tuple type
  if (auto Arr = llvm::dyn_cast<ArrayTy>(BaseT.getPtr())) {
    E.setType(Arr->getContainedTy());
    return Arr->getContainedTy();
  }

  // Error: base is not an array
  error("Cannot index into non-array type")
      .with_primary_label(
          E.getBase()->getSpan(),
          std::format("type `{}` cannot be indexed", toString(BaseT)))
      .emit(*DiagMan);
  return TypeCtx::getErr(E.getSpan());
}

} // namespace phi
