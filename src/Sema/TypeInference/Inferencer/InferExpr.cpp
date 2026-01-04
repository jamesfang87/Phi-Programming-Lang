#include "Sema/TypeInference/Inferencer.hpp"

#include <print>
#include <vector>

#include <llvm/ADT/STLExtras.h>
#include <llvm/ADT/TypeSwitch.h>
#include <llvm/Support/Casting.h>

#include "AST/Nodes/Decl.hpp"
#include "AST/TypeSystem/Context.hpp"
#include "AST/TypeSystem/Type.hpp"
#include "Diagnostics/DiagnosticBuilder.hpp"
#include "Lexer/TokenKind.hpp"

namespace phi {

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
  std::vector<TypeRef> Ts;
  for (auto &Elem : E.getElements()) {
    Ts.push_back(visit(*Elem));
  }
  return TypeCtx::getTuple(Ts, E.getSpan());

  Unifier.unify(TypeCtx::getTuple(Ts, E.getSpan()), E.getType());
  return Unifier.resolve(E.getType());
}

TypeRef TypeInferencer::visit(DeclRefExpr &E) {
  assert(E.getDecl());

  auto Res = Unifier.unify(E.getDecl()->getType(), E.getType());
  if (!Res) {
    return TypeCtx::getErr(E.getSpan());
  }

  return Unifier.resolve(E.getType());
}

TypeRef TypeInferencer::visit(FunCallExpr &E) {
  assert(llvm::isa<FunDecl>(E.getDecl()));

  bool Errored = false;
  for (auto [Arg, Param] : llvm::zip(E.getArgs(), E.getDecl()->getParams())) {
    visit(*Arg);
    auto Res = Unifier.unify(Arg->getType(), Param->getType());
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

  Errored = Unifier.unify(E.getType(), E.getDecl()->getReturnTy()) && Errored;

  return Errored ? TypeCtx::getErr(E.getSpan()) : Unifier.resolve(E.getType());
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

  if (K.isEquality()) {
    auto Null = TypeCtx::getBuiltin(BuiltinTy::Null, E.getSpan());
    Unifier.unify(E.getType(), Null);
    return Null;
  }

  if (K.isComparison() || K.isEquality()) {
    auto Bool = TypeCtx::getBuiltin(BuiltinTy::Bool, E.getSpan());
    Unifier.unify(E.getType(), Bool);
    return Bool;
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
  case phi::TokenKind::Amp: {
    return TypeCtx::getRef(OperandT, E.getSpan());
  }
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
  for (auto &Init : E.getInits()) {
    visit(*Init);

    if (E.isAnonymous())
      continue;

    llvm::TypeSwitch<AdtDecl *>(E.getDecl())
        .Case<StructDecl>([&](StructDecl *D) {
          auto Declared = D->getField(Init->getId())->getType();
          auto Got = Init->getInitValue()->getType();
          Unifier.unify(Declared, Got);
        })
        .Case<EnumDecl>([&](EnumDecl *D) {
          auto *Variant = D->getVariant(Init->getId());
          if (Variant->hasType()) {
            auto Declared = Variant->getType();
            auto Got = Init->getInitValue()->getType();
            Unifier.unify(Declared, Got);
          }
        });
  }

  return E.getType();
}

TypeRef TypeInferencer::visit(MemberInit &E) {
  if (E.getInitValue()) {
    return visit(*E.getInitValue());
  }
  return TypeCtx::getBuiltin(BuiltinTy::Null, E.getSpan());
}

TypeRef TypeInferencer::visit(FieldAccessExpr &E) {
  // 1. Infer base type
  auto BaseT = visit(*E.getBase()).getUnderlying();

  // 2. Only structs / ADTs can have fields
  if (!BaseT.isAdt() && !BaseT.isVar()) {
    error("Cannot access field on non-ADT type")
        .with_primary_label(
            E.getBase()->getSpan(),
            std::format("type `{}` has no fields", toString(BaseT)))
        .emit(*DiagMan);
    return TypeCtx::getErr(E.getSpan());
  }

  // 3. Resolve ADT
  if (BaseT.isVar()) {
    return E.getType();
  }

  auto *Adt = llvm::cast<AdtTy>(BaseT.getPtr());
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

  auto FieldT = Field->getType();
  Unifier.unify(E.getType(), FieldT);
  return FieldT;
}

TypeRef TypeInferencer::visit(MethodCallExpr &E) {
  // 1. Infer base type
  auto BaseT = visit(*E.getBase()).getUnderlying();

  // 2. Only structs / ADTs / traits can have methods
  if (!BaseT.isAdt() && !BaseT.isVar()) {
    error("Cannot call method on non-ADT type")
        .with_primary_label(
            E.getBase()->getSpan(),
            std::format("type `{}` has no methods", toString(BaseT)))
        .emit(*DiagMan);
    return TypeCtx::getErr(E.getSpan());
  }

  if (BaseT.isVar()) {
    // Fresh return type for method on type variable
    for (auto &Arg : E.getArgs())
      visit(*Arg); // just visit args, we can't check types yet
    return E.getType();
  }

  auto *Adt = llvm::cast<AdtTy>(BaseT.getPtr());
  auto *Decl = Adt->getDecl();
  if (!Decl) {
    error("Cannot call method on unknown type")
        .with_primary_label(E.getBase()->getSpan(),
                            std::format("unknown ADT `{}`", Adt->getId()))
        .emit(*DiagMan);
    return TypeCtx::getErr(E.getSpan());
  }

  auto *Method =
      Decl->getMethod(llvm::dyn_cast<DeclRefExpr>(&E.getCallee())->getId());
  if (!Method) {
    error(std::format("Method `{}` not found in `{}`", E.getMethod().getId(),
                      Adt->getId()))
        .with_primary_label(E.getBase()->getSpan(),
                            std::format("type `{}` has no method `{}`",
                                        Adt->getId(), E.getMethod().getId()))
        .emit(*DiagMan);
    return TypeCtx::getErr(E.getSpan());
  }
  E.setMethod(Method);

  bool Errored = false;

  // 3. Unify arguments
  auto &Params = Method->getParams();
  if (Params.size() != E.getArgs().size() + 1) {
    error("Argument count mismatch")
        .with_primary_label(E.getBase()->getSpan(),
                            std::format("expected {} argument(s), got {}",
                                        Params.size(), E.getArgs().size() + 1))
        .emit(*DiagMan);
    Errored = true;
  } else {
    // Skip the first param (self) when unifying arguments
    for (auto [Arg, Param] :
         llvm::zip(E.getArgs(), llvm::drop_begin(Params, 1))) {
      auto ArgT = visit(*Arg);
      auto Res = Unifier.unify(ArgT, Param->getType());
      if (!Res) {
        error(std::format("Mismatched type for parameter `{}`", Param->getId()))
            .with_primary_label(Arg->getSpan(),
                                std::format("expected type `{}` but got `{}`",
                                            toString(Param->getType()),
                                            toString(ArgT)))
            .emit(*DiagMan);
        Errored = true;
      }
    }
  }
  // 4. Unify return type
  auto RetT = Method->getReturnTy();
  auto RetRes = Unifier.unify(E.getType(), RetT);
  if (!RetRes)
    Errored = true;

  return Errored ? TypeCtx::getErr(E.getSpan()) : RetT;
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
    std::println("{}", ArmT.toString());
    Unifier.unify(E.getType(), ArmT);
  }

  return Unifier.resolve(E.getType());
}

TypeRef TypeInferencer::visit(Expr &E) {
  return llvm::TypeSwitch<Expr *, TypeRef>(&E)
      .Case<IntLiteral>([&](IntLiteral *X) { return visit(*X); })
      .Case<FloatLiteral>([&](FloatLiteral *X) { return visit(*X); })
      .Case<StrLiteral>([&](StrLiteral *X) { return visit(*X); })
      .Case<CharLiteral>([&](CharLiteral *X) { return visit(*X); })
      .Case<BoolLiteral>([&](BoolLiteral *X) { return visit(*X); })
      .Case<RangeLiteral>([&](RangeLiteral *X) { return visit(*X); })
      .Case<TupleLiteral>([&](TupleLiteral *X) { return visit(*X); })
      .Case<DeclRefExpr>([&](DeclRefExpr *X) { return visit(*X); })
      .Case<FunCallExpr>([&](FunCallExpr *X) { return visit(*X); })
      .Case<BinaryOp>([&](BinaryOp *X) { return visit(*X); })
      .Case<UnaryOp>([&](UnaryOp *X) { return visit(*X); })
      .Case<MemberInit>([&](MemberInit *X) { return visit(*X); })
      .Case<FieldAccessExpr>([&](FieldAccessExpr *X) { return visit(*X); })
      .Case<MethodCallExpr>([&](MethodCallExpr *X) { return visit(*X); })
      .Case<MatchExpr>([&](MatchExpr *X) { return visit(*X); })
      .Case<AdtInit>([&](AdtInit *X) { return visit(*X); })
      .Default([&](Expr *) {
        llvm_unreachable("Unhandled Expr kind in TypeInferencer");
        return TypeCtx::getErr(E.getSpan());
      });
}

} // namespace phi
