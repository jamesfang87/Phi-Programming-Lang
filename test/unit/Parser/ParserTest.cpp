//===----------------------------------------------------------------------===//
//
// Parser tests for the Phi programming language
//
//===----------------------------------------------------------------------===//

#include "Parser/Parser.hpp"
#include "AST/Nodes/Decl.hpp"
#include "AST/Nodes/Expr.hpp"
#include "AST/Nodes/Stmt.hpp"
#include "Diagnostics/DiagnosticManager.hpp"
#include "Lexer/Lexer.hpp"
#include "SrcManager/SrcManager.hpp"

#include "llvm/Support/Casting.h"
#include <gtest/gtest.h>
#include <memory>

using namespace phi;

namespace {

class ParserTest : public ::testing::Test {
protected:
  void SetUp() override { DiagMgr = std::make_unique<DiagnosticManager>(); }

  std::unique_ptr<ModuleDecl> parse(const std::string &Source,
                                    const std::string &Path = "test.phi") {

    DiagMgr->getSrcManager().addSrcFile(Path, Source);
    auto Tokens = Lexer(Source, Path, DiagMgr.get()).scan();
    return Parser(std::move(Tokens), DiagMgr.get()).parse();
  }

  std::unique_ptr<DiagnosticManager> DiagMgr;
};

//===----------------------------------------------------------------------===//
// Expression Parsing Tests
//===----------------------------------------------------------------------===//

TEST_F(ParserTest, IntegerLiteral) {
  auto Mod = parse(R"(
    fun test() {
      42;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  auto *Stmt = llvm::dyn_cast<ExprStmt>(Fun->getBody().getStmts()[0].get());
  auto *Lit = llvm::dyn_cast<IntLiteral>(&Stmt->getExpr());
  ASSERT_NE(Lit, nullptr);
  EXPECT_EQ(Lit->getValue(), 42);
}

TEST_F(ParserTest, FloatLiteral) {
  auto Mod = parse(R"(
    fun test() {
      3.14;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  auto *Stmt = llvm::dyn_cast<ExprStmt>(Fun->getBody().getStmts()[0].get());
  auto *Lit = llvm::dyn_cast<FloatLiteral>(&Stmt->getExpr());
  ASSERT_NE(Lit, nullptr);
  EXPECT_DOUBLE_EQ(Lit->getValue(), 3.14);
}

TEST_F(ParserTest, StringLiteral) {
  auto Mod = parse(R"(
    fun test() {
      "hello world";
    }
  )");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  auto *Stmt = llvm::dyn_cast<ExprStmt>(Fun->getBody().getStmts()[0].get());
  auto *Lit = llvm::dyn_cast<StrLiteral>(&Stmt->getExpr());
  ASSERT_NE(Lit, nullptr);
  EXPECT_EQ(Lit->getValue(), "hello world");
}

TEST_F(ParserTest, BoolLiterals) {
  auto Mod = parse(R"(
    fun test() {
      true;
      false;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  auto *TrueStmt = llvm::dyn_cast<ExprStmt>(Fun->getBody().getStmts()[0].get());
  auto *FalseStmt =
      llvm::dyn_cast<ExprStmt>(Fun->getBody().getStmts()[1].get());
  auto *TrueLit = llvm::dyn_cast<BoolLiteral>(&TrueStmt->getExpr());
  auto *FalseLit = llvm::dyn_cast<BoolLiteral>(&FalseStmt->getExpr());
  ASSERT_NE(TrueLit, nullptr);
  ASSERT_NE(FalseLit, nullptr);
  EXPECT_TRUE(TrueLit->getValue());
  EXPECT_FALSE(FalseLit->getValue());
}

TEST_F(ParserTest, RangeLiteral) {
  auto Mod = parse(R"(
    fun test() {
      0..10;
      0..=10;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_EQ(Fun->getBody().getStmts().size(), 2u);
}

TEST_F(ParserTest, TupleLiteral) {
  auto Mod = parse(R"(
    fun test() {
      (1, 2, 3);
      (x, 3.4, "str");
    }
  )");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_EQ(Fun->getBody().getStmts().size(), 2u);
}

TEST_F(ParserTest, UnaryOperators) {
  auto Mod = parse(R"(
    fun test() {
      -x;
      !x;
      ++x;
      --x;
      &x;
      *x;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_EQ(Fun->getBody().getStmts().size(), 6u);
}

TEST_F(ParserTest, PostfixOperators) {
  auto Mod = parse(R"(
    fun test() {
      x++;
      x--;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_EQ(Fun->getBody().getStmts().size(), 2u);
}

TEST_F(ParserTest, BinaryOperators) {
  auto Mod = parse(R"(
    fun test() {
      a + b;
      a - b;
      a * b;
      a / b;
      a % b;
      a == b;
      a != b;
      a < b;
      a <= b;
      a > b;
      a >= b;
      a && b;
      a || b;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_EQ(Fun->getBody().getStmts().size(), 13u);
}

TEST_F(ParserTest, OperatorPrecedence) {
  auto Mod = parse(R"(
    fun test() {
      1 + 2 * 3;
      1 * 2 + 3;
      (1 + 2) * 3;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_EQ(Fun->getBody().getStmts().size(), 3u);
}

TEST_F(ParserTest, FunctionCall) {
  auto Mod = parse(R"(
    fun test() {
      foo();
      bar(1, 2, 3);
      baz(x, y);
    }
  )");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_EQ(Fun->getBody().getStmts().size(), 3u);
}

TEST_F(ParserTest, FieldAccess) {
  auto Mod = parse(R"(
    fun test() {
      obj.field;
      obj.field1.field2;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_EQ(Fun->getBody().getStmts().size(), 2u);
}

TEST_F(ParserTest, MethodCall) {
  auto Mod = parse(R"(
    fun test() {
      obj.method();
      obj.method(arg1, arg2);
    }
  )");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_EQ(Fun->getBody().getStmts().size(), 2u);
}

TEST_F(ParserTest, Grouping) {
  auto Mod = parse(R"(
    fun test() {
      (1 + 2) * 3;
      ((1 + 2) * 3) / 4;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_EQ(Fun->getBody().getStmts().size(), 2u);
}

TEST_F(ParserTest, DeclRefExpr) {
  auto Mod = parse(R"(
    fun test() {
      x;
      this;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_EQ(Fun->getBody().getStmts().size(), 2u);
}

//===----------------------------------------------------------------------===//
// Statement Parsing Tests
//===----------------------------------------------------------------------===//

TEST_F(ParserTest, ReturnStatement) {
  auto Mod = parse(R"(
    fun test() {
      return;
      return 42;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_EQ(Fun->getBody().getStmts().size(), 2u);
  auto *Return1 =
      llvm::dyn_cast<ReturnStmt>(Fun->getBody().getStmts()[0].get());
  auto *Return2 =
      llvm::dyn_cast<ReturnStmt>(Fun->getBody().getStmts()[1].get());
  ASSERT_NE(Return1, nullptr);
  ASSERT_NE(Return2, nullptr);
  EXPECT_EQ(Return1->hasExpr(), false);
  EXPECT_EQ(Return2->hasExpr(), true);
}

TEST_F(ParserTest, IfStatement) {
  auto Mod = parse(R"(
    fun test() {
      if x < 5 {
        return 1;
      }
    }
  )");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_EQ(Fun->getBody().getStmts().size(), 1u);
  auto *If = llvm::dyn_cast<IfStmt>(Fun->getBody().getStmts()[0].get());
  ASSERT_NE(If, nullptr);
}

TEST_F(ParserTest, IfElseStatement) {
  auto Mod = parse(R"(
    fun test() {
      if x < 5 {
        return 1;
      } else {
        return 2;
      }
    }
  )");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_EQ(Fun->getBody().getStmts().size(), 1u);
  auto *If = llvm::dyn_cast<IfStmt>(Fun->getBody().getStmts()[0].get());
  ASSERT_NE(If, nullptr);
  EXPECT_EQ(If->hasElse(), true);
}

TEST_F(ParserTest, WhileStatement) {
  auto Mod = parse(R"(
    fun test() {
      while x < 10 {
        x = x + 1;
      }
    }
  )");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_EQ(Fun->getBody().getStmts().size(), 1u);
  auto *While = llvm::dyn_cast<WhileStmt>(Fun->getBody().getStmts()[0].get());
  ASSERT_NE(While, nullptr);
}

TEST_F(ParserTest, ForStatement) {
  auto Mod = parse(R"(
    fun test() {
      for i in 0..10 {
        x = x + i;
      }
    }
  )");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_EQ(Fun->getBody().getStmts().size(), 1u);
  auto *For = llvm::dyn_cast<ForStmt>(Fun->getBody().getStmts()[0].get());
  ASSERT_NE(For, nullptr);
}

TEST_F(ParserTest, VariableDeclaration) {
  auto Mod = parse(R"(
    fun test() {
      var x = 42;
      const y = 10;
      var z: i32 = 5;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_EQ(Fun->getBody().getStmts().size(), 3u);
}

TEST_F(ParserTest, BreakStatement) {
  auto Mod = parse(R"(
    fun test() {
      while true {
        break;
      }
    }
  )");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_EQ(Fun->getBody().getStmts().size(), 1u);
  auto *While = llvm::dyn_cast<WhileStmt>(Fun->getBody().getStmts()[0].get());
  auto *Break = llvm::dyn_cast<BreakStmt>(While->getBody().getStmts()[0].get());
  ASSERT_NE(Break, nullptr);
}

TEST_F(ParserTest, ContinueStatement) {
  auto Mod = parse(R"(
    fun test() {
      while true {
        continue;
      }
    }
  )");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_EQ(Fun->getBody().getStmts().size(), 1u);
  auto *While = llvm::dyn_cast<WhileStmt>(Fun->getBody().getStmts()[0].get());
  auto *Cont =
      llvm::dyn_cast<ContinueStmt>(While->getBody().getStmts()[0].get());
  ASSERT_NE(Cont, nullptr);
}

TEST_F(ParserTest, DeferStatement) {
  auto Mod = parse(R"(
    fun test() {
      defer cleanup();
    }
  )");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_EQ(Fun->getBody().getStmts().size(), 1u);
  auto *Defer = llvm::dyn_cast<DeferStmt>(Fun->getBody().getStmts()[0].get());
  ASSERT_NE(Defer, nullptr);
}

//===----------------------------------------------------------------------===//
// Declaration Parsing Tests
//===----------------------------------------------------------------------===//

TEST_F(ParserTest, FunctionDeclaration) {
  auto Mod = parse(R"(
    fun add(const x: i32, const y: i32) -> i32 {
      return x + y;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  ASSERT_EQ(Mod->getItems().size(), 1u);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_NE(Fun, nullptr);
  EXPECT_EQ(Fun->getId(), "add");
  EXPECT_EQ(Fun->getParams().size(), 2u);
}

TEST_F(ParserTest, FunctionWithoutReturnType) {
  auto Mod = parse(R"(
    fun test() {
      return;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_NE(Fun, nullptr);
}

TEST_F(ParserTest, FunctionWithNoParameters) {
  auto Mod = parse(R"(
    fun main() {
      return;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_NE(Fun, nullptr);
  EXPECT_EQ(Fun->getParams().size(), 0u);
}

TEST_F(ParserTest, StructDeclaration) {
  auto Mod = parse(R"(
    struct Point {
      public x: f64;
      public y: f64;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  ASSERT_EQ(Mod->getItems().size(), 1u);
  auto *Struct = llvm::dyn_cast<StructDecl>(Mod->getItems()[0].get());
  ASSERT_NE(Struct, nullptr);
  EXPECT_EQ(Struct->getId(), "Point");
  EXPECT_EQ(Struct->getFields().size(), 2u);
}

TEST_F(ParserTest, EnumDeclaration) {
  auto Mod = parse(R"(
    enum Result {
      Ok: i32;
      Err: string;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  ASSERT_EQ(Mod->getItems().size(), 1u);
  auto *Enum = llvm::dyn_cast<EnumDecl>(Mod->getItems()[0].get());
  ASSERT_NE(Enum, nullptr);
  EXPECT_EQ(Enum->getId(), "Result");
  EXPECT_GE(Enum->getVariants().size(), 2u);
}

TEST_F(ParserTest, ModuleDeclaration) {
  auto Mod = parse(R"(
    module mymodule
    fun test() {}
  )");
  ASSERT_NE(Mod, nullptr);
  EXPECT_NE(Mod->getId(), "$main");
}

TEST_F(ParserTest, ImportStatement) {
  auto Mod = parse(R"(
    import othermodule;
    fun test() {}
  )");
  ASSERT_NE(Mod, nullptr);
  EXPECT_EQ(Mod->getImports().size(), 1u);
}

//===----------------------------------------------------------------------===//
// Type Parsing Tests
//===----------------------------------------------------------------------===//

TEST_F(ParserTest, BuiltinTypes) {
  auto Mod = parse(R"(
    fun test() {
      var a: i8 = 1;
      var b: i16 = 2;
      var c: i32 = 3;
      var d: i64 = 4;
      var e: u8 = 5;
      var f: u16 = 6;
      var g: u32 = 7;
      var h: u64 = 8;
      var i: f32 = 9.0;
      var j: f64 = 10.0;
      var k: string = "hello";
      var l: char = 'a';
      var m: bool = true;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_EQ(Fun->getBody().getStmts().size(), 13u);
}

TEST_F(ParserTest, PointerType) {
  auto Mod = parse(R"(
    fun test() {
      var x: *i32 = nullptr;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_EQ(Fun->getBody().getStmts().size(), 1u);
}

TEST_F(ParserTest, ReferenceType) {
  auto Mod = parse(R"(
    fun test() {
      var x: &i32 = nullptr;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_EQ(Fun->getBody().getStmts().size(), 1u);
}

TEST_F(ParserTest, TupleType) {
  auto Mod = parse(R"(
    fun test() {
      var x: (i32, f64, string) = (1, 2.0, "test");
    }
  )");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_EQ(Fun->getBody().getStmts().size(), 1u);
}

TEST_F(ParserTest, ADTType) {
  auto Mod = parse(R"(
    struct Point {
      x: f64;
      y: f64;
    }
    fun test() {
      var p: Point = Point { x: 1.0, y: 2.0 };
    }
  )");
  ASSERT_NE(Mod, nullptr);
  ASSERT_EQ(Mod->getItems().size(), 2u);
}

//===----------------------------------------------------------------------===//
// Complex Expression Tests
//===----------------------------------------------------------------------===//

TEST_F(ParserTest, NestedExpressions) {
  auto Mod = parse(R"(
    fun test() {
      (a + b) * (c - d);
      foo(bar(x), y);
      obj.field.method();
    }
  )");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_EQ(Fun->getBody().getStmts().size(), 3u);
}

TEST_F(ParserTest, AssignmentOperators) {
  auto Mod = parse(R"(
    fun test() {
      x += 1;
      x -= 1;
      x *= 2;
      x /= 2;
      x %= 3;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  auto *Fun = llvm::dyn_cast<FunDecl>(Mod->getItems()[0].get());
  ASSERT_EQ(Fun->getBody().getStmts().size(), 5u);
}

//===----------------------------------------------------------------------===//
// Error Recovery Tests
//===----------------------------------------------------------------------===//

TEST_F(ParserTest, MissingSemicolon) {
  auto Mod = parse(R"(
    fun test() {
      var x = 1
      var y = 2;
    }
  )");
  // Should recover and continue parsing
  ASSERT_NE(Mod, nullptr);
}

TEST_F(ParserTest, MissingClosingBrace) {
  auto Mod = parse(R"(
    fun test() {
      var x = 1;
  )");
  // Should handle error gracefully
  ASSERT_NE(Mod, nullptr);
}

TEST_F(ParserTest, EmptyModule) {
  auto Mod = parse(R"()");
  // Empty module should be handled
  ASSERT_NE(Mod, nullptr);
}

} // namespace
