#include <gtest/gtest.h>

#include "Diagnostics/DiagnosticManager.hpp"
#include "Lexer/Lexer.hpp"
#include "Parser/Parser.hpp"

#include "AST/Nodes/Decl.hpp"
#include "AST/Nodes/Expr.hpp"
#include "AST/Nodes/Stmt.hpp"
#include "AST/TypeSystem/Type.hpp"

#include <llvm/Support/Casting.h>

#include <memory>
#include <string>

using namespace phi;

// Helper: parse source and return ModuleDecl
static std::unique_ptr<ModuleDecl> parse(const std::string &Src,
                                         DiagnosticManager &Diags) {
  Diags.getSrcManager().addSrcFile("test.phi", Src);
  Lexer L(Src, "test.phi", &Diags);
  auto Tokens = L.scan();
  if (Diags.hasError())
    return nullptr;
  Parser P(Tokens, &Diags);
  return P.parse();
}

static std::unique_ptr<ModuleDecl> parseOk(const std::string &Src) {
  DiagnosticConfig Cfg;
  Cfg.UseColors = false;
  DiagnosticManager Diags(Cfg);
  auto Mod = parse(Src, Diags);
  EXPECT_FALSE(Diags.hasError()) << "Unexpected parse error for: " << Src;
  return Mod;
}

static void parseError(const std::string &Src) {
  DiagnosticConfig Cfg;
  Cfg.UseColors = false;
  DiagnosticManager Diags(Cfg);
  auto Mod = parse(Src, Diags);
  EXPECT_TRUE(Diags.hasError()) << "Expected parse error for: " << Src;
}

//===----------------------------------------------------------------------===//
// Empty module
//===----------------------------------------------------------------------===//

TEST(Parser, EmptyModule) {
  auto Mod = parseOk("");
  ASSERT_NE(Mod, nullptr);
  EXPECT_EQ(Mod->getItems().size(), 0u);
}

//===----------------------------------------------------------------------===//
// Function Declarations
//===----------------------------------------------------------------------===//

TEST(Parser, SimpleFunctionDecl) {
  auto Mod = parseOk("fun main() {}");
  ASSERT_NE(Mod, nullptr);
  ASSERT_EQ(Mod->getItems().size(), 1u);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_NE(Fun, nullptr);
  EXPECT_EQ(Fun->getId(), "main");
  EXPECT_EQ(Fun->getParams().size(), 0u);
}

TEST(Parser, FunctionWithParams) {
  auto Mod = parseOk("fun add(const a: i32, const b: i32) -> i32 { return a + b; }");
  ASSERT_NE(Mod, nullptr);
  ASSERT_EQ(Mod->getItems().size(), 1u);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_NE(Fun, nullptr);
  EXPECT_EQ(Fun->getId(), "add");
  EXPECT_EQ(Fun->getParams().size(), 2u);
  EXPECT_EQ(Fun->getParams()[0]->getId(), "a");
  EXPECT_EQ(Fun->getParams()[1]->getId(), "b");
}

TEST(Parser, FunctionReturnType) {
  auto Mod = parseOk("fun getVal() -> i32 { return 42; }");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_NE(Fun, nullptr);
  EXPECT_TRUE(Fun->getReturnType().isBuiltin());
}

TEST(Parser, MultipleFunctions) {
  auto Mod = parseOk("fun foo() {} fun bar() {} fun baz() {}");
  ASSERT_NE(Mod, nullptr);
  EXPECT_EQ(Mod->getItems().size(), 3u);
}

TEST(Parser, GenericFunction) {
  auto Mod = parseOk("fun identity<T>(const x: T) -> T { return x; }");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_NE(Fun, nullptr);
  EXPECT_EQ(Fun->getId(), "identity");
  EXPECT_TRUE(Fun->hasTypeArgs());
  EXPECT_EQ(Fun->getTypeArgs().size(), 1u);
  EXPECT_EQ(Fun->getTypeArgs()[0]->getId(), "T");
}

//===----------------------------------------------------------------------===//
// Variable Declarations
//===----------------------------------------------------------------------===//

TEST(Parser, ConstDecl) {
  auto Mod = parseOk("fun main() { const x = 5; }");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_NE(Fun, nullptr);
  ASSERT_EQ(Fun->getBody().getStmts().size(), 1u);
  auto *DS = llvm::dyn_cast<DeclStmt>(Fun->getBody().getStmts()[0].get());
  ASSERT_NE(DS, nullptr);
  EXPECT_EQ(DS->getDecl().getId(), "x");
  EXPECT_TRUE(DS->getDecl().isConst());
}

TEST(Parser, VarDeclWithType) {
  auto Mod = parseOk("fun main() { var y: i32 = 10; }");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_NE(Fun, nullptr);
  auto *DS = llvm::dyn_cast<DeclStmt>(Fun->getBody().getStmts()[0].get());
  ASSERT_NE(DS, nullptr);
  EXPECT_EQ(DS->getDecl().getId(), "y");
  EXPECT_FALSE(DS->getDecl().isConst());
}

//===----------------------------------------------------------------------===//
// Struct Declarations (comma-terminated fields)
//===----------------------------------------------------------------------===//

TEST(Parser, SimpleStruct) {
  auto Mod = parseOk("struct Point { public x: f64, public y: f64 }");
  ASSERT_NE(Mod, nullptr);
  ASSERT_EQ(Mod->getItems().size(), 1u);
  auto *S = llvm::dyn_cast<StructDecl>(Mod->getItems()[0].get());
  ASSERT_NE(S, nullptr);
  EXPECT_EQ(S->getId(), "Point");
  EXPECT_EQ(S->getFields().size(), 2u);
  EXPECT_EQ(S->getFields()[0]->getId(), "x");
  EXPECT_EQ(S->getFields()[1]->getId(), "y");
}

TEST(Parser, StructWithMethod) {
  // Comma between last field and first method
  auto Mod = parseOk(R"(
    struct Vec2 {
      public x: f64,
      public y: f64,

      fun dot(const this, const other: Vec2) -> f64 {
        return this.x * other.x + this.y * other.y;
      }
    }
  )");
  ASSERT_NE(Mod, nullptr);
  // There should be at least 1 item (StructDecl)
  // Note: static methods may be desugared into FunDecls at top-level
  bool FoundStruct = false;
  for (auto &Item : Mod->getItems()) {
    if (auto *S = llvm::dyn_cast<StructDecl>(Item.get())) {
      FoundStruct = true;
      EXPECT_EQ(S->getFields().size(), 2u);
      EXPECT_EQ(S->getMethods().size(), 1u);
      EXPECT_EQ(S->getMethods()[0]->getId(), "dot");
    }
  }
  EXPECT_TRUE(FoundStruct);
}

TEST(Parser, GenericStruct) {
  auto Mod = parseOk(R"(
    struct Wrapper<T> {
      public value: T
    }
  )");
  ASSERT_NE(Mod, nullptr);
  auto *S = llvm::dyn_cast<StructDecl>(Mod->getItems()[0].get());
  ASSERT_NE(S, nullptr);
  EXPECT_TRUE(S->hasTypeArgs());
  EXPECT_EQ(S->getTypeArgs().size(), 1u);
  EXPECT_EQ(S->getTypeArgs()[0]->getId(), "T");
}

//===----------------------------------------------------------------------===//
// Enum Declarations (comma-terminated variants)
//===----------------------------------------------------------------------===//

TEST(Parser, SimpleEnum) {
  auto Mod = parseOk(R"(
    enum Color {
      Red,
      Green,
      Blue
    }
  )");
  ASSERT_NE(Mod, nullptr);
  auto *E = llvm::dyn_cast<EnumDecl>(Mod->getItems()[0].get());
  ASSERT_NE(E, nullptr);
  EXPECT_EQ(E->getId(), "Color");
  EXPECT_EQ(E->getVariants().size(), 3u);
  EXPECT_EQ(E->getVariants()[0]->getId(), "Red");
  EXPECT_EQ(E->getVariants()[1]->getId(), "Green");
  EXPECT_EQ(E->getVariants()[2]->getId(), "Blue");
}

TEST(Parser, EnumWithPayloads) {
  auto Mod = parseOk(R"(
    enum Shape {
      Circle: f64,
      Rectangle: { l: f64, w: f64 }
    }
  )");
  ASSERT_NE(Mod, nullptr);
  bool FoundEnum = false;
  for (auto &Item : Mod->getItems()) {
    if (auto *E = llvm::dyn_cast<EnumDecl>(Item.get())) {
      if (E->getId() == "Shape") {
        FoundEnum = true;
        EXPECT_EQ(E->getVariants().size(), 2u);
        EXPECT_TRUE(E->getVariants()[0]->hasPayload());
        EXPECT_TRUE(E->getVariants()[1]->hasPayload());
      }
    }
  }
  EXPECT_TRUE(FoundEnum);
}

TEST(Parser, EnumWithMethod) {
  // Comma between last variant and method
  auto Mod = parseOk(R"(
    enum Result {
      Ok: i32,
      Err: string,

      fun is_ok(const this) -> bool {
        return match this {
          .Ok => true,
          .Err => false,
        };
      }
    }
  )");
  ASSERT_NE(Mod, nullptr);
  bool FoundEnum = false;
  for (auto &Item : Mod->getItems()) {
    if (auto *E = llvm::dyn_cast<EnumDecl>(Item.get())) {
      if (E->getId() == "Result") {
        FoundEnum = true;
        EXPECT_EQ(E->getVariants().size(), 2u);
        EXPECT_EQ(E->getMethods().size(), 1u);
      }
    }
  }
  EXPECT_TRUE(FoundEnum);
}

TEST(Parser, GenericEnum) {
  auto Mod = parseOk(R"(
    enum Option<T> {
      Some: T,
      None
    }
  )");
  ASSERT_NE(Mod, nullptr);
  auto *E = llvm::dyn_cast<EnumDecl>(Mod->getItems()[0].get());
  ASSERT_NE(E, nullptr);
  EXPECT_TRUE(E->hasTypeArgs());
  EXPECT_EQ(E->getTypeArgs().size(), 1u);
  EXPECT_EQ(E->getTypeArgs()[0]->getId(), "T");
  EXPECT_EQ(E->getVariants().size(), 2u);
}

//===----------------------------------------------------------------------===//
// Statements
//===----------------------------------------------------------------------===//

TEST(Parser, IfElseStatement) {
  auto Mod = parseOk(R"(
    fun main() {
      if true {
        const x = 1;
      } else {
        const x = 2;
      }
    }
  )");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_NE(Fun, nullptr);
  ASSERT_EQ(Fun->getBody().getStmts().size(), 1u);
  auto *IS = llvm::dyn_cast<IfStmt>(Fun->getBody().getStmts()[0].get());
  ASSERT_NE(IS, nullptr);
  EXPECT_TRUE(IS->hasElse());
}

TEST(Parser, WhileLoop) {
  auto Mod = parseOk(R"(
    fun main() {
      var i = 0;
      while i < 10 {
        i++;
      }
    }
  )");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_NE(Fun, nullptr);
  EXPECT_GE(Fun->getBody().getStmts().size(), 2u);
}

TEST(Parser, ForLoop) {
  auto Mod = parseOk(R"(
    fun main() {
      for i in 0..10 {
        const x = i;
      }
    }
  )");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_NE(Fun, nullptr);
  auto *FS = llvm::dyn_cast<ForStmt>(Fun->getBody().getStmts()[0].get());
  ASSERT_NE(FS, nullptr);
}

TEST(Parser, ReturnStatement) {
  auto Mod = parseOk("fun foo() -> i32 { return 42; }");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_NE(Fun, nullptr);
  auto *RS = llvm::dyn_cast<ReturnStmt>(Fun->getBody().getStmts()[0].get());
  ASSERT_NE(RS, nullptr);
  EXPECT_TRUE(RS->hasExpr());
}

TEST(Parser, BreakAndContinue) {
  auto Mod = parseOk(R"(
    fun main() {
      while true {
        break;
        continue;
      }
    }
  )");
  ASSERT_NE(Mod, nullptr);
}

TEST(Parser, DeferStatement) {
  auto Mod = parseOk(R"(
    fun main() {
      defer println("done");
    }

    fun println(const msg: string) {}
  )");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_NE(Fun, nullptr);
  auto *DS = llvm::dyn_cast<DeferStmt>(Fun->getBody().getStmts()[0].get());
  ASSERT_NE(DS, nullptr);
}

//===----------------------------------------------------------------------===//
// Expression Parsing & Precedence
//===----------------------------------------------------------------------===//

TEST(Parser, BinaryExprPrecedence) {
  // 1 + 2 * 3 should parse as 1 + (2 * 3)
  auto Mod = parseOk("fun main() { const x = 1 + 2 * 3; }");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_NE(Fun, nullptr);
  auto *DS = llvm::dyn_cast<DeclStmt>(Fun->getBody().getStmts()[0].get());
  ASSERT_NE(DS, nullptr);
  auto *Init = llvm::dyn_cast<BinaryOp>(&DS->getDecl().getInit());
  ASSERT_NE(Init, nullptr);
  // Top-level should be Plus
  EXPECT_EQ(Init->getOp().Value, TokenKind::Plus);
  // RHS should be Mul
  auto *Rhs = llvm::dyn_cast<BinaryOp>(&Init->getRhs());
  ASSERT_NE(Rhs, nullptr);
  EXPECT_EQ(Rhs->getOp().Value, TokenKind::Star);
}

TEST(Parser, GroupingExpression) {
  // (1 + 2) * 3 should parse as (1 + 2) * 3
  auto Mod = parseOk("fun main() { const x = (1 + 2) * 3; }");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  auto *DS = llvm::dyn_cast<DeclStmt>(Fun->getBody().getStmts()[0].get());
  auto *Init = llvm::dyn_cast<BinaryOp>(&DS->getDecl().getInit());
  ASSERT_NE(Init, nullptr);
  // Top-level should be Star
  EXPECT_EQ(Init->getOp().Value, TokenKind::Star);
}

TEST(Parser, UnaryExpression) {
  auto Mod = parseOk("fun main() { const x = -42; const y = !true; }");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_NE(Fun, nullptr);
  auto *DS = llvm::dyn_cast<DeclStmt>(Fun->getBody().getStmts()[0].get());
  auto *Init = llvm::dyn_cast<UnaryOp>(&DS->getDecl().getInit());
  ASSERT_NE(Init, nullptr);
}

//===----------------------------------------------------------------------===//
// Literals & Complex Expression Types
//===----------------------------------------------------------------------===//

TEST(Parser, TupleLiteral) {
  auto Mod = parseOk("fun main() { const t = (1, 2, 3); }");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  auto *DS = llvm::dyn_cast<DeclStmt>(Fun->getBody().getStmts()[0].get());
  auto *Init = llvm::dyn_cast<TupleLiteral>(&DS->getDecl().getInit());
  ASSERT_NE(Init, nullptr);
  EXPECT_EQ(Init->getElements().size(), 3u);
}

TEST(Parser, ArrayLiteral) {
  auto Mod = parseOk("fun main() { const arr = [1, 2, 3]; }");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  auto *DS = llvm::dyn_cast<DeclStmt>(Fun->getBody().getStmts()[0].get());
  auto *Init = llvm::dyn_cast<ArrayLiteral>(&DS->getDecl().getInit());
  ASSERT_NE(Init, nullptr);
  EXPECT_EQ(Init->getElements().size(), 3u);
}

TEST(Parser, FunctionCall) {
  auto Mod = parseOk("fun foo(const x: i32) {} fun main() { foo(42); }");
  ASSERT_NE(Mod, nullptr);
}

TEST(Parser, MatchExpression) {
  auto Mod = parseOk(R"(
    enum Color { Red, Green, Blue }

    fun main() {
      const c = Color { Red };
      const x = match c {
        .Red => 1,
        .Green => 2,
        .Blue => 3,
      };
    }
  )");
  ASSERT_NE(Mod, nullptr);
}

TEST(Parser, StructInit) {
  auto Mod = parseOk("struct Point { public x: f64, public y: f64 }\n    fun main() { const p = Point { x : 1.0, y : 2.0 }; }");
  ASSERT_NE(Mod, nullptr);
}

//===----------------------------------------------------------------------===//
// Complex Types
//===----------------------------------------------------------------------===//

TEST(Parser, ArrayTypeAnnotation) {
  auto Mod = parseOk("fun foo(const arr: [i32]) {}");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_NE(Fun, nullptr);
  EXPECT_EQ(Fun->getParams().size(), 1u);
  EXPECT_TRUE(Fun->getParams()[0]->getType().isArray());
}

TEST(Parser, TupleTypeAnnotation) {
  auto Mod = parseOk("fun foo() -> (i32, f64) { return (1, 2.0); }");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_NE(Fun, nullptr);
  EXPECT_TRUE(Fun->getReturnType().isTuple());
}

TEST(Parser, PointerType) {
  auto Mod = parseOk("fun foo(const p: *i32) {}");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_NE(Fun, nullptr);
  EXPECT_TRUE(Fun->getParams()[0]->getType().isPtr());
}

TEST(Parser, ReferenceType) {
  auto Mod = parseOk("fun foo(const r: &i32) {}");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_NE(Fun, nullptr);
  EXPECT_TRUE(Fun->getParams()[0]->getType().isRef());
}

TEST(Parser, NestedArrayType) {
  auto Mod = parseOk("fun foo(const arr: [[i32]]) {}");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_NE(Fun, nullptr);
  auto Ty = Fun->getParams()[0]->getType();
  EXPECT_TRUE(Ty.isArray());
  auto *ArrTy2 = llvm::dyn_cast<ArrayTy>(Ty.getPtr());
  EXPECT_TRUE(ArrTy2->getContainedTy().isArray());
}

TEST(Parser, NestedTupleType) {
  auto Mod = parseOk("fun foo() -> (i32, (f64, bool)) { return (1, (2.0, true)); }");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_NE(Fun, nullptr);
  auto Ty = Fun->getReturnType();
  EXPECT_TRUE(Ty.isTuple());
}

TEST(Parser, GenericTypeAnnotation) {
  auto Mod = parseOk(R"(
    struct Box<T> { public value: T }
    fun foo(const b: Box<i32>) {}
  )");
  ASSERT_NE(Mod, nullptr);
  // Box<i32> is parsed as an AppliedTy
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems().back().get());
  ASSERT_NE(Fun, nullptr);
  EXPECT_TRUE(Fun->getParams()[0]->getType().isApplied());
}

TEST(Parser, ArrayOfTuples) {
  auto Mod = parseOk("fun foo(const arr: [(i32, f64)]) {}");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_NE(Fun, nullptr);
  auto Ty = Fun->getParams()[0]->getType();
  EXPECT_TRUE(Ty.isArray());
  auto *A = llvm::dyn_cast<ArrayTy>(Ty.getPtr());
  EXPECT_TRUE(A->getContainedTy().isTuple());
}

// Note: *[i32] (pointer to array) is not currently supported by the parser.
// The parser doesn't handle * before [ in type parsing.

TEST(Parser, RefToGeneric) {
  auto Mod = parseOk(R"(
    struct Box<T> { public value: T }
    fun foo(const r: &Box<i32>) {}
  )");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems().back().get());
  ASSERT_NE(Fun, nullptr);
  auto Ty = Fun->getParams()[0]->getType();
  EXPECT_TRUE(Ty.isRef());
  auto *R = llvm::dyn_cast<RefTy>(Ty.getPtr());
  EXPECT_TRUE(R->getPointee().isApplied());
}

//===----------------------------------------------------------------------===//
// Error Recovery
//===----------------------------------------------------------------------===//

TEST(Parser, MissingSemicolon) {
  // Should produce error but not crash
  parseError("fun main() { const x = 5 }");
}

TEST(Parser, UnclosedBrace) {
  parseError("fun main() {");
}

TEST(Parser, InvalidTopLevel) {
  parseError("42;");
}
