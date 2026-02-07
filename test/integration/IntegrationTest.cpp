//===----------------------------------------------------------------------===//
//
// Integration tests for the Phi programming language compiler
//
//===----------------------------------------------------------------------===//

#include <gtest/gtest.h>
#include <memory>
#include <string>
#include <vector>

#include "AST/Nodes/Decl.hpp"
#include "Diagnostics/DiagnosticManager.hpp"
#include "Lexer/Lexer.hpp"
#include "Parser/Parser.hpp"
#include "Sema/NameResolution/NameResolver.hpp"
#include "Sema/TypeInference/Inferencer.hpp"
#include "SrcManager/SrcManager.hpp"

using namespace phi;

namespace {

class IntegrationTest : public ::testing::Test {
protected:
  IntegrationTest() { DiagMgr = std::make_shared<DiagnosticManager>(); }

  bool compile(const std::string &Source,
               const std::string &Path = "test.phi") {
    DiagMgr->getSrcManager().addSrcFile(Path, Source);
    auto Tokens = Lexer(Source, Path, DiagMgr.get()).scan();
    Parser P(std::move(Tokens), DiagMgr.get());
    auto Mod = P.parse();
    if (!Mod)
      return false;

    std::vector<ModuleDecl *> Mods = {Mod.get()};
    NameResolver R(std::move(Mods), DiagMgr.get());
    auto ResolvedMod = R.resolve();
    if (DiagMgr->hasError())
      return false;

    // Type inference
    TypeInferencer TI(ResolvedMod, DiagMgr.get());
    TI.infer();

    return !DiagMgr->hasError();
  }

  std::shared_ptr<DiagnosticManager> DiagMgr;
};

//===----------------------------------------------------------------------===//
// Simple Program Tests
//===----------------------------------------------------------------------===//

TEST_F(IntegrationTest, HelloWorld) {
  std::string Source = R"(
    fun main() {
      return;
    }
  )";
  EXPECT_TRUE(compile(Source));
}

TEST_F(IntegrationTest, Arithmetic) {
  std::string Source = R"(
    fun main() {
      var x = 1 + 2;
      var y = 3 * 4;
      var z = x + y;
      return;
    }
  )";
  EXPECT_TRUE(compile(Source));
}

TEST_F(IntegrationTest, SimpleFunction) {
  std::string Source = R"(
    fun add(const x: i32, const y: i32) -> i32 {
      return x + y;
    }
    fun main() {
      var result = add(1, 2);
      return;
    }
  )";
  EXPECT_TRUE(compile(Source));
}

//===----------------------------------------------------------------------===//
// Control Flow Tests
//===----------------------------------------------------------------------===//

TEST_F(IntegrationTest, IfStatement) {
  std::string Source = R"(
    fun test(const x: i32) -> i32 {
      if x < 5 {
        return 1;
      } else {
        return 2;
      }
    }
  )";
  EXPECT_TRUE(compile(Source));
}

TEST_F(IntegrationTest, WhileLoop) {
  std::string Source = R"(
    fun test() {
      var i = 0;
      while i < 10 {
        i = i + 1;
      }
      return;
    }
  )";
  EXPECT_TRUE(compile(Source));
}

TEST_F(IntegrationTest, ForLoop) {
  std::string Source = R"(
    fun test() {
      var sum = 0;
      for i in 0..10 {
        sum = sum + i;
      }
      return;
    }
  )";
  EXPECT_TRUE(compile(Source));
}

TEST_F(IntegrationTest, NestedControlFlow) {
  std::string Source = R"(
    fun test() {
      var x = 0;
      while x < 10 {
        if x == 5 {
          break;
        }
        x = x + 1;
      }
      return;
    }
  )";
  EXPECT_TRUE(compile(Source));
}

//===----------------------------------------------------------------------===//
// Function Tests
//===----------------------------------------------------------------------===//

TEST_F(IntegrationTest, FunctionWithParameters) {
  std::string Source = R"(
    fun multiply(const a: i32, const b: i32) -> i32 {
      return a * b;
    }
    fun main() {
      var result = multiply(3, 4);
      return;
    }
  )";
  EXPECT_TRUE(compile(Source));
}

TEST_F(IntegrationTest, RecursiveFunction) {
  std::string Source = R"(
    fun factorial(const n: i32) -> i32 {
      if n == 0 {
        return 1;
      }
      return n * factorial(n - 1);
    }
  )";
  EXPECT_TRUE(compile(Source));
}

TEST_F(IntegrationTest, FunctionWithoutReturnType) {
  std::string Source = R"(
    fun test() {
      var x = 42;
      return;
    }
  )";
  EXPECT_TRUE(compile(Source));
}

//===----------------------------------------------------------------------===//
// Struct Tests
//===----------------------------------------------------------------------===//

TEST_F(IntegrationTest, StructDeclaration) {
  std::string Source = R"(
    struct Point {
      x: f64;
      y: f64;
    }
    fun main() {
      return;
    }
  )";
  EXPECT_TRUE(compile(Source));
}

TEST_F(IntegrationTest, StructInitialization) {
  std::string Source = R"(
    struct Point {
      x: f64;
      y: f64;
    }
    fun main() {
      var p = Point { x: 1.0, y: 2.0 };
      return;
    }
  )";
  EXPECT_TRUE(compile(Source));
}

TEST_F(IntegrationTest, StructFieldAccess) {
  std::string Source = R"(
    struct Point {
      x: f64;
      y: f64;
    }
    fun main() {
      var p = Point { x: 1.0, y: 2.0 };
      var x = p.x;
      var y = p.y;
      return;
    }
  )";
  EXPECT_TRUE(compile(Source));
}

TEST_F(IntegrationTest, StructMethod) {
  std::string Source = R"(
    struct Point {
      x: f64;
      y: f64;
      fun dot(const this, const other: Point) -> f64 {
        return this.x * other.x + this.y * other.y;
      }
    }
    fun main() {
      var p1 = Point { x: 1.0, y: 2.0 };
      var p2 = Point { x: 3.0, y: 4.0 };
      var result = p1.dot(p2);
      return;
    }
  )";
  EXPECT_TRUE(compile(Source));
}

//===----------------------------------------------------------------------===//
// Enum Tests
//===----------------------------------------------------------------------===//

TEST_F(IntegrationTest, EnumDeclaration) {
  std::string Source = R"(
    enum Result {
      Ok: i32;
      Err: string;
    }
    fun main() {
      return;
    }
  )";
  EXPECT_TRUE(compile(Source));
}

TEST_F(IntegrationTest, EnumVariantAccess) {
  std::string Source = R"(
    enum Result {
      Ok: i32;
      Err: string;
    }
    fun main() {
      var r = Result::Ok(42);
      return;
    }
  )";
  EXPECT_TRUE(compile(Source));
}

//===----------------------------------------------------------------------===//
// Error Case Tests
//===----------------------------------------------------------------------===//

TEST_F(IntegrationTest, UndeclaredVariable) {
  std::string Source = R"(
    fun test() {
      x;
    }
  )";
  EXPECT_FALSE(compile(Source));
  EXPECT_TRUE(DiagMgr->hasError());
}

TEST_F(IntegrationTest, UndeclaredFunction) {
  std::string Source = R"(
    fun test() {
      unknown();
    }
  )";
  EXPECT_FALSE(compile(Source));
  EXPECT_TRUE(DiagMgr->hasError());
}

TEST_F(IntegrationTest, TypeMismatch) {
  std::string Source = R"(
    fun test() {
      var x: i32 = "hello";
    }
  )";
  EXPECT_FALSE(compile(Source));
  EXPECT_TRUE(DiagMgr->hasError());
}

TEST_F(IntegrationTest, VariableRedeclaration) {
  std::string Source = R"(
    fun test() {
      var x = 1;
      var x = 2;
    }
  )";
  EXPECT_FALSE(compile(Source));
  EXPECT_TRUE(DiagMgr->hasError());
}

//===----------------------------------------------------------------------===//
// Complex Program Tests
//===----------------------------------------------------------------------===//

TEST_F(IntegrationTest, ComplexProgram) {
  std::string Source = R"(
    struct Vector2D {
      x: f64;
      y: f64;
      fun add(const this, const other: Vector2D) -> Vector2D {
        return Vector2D { x: this.x + other.x, y: this.y + other.y };
      }
    }
    fun compute(const v1: Vector2D, const v2: Vector2D) -> f64 {
      var sum = v1.add(v2);
      return sum.x + sum.y;
    }
    fun main() {
      var v1 = Vector2D { x: 1.0, y: 2.0 };
      var v2 = Vector2D { x: 3.0, y: 4.0 };
      var result = compute(v1, v2);
      return;
    }
  )";
  EXPECT_TRUE(compile(Source));
}

TEST_F(IntegrationTest, MultipleFunctions) {
  std::string Source = R"(
    fun add(const x: i32, const y: i32) -> i32 {
      return x + y;
    }
    fun multiply(const x: i32, const y: i32) -> i32 {
      return x * y;
    }
    fun main() {
      var a = add(1, 2);
      var b = multiply(3, 4);
      var c = add(a, b);
      return;
    }
  )";
  EXPECT_TRUE(compile(Source));
}

TEST_F(IntegrationTest, NestedScopes) {
  std::string Source = R"(
    fun test() {
      var x = 1;
      {
        var y = 2;
        {
          var z = 3;
          x + y + z;
        }
      }
      return;
    }
  )";
  EXPECT_TRUE(compile(Source));
}

} // namespace
