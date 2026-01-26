//===----------------------------------------------------------------------===//
//
// Name Resolver tests for the Phi programming language
//
//===----------------------------------------------------------------------===//

// Include standard library headers before LLVM to avoid cxxabi conflicts
#include <gtest/gtest.h>
#include <memory>
#include <string>
#include <vector>

#include "AST/Nodes/Decl.hpp"
#include "Diagnostics/DiagnosticManager.hpp"
#include "Lexer/Lexer.hpp"
#include "Parser/Parser.hpp"
#include "Sema/NameResolution/NameResolver.hpp"
#include "SrcManager/SrcManager.hpp"

using namespace phi;

namespace {

class NameResolverTest : public ::testing::Test {
protected:
  NameResolverTest() { DiagMgr = std::make_shared<DiagnosticManager>(); }

  ModuleDecl *parseAndResolve(const std::string &Source,
                              const std::string &Path = "test.phi") {
    DiagMgr->getSrcManager().addSrcFile(Path, Source);
    auto Tokens = Lexer(Source, Path, DiagMgr.get()).scan();
    Parser P(std::move(Tokens), DiagMgr.get());
    auto Mod = P.parse();
    if (!Mod)
      return nullptr;

    std::vector<ModuleDecl *> Mods = {Mod.get()};

    NameResolver R(std::move(Mods), DiagMgr.get());
    return R.resolveSingleMod(Mod.get());
  }

  std::shared_ptr<DiagnosticManager> DiagMgr;
};

//===----------------------------------------------------------------------===//
// Variable Resolution Tests
//===----------------------------------------------------------------------===//

TEST_F(NameResolverTest, LocalVariableResolution) {
  auto Mod = parseAndResolve(R"(
    fun test() {
      var x = 42;
      x;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  EXPECT_FALSE(DiagMgr->hasError());
}

TEST_F(NameResolverTest, ParameterResolution) {
  auto Mod = parseAndResolve(R"(
    fun add(const x: i32, const y: i32) -> i32 {
      return x + y;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  EXPECT_FALSE(DiagMgr->hasError());
}

TEST_F(NameResolverTest, VariableShadowing) {
  auto Mod = parseAndResolve(R"(
    fun test() {
      var x = 1;
      {
        var x = 2;
        x;
      }
      x;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  EXPECT_FALSE(DiagMgr->hasError());
}

TEST_F(NameResolverTest, UndeclaredVariable) {
  auto Mod = parseAndResolve(R"(
    fun test() {
      x;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  EXPECT_TRUE(DiagMgr->hasError());
}

TEST_F(NameResolverTest, VariableInDifferentScopes) {
  auto Mod = parseAndResolve(R"(
    fun test() {
      var x = 1;
      {
        var y = 2;
        x + y;
      }
      x;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  EXPECT_FALSE(DiagMgr->hasError());
}

TEST_F(NameResolverTest, VariableAfterDeclaration) {
  auto Mod = parseAndResolve(R"(
    fun test() {
      var x = 1;
      var y = x;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  EXPECT_FALSE(DiagMgr->hasError());
}

//===----------------------------------------------------------------------===//
// Function Resolution Tests
//===----------------------------------------------------------------------===//

TEST_F(NameResolverTest, FunctionCallResolution) {
  auto Mod = parseAndResolve(R"(
    fun add(const x: i32, const y: i32) -> i32 {
      return x + y;
    }
    fun test() {
      add(1, 2);
    }
  )");
  ASSERT_NE(Mod, nullptr);
  EXPECT_FALSE(DiagMgr->hasError());
}

TEST_F(NameResolverTest, UndeclaredFunction) {
  auto Mod = parseAndResolve(R"(
    fun test() {
      unknown();
    }
  )");
  ASSERT_NE(Mod, nullptr);
  EXPECT_TRUE(DiagMgr->hasError());
}

TEST_F(NameResolverTest, RecursiveFunctionCall) {
  auto Mod = parseAndResolve(R"(
    fun factorial(const n: i32) -> i32 {
      if n == 0 {
        return 1;
      }
      return n * factorial(n - 1);
    }
  )");
  ASSERT_NE(Mod, nullptr);
  EXPECT_FALSE(DiagMgr->hasError());
}

TEST_F(NameResolverTest, FunctionCallWithArguments) {
  auto Mod = parseAndResolve(R"(
    fun add(const x: i32, const y: i32) -> i32 {
      return x + y;
    }
    fun test() {
      var a = 1;
      var b = 2;
      add(a, b);
    }
  )");
  ASSERT_NE(Mod, nullptr);
  EXPECT_FALSE(DiagMgr->hasError());
}

//===----------------------------------------------------------------------===//
// Type Resolution Tests
//===----------------------------------------------------------------------===//

TEST_F(NameResolverTest, BuiltinTypeResolution) {
  auto Mod = parseAndResolve(R"(
    fun test() {
      var x: i32 = 42;
      var y: f64 = 3.14;
      var z: string = "hello";
    }
  )");
  ASSERT_NE(Mod, nullptr);
  EXPECT_FALSE(DiagMgr->hasError());
}

TEST_F(NameResolverTest, StructTypeResolution) {
  auto Mod = parseAndResolve(R"(
    struct Point {
      x: f64;
      y: f64;
    }
    fun test() {
      var p: Point = Point { x: 1.0, y: 2.0 };
    }
  )");
  ASSERT_NE(Mod, nullptr);
  EXPECT_FALSE(DiagMgr->hasError());
}

TEST_F(NameResolverTest, EnumTypeResolution) {
  auto Mod = parseAndResolve(R"(
    enum Result {
      Ok: i32;
      Err: string;
    }
    fun test() {
      var r: Result = Result::Ok(42);
    }
  )");
  ASSERT_NE(Mod, nullptr);
  EXPECT_FALSE(DiagMgr->hasError());
}

TEST_F(NameResolverTest, UndefinedType) {
  auto Mod = parseAndResolve(R"(
    fun test() {
      var x: UnknownType = 42;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  EXPECT_TRUE(DiagMgr->hasError());
}

TEST_F(NameResolverTest, PointerTypeResolution) {
  auto Mod = parseAndResolve(R"(
    fun test() {
      var x: *i32 = nullptr;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  EXPECT_FALSE(DiagMgr->hasError());
}

TEST_F(NameResolverTest, ReferenceTypeResolution) {
  auto Mod = parseAndResolve(R"(
    fun test() {
      var x: &i32 = nullptr;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  EXPECT_FALSE(DiagMgr->hasError());
}

//===----------------------------------------------------------------------===//
// ADT Resolution Tests
//===----------------------------------------------------------------------===//

TEST_F(NameResolverTest, StructFieldAccess) {
  auto Mod = parseAndResolve(R"(
    struct Point {
      x: f64;
      y: f64;
    }
    fun test() {
      var p = Point { x: 1.0, y: 2.0 };
      p.x;
      p.y;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  EXPECT_FALSE(DiagMgr->hasError());
}

TEST_F(NameResolverTest, StructMethodCall) {
  auto Mod = parseAndResolve(R"(
    struct Point {
      x: f64;
      y: f64;
      fun dot(const this, const other: Point) -> f64 {
        return this.x * other.x + this.y * other.y;
      }
    }
    fun test() {
      var p1 = Point { x: 1.0, y: 2.0 };
      var p2 = Point { x: 3.0, y: 4.0 };
      p1.dot(p2);
    }
  )");
  ASSERT_NE(Mod, nullptr);
  EXPECT_FALSE(DiagMgr->hasError());
}

TEST_F(NameResolverTest, StructConstructor) {
  auto Mod = parseAndResolve(R"(
    struct Point {
      x: f64;
      y: f64;
    }
    fun test() {
      var p = Point { x: 1.0, y: 2.0 };
    }
  )");
  ASSERT_NE(Mod, nullptr);
  EXPECT_FALSE(DiagMgr->hasError());
}

TEST_F(NameResolverTest, EnumVariantAccess) {
  auto Mod = parseAndResolve(R"(
    enum Result {
      Ok: i32;
      Err: string;
    }
    fun test() {
      var r = Result::Ok(42);
    }
  )");
  ASSERT_NE(Mod, nullptr);
  EXPECT_FALSE(DiagMgr->hasError());
}

TEST_F(NameResolverTest, UndefinedStruct) {
  auto Mod = parseAndResolve(R"(
    fun test() {
      var p = UnknownStruct { x: 1.0 };
    }
  )");
  ASSERT_NE(Mod, nullptr);
  EXPECT_TRUE(DiagMgr->hasError());
}

TEST_F(NameResolverTest, FieldNotFound) {
  auto Mod = parseAndResolve(R"(
    struct Point {
      x: f64;
      y: f64;
    }
    fun test() {
      var p = Point { x: 1.0, y: 2.0 };
      p.z;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  EXPECT_TRUE(DiagMgr->hasError());
}

//===----------------------------------------------------------------------===//
// Scope Management Tests
//===----------------------------------------------------------------------===//

TEST_F(NameResolverTest, GlobalScope) {
  auto Mod = parseAndResolve(R"(
    fun global() -> i32 {
      return 42;
    }
    fun test() {
      global();
    }
  )");
  ASSERT_NE(Mod, nullptr);
  EXPECT_FALSE(DiagMgr->hasError());
}

TEST_F(NameResolverTest, FunctionScope) {
  auto Mod = parseAndResolve(R"(
    fun test() {
      var x = 1;
      var y = 2;
      x + y;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  EXPECT_FALSE(DiagMgr->hasError());
}

TEST_F(NameResolverTest, BlockScope) {
  auto Mod = parseAndResolve(R"(
    fun test() {
      var x = 1;
      {
        var y = 2;
        x + y;
      }
      x;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  EXPECT_FALSE(DiagMgr->hasError());
}

TEST_F(NameResolverTest, NestedScopes) {
  auto Mod = parseAndResolve(R"(
    fun test() {
      var x = 1;
      {
        var y = 2;
        {
          var z = 3;
          x + y + z;
        }
        x + y;
      }
      x;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  EXPECT_FALSE(DiagMgr->hasError());
}

TEST_F(NameResolverTest, ScopeExit) {
  auto Mod = parseAndResolve(R"(
    fun test() {
      {
        var x = 1;
      }
      var y = 2;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  EXPECT_FALSE(DiagMgr->hasError());
}

TEST_F(NameResolverTest, ForLoopScope) {
  auto Mod = parseAndResolve(R"(
    fun test() {
      for i in 0..10 {
        var x = i;
        x;
      }
    }
  )");
  ASSERT_NE(Mod, nullptr);
  EXPECT_FALSE(DiagMgr->hasError());
}

//===----------------------------------------------------------------------===//
// Error Case Tests
//===----------------------------------------------------------------------===//

TEST_F(NameResolverTest, VariableRedeclaration) {
  auto Mod = parseAndResolve(R"(
    fun test() {
      var x = 1;
      var x = 2;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  EXPECT_TRUE(DiagMgr->hasError());
}

TEST_F(NameResolverTest, ParameterRedeclaration) {
  auto Mod = parseAndResolve(R"(
    fun test(const x: i32, const x: i32) {
      return;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  EXPECT_TRUE(DiagMgr->hasError());
}

TEST_F(NameResolverTest, FunctionRedeclaration) {
  auto Mod = parseAndResolve(R"(
    fun test() {
      return;
    }
    fun test() {
      return;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  EXPECT_TRUE(DiagMgr->hasError());
}

TEST_F(NameResolverTest, StructRedeclaration) {
  auto Mod = parseAndResolve(R"(
    struct Point {
      x: f64;
    }
    struct Point {
      y: f64;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  EXPECT_TRUE(DiagMgr->hasError());
}

TEST_F(NameResolverTest, EnumRedeclaration) {
  auto Mod = parseAndResolve(R"(
    enum Result {
      Ok: i32;
    }
    enum Result {
      Err: string;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  EXPECT_TRUE(DiagMgr->hasError());
}

TEST_F(NameResolverTest, TypeNotFound) {
  auto Mod = parseAndResolve(R"(
    fun test() {
      var x: UnknownType = 42;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  EXPECT_TRUE(DiagMgr->hasError());
}

TEST_F(NameResolverTest, VariantNotFound) {
  auto Mod = parseAndResolve(R"(
    enum Result {
      Ok: i32;
    }
    fun test() {
      var r = Result::Unknown(42);
    }
  )");
  ASSERT_NE(Mod, nullptr);
  EXPECT_TRUE(DiagMgr->hasError());
}

//===----------------------------------------------------------------------===//
// Complex Resolution Tests
//===----------------------------------------------------------------------===//

TEST_F(NameResolverTest, NestedFunctionCalls) {
  auto Mod = parseAndResolve(R"(
    fun add(const x: i32, const y: i32) -> i32 {
      return x + y;
    }
    fun test() {
      add(add(1, 2), add(3, 4));
    }
  )");
  ASSERT_NE(Mod, nullptr);
  EXPECT_FALSE(DiagMgr->hasError());
}

TEST_F(NameResolverTest, ComplexExpression) {
  auto Mod = parseAndResolve(R"(
    fun test() {
      var a = 1;
      var b = 2;
      var c = 3;
      (a + b) * c;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  EXPECT_FALSE(DiagMgr->hasError());
}

TEST_F(NameResolverTest, MethodChain) {
  auto Mod = parseAndResolve(R"(
    struct Point {
      x: f64;
      y: f64;
      fun getX(const this) -> f64 {
        return this.x;
      }
    }
    fun test() {
      var p = Point { x: 1.0, y: 2.0 };
      p.getX();
    }
  )");
  ASSERT_NE(Mod, nullptr);
  EXPECT_FALSE(DiagMgr->hasError());
}

TEST_F(NameResolverTest, ControlFlowWithVariables) {
  auto Mod = parseAndResolve(R"(
    fun test() {
      var x = 1;
      if x < 5 {
        var y = 2;
        x + y;
      } else {
        var z = 3;
        x + z;
      }
    }
  )");
  ASSERT_NE(Mod, nullptr);
  EXPECT_FALSE(DiagMgr->hasError());
}

TEST_F(NameResolverTest, LoopWithVariables) {
  auto Mod = parseAndResolve(R"(
    fun test() {
      var sum = 0;
      for i in 0..10 {
        sum = sum + i;
      }
      sum;
    }
  )");
  ASSERT_NE(Mod, nullptr);
  EXPECT_FALSE(DiagMgr->hasError());
}

} // namespace
