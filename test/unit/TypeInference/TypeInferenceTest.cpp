#include <gtest/gtest.h>

#include "Diagnostics/DiagnosticManager.hpp"
#include "Lexer/Lexer.hpp"
#include "Parser/Parser.hpp"
#include "Sema/NameResolution/NameResolver.hpp"
#include "Sema/TypeInference/Inferencer.hpp"

#include "AST/Nodes/Decl.hpp"
#include "AST/TypeSystem/Type.hpp"

#include <memory>
#include <string>
#include <vector>

using namespace phi;

// Helper: run full sema pipeline (name resolve + type inference)
// Returns true if no errors
static bool sema(const std::string &Src) {
  DiagnosticConfig Cfg;
  Cfg.UseColors = false;
  DiagnosticManager Diags(Cfg);
  Diags.getSrcManager().addSrcFile("test.phi", Src);

  Lexer L(Src, "test.phi", &Diags);
  auto Tokens = L.scan();
  if (Diags.hasError())
    return false;

  Parser P(Tokens, &Diags);
  auto Mod = P.parse();
  if (!Mod || Diags.hasError())
    return false;

  std::vector<ModuleDecl *> Mods = {Mod.get()};
  auto Resolved = NameResolver(Mods, &Diags).resolve();
  if (Diags.hasError())
    return false;

  TypeInferencer(Resolved, &Diags).infer();
  return !Diags.hasError();
}

//===----------------------------------------------------------------------===//
// Literal Type Inference
//===----------------------------------------------------------------------===//

TEST(TypeInference, IntLiteral) {
  EXPECT_TRUE(sema(R"(
    fun main() {
      const x = 42;
    }
  )"));
}

TEST(TypeInference, FloatLiteral) {
  EXPECT_TRUE(sema(R"(
    fun main() {
      const x = 3.14;
    }
  )"));
}

TEST(TypeInference, BoolLiteral) {
  EXPECT_TRUE(sema(R"(
    fun main() {
      const x = true;
      const y = false;
    }
  )"));
}

TEST(TypeInference, StringLiteral) {
  EXPECT_TRUE(sema(R"(
    fun main() {
      const x = "hello";
    }
  )"));
}

TEST(TypeInference, CharLiteral) {
  EXPECT_TRUE(sema(R"(
    fun main() {
      const x = 'a';
    }
  )"));
}

//===----------------------------------------------------------------------===//
// Variable Type Inference from Initializer
//===----------------------------------------------------------------------===//

TEST(TypeInference, VarInferFromInt) {
  EXPECT_TRUE(sema(R"(
    fun main() {
      const x = 5;
      const y: i32 = x;
    }
  )"));
}

TEST(TypeInference, VarInferFromExpression) {
  EXPECT_TRUE(sema(R"(
    fun main() {
      const x = 1 + 2;
    }
  )"));
}

TEST(TypeInference, VarExplicitType) {
  EXPECT_TRUE(sema(R"(
    fun main() {
      const x: i32 = 5;
    }
  )"));
}

//===----------------------------------------------------------------------===//
// Type Mismatch Errors
//===----------------------------------------------------------------------===//

TEST(TypeInference, TypeMismatchAssign) {
  EXPECT_FALSE(sema(R"(
    fun main() {
      const x: i32 = "hello";
    }
  )"));
}

TEST(TypeInference, ReturnTypeMismatch) {
  EXPECT_FALSE(sema(R"(
    fun foo() -> i32 {
      return "hello";
    }
  )"));
}

TEST(TypeInference, ParamTypeMismatch) {
  EXPECT_FALSE(sema(R"(
    fun foo(const x: i32) {}
    fun main() { foo("hello"); }
  )"));
}

//===----------------------------------------------------------------------===//
// Binary Expression Type Inference
//===----------------------------------------------------------------------===//

TEST(TypeInference, ArithmeticOps) {
  EXPECT_TRUE(sema(R"(
    fun main() {
      const a = 1 + 2;
      const b = 3 - 1;
      const c = 2 * 3;
      const d = 10 / 2;
      const e = 10 % 3;
    }
  )"));
}

TEST(TypeInference, ComparisonOps) {
  EXPECT_TRUE(sema(R"(
    fun main() {
      const a = 1 < 2;
      const b = 1 > 2;
      const c = 1 <= 2;
      const d = 1 >= 2;
      const e = 1 == 2;
      const f = 1 != 2;
    }
  )"));
}

TEST(TypeInference, LogicalOps) {
  EXPECT_TRUE(sema(R"(
    fun main() {
      const a = true && false;
      const b = true || false;
    }
  )"));
}

TEST(TypeInference, FloatArithmetic) {
  EXPECT_TRUE(sema(R"(
    fun main() {
      const x: f64 = 1.0 + 2.0;
      const y: f64 = 3.0 * 4.0;
    }
  )"));
}

//===----------------------------------------------------------------------===//
// Unary Expression Type Inference
//===----------------------------------------------------------------------===//

TEST(TypeInference, UnaryNegate) {
  EXPECT_TRUE(sema(R"(
    fun main() {
      const x = -42;
    }
  )"));
}

TEST(TypeInference, UnaryNot) {
  EXPECT_TRUE(sema(R"(
    fun main() {
      const x = !true;
    }
  )"));
}

//===----------------------------------------------------------------------===//
// Function Return Type
//===----------------------------------------------------------------------===//

TEST(TypeInference, FunctionReturnType) {
  EXPECT_TRUE(sema(R"(
    fun foo() -> i32 {
      return 42;
    }
    fun main() {
      const x = foo();
    }
  )"));
}

TEST(TypeInference, FunctionMultipleReturns) {
  EXPECT_TRUE(sema(R"(
    fun abs(const x: i32) -> i32 {
      if x < 0 {
        return -x;
      }
      return x;
    }
  )"));
}

//===----------------------------------------------------------------------===//
// Struct Type Inference
//===----------------------------------------------------------------------===//

TEST(TypeInference, StructInit) {
  EXPECT_TRUE(sema(R"(
    struct Point { public x: f64, public y: f64 }
    fun main() {
      const p = Point { x : 1.0, y : 2.0 };
    }
  )"));
}

TEST(TypeInference, StructFieldAccess) {
  EXPECT_TRUE(sema(R"(
    struct Point { public x: f64, public y: f64 }
    fun main() {
      const p = Point { x : 1.0, y : 2.0 };
      const val: f64 = p.x;
    }
  )"));
}

TEST(TypeInference, StructFieldTypeMismatch) {
  EXPECT_FALSE(sema(R"(
    struct Point { public x: f64, public y: f64 }
    fun main() {
      const p = Point { x : "hello", y : 2.0 };
    }
  )"));
}

//===----------------------------------------------------------------------===//
// Enum Type Inference
//===----------------------------------------------------------------------===//

TEST(TypeInference, EnumInit) {
  EXPECT_TRUE(sema(R"(
    enum Color { Red, Green, Blue }
    fun main() {
      const c = Color { Red };
    }
  )"));
}

TEST(TypeInference, EnumPayloadInit) {
  EXPECT_TRUE(sema(R"(
    enum Shape {
      Circle: f64,
      Square: f64
    }
    fun main() {
      const s = Shape { Circle : 5.0 };
    }
  )"));
}

TEST(TypeInference, EnumMatch) {
  EXPECT_TRUE(sema(R"(
    enum Color { Red, Green, Blue }
    fun main() {
      const c = Color { Red };
      const x: i32 = match c {
        .Red => 1,
        .Green => 2,
        .Blue => 3,
      };
    }
  )"));
}

//===----------------------------------------------------------------------===//
// Array Type Inference
//===----------------------------------------------------------------------===//

TEST(TypeInference, ArrayLiteral) {
  EXPECT_TRUE(sema(R"(
    fun main() {
      const arr = [1, 2, 3];
    }
  )"));
}

TEST(TypeInference, ArrayExplicitType) {
  EXPECT_TRUE(sema(R"(
    fun main() {
      const arr: [i32] = [1, 2, 3];
    }
  )"));
}

TEST(TypeInference, ArrayIndex) {
  EXPECT_TRUE(sema(R"(
    fun main() {
      const arr = [10, 20, 30];
      const x = arr[0];
    }
  )"));
}

TEST(TypeInference, NestedArray) {
  EXPECT_TRUE(sema(R"(
    fun main() {
      const arr: [[i32]] = [[1, 2], [3, 4]];
    }
  )"));
}

//===----------------------------------------------------------------------===//
// Tuple Type Inference
//===----------------------------------------------------------------------===//

TEST(TypeInference, TupleLiteral) {
  EXPECT_TRUE(sema(R"(
    fun main() {
      const t = (1, 2.0, true);
    }
  )"));
}

TEST(TypeInference, TupleExplicitType) {
  EXPECT_TRUE(sema(R"(
    fun main() {
      const t: (i32, f64) = (1, 2.0);
    }
  )"));
}

TEST(TypeInference, TupleIndex) {
  EXPECT_TRUE(sema(R"(
    fun main() {
      const t = (1, 2.0);
      const x = t.0;
    }
  )"));
}

TEST(TypeInference, TupleReturn) {
  EXPECT_TRUE(sema(R"(
    fun pair() -> (i32, f64) {
      return (42, 3.14);
    }
    fun main() {
      const p = pair();
    }
  )"));
}

//===----------------------------------------------------------------------===//
// Generic Type Inference
//===----------------------------------------------------------------------===//

TEST(TypeInference, GenericStruct) {
  EXPECT_TRUE(sema(R"(
    struct Box<T> { public value: T }
    fun main() {
      const b = Box::<i32> { value : 42 };
    }
  )"));
}

TEST(TypeInference, GenericEnum) {
  EXPECT_TRUE(sema(R"(
    enum Option<T> {
      Some: T,
      None
    }
    fun main() {
      const x = Option::<i32> { Some : 42 };
      const y = Option::<i32> { None };
    }
  )"));
}

TEST(TypeInference, GenericFunction) {
  EXPECT_TRUE(sema(R"(
    fun identity<T>(const x: T) -> T { return x; }
    fun main() {
      const y = identity::<i32>(5);
    }
  )"));
}

TEST(TypeInference, GenericPair) {
  EXPECT_TRUE(sema(R"(
    struct Pair<A, B> {
      public first: A,
      public second: B
    }
    fun main() {
      const p = Pair::<i32, f64> { first : 1, second : 2.0 };
    }
  )"));
}

TEST(TypeInference, GenericFieldTypeMismatch) {
  EXPECT_FALSE(sema(R"(
    struct Box<T> { public value: T }
    fun main() {
      const b = Box::<i32> { value : "oops" };
    }
  )"));
}

//===----------------------------------------------------------------------===//
// Control Flow Type Checking
//===----------------------------------------------------------------------===//

TEST(TypeInference, IfConditionBool) {
  EXPECT_TRUE(sema(R"(
    fun main() {
      if true {
        const x = 1;
      }
    }
  )"));
}

TEST(TypeInference, WhileConditionBool) {
  EXPECT_TRUE(sema(R"(
    fun main() {
      var i = 0;
      while i < 10 {
        i++;
      }
    }
  )"));
}

TEST(TypeInference, ForLoop) {
  EXPECT_TRUE(sema(R"(
    fun main() {
      for i in 0..10 {
        const x = i;
      }
    }
  )"));
}

//===----------------------------------------------------------------------===//
// Complex Programs
//===----------------------------------------------------------------------===//

TEST(TypeInference, StructWithMethods) {
  EXPECT_TRUE(sema(R"(
    struct Vec2 {
      public x: f64,
      public y: f64,

      fun dot(const this, const other: Vec2) -> f64 {
        return this.x * other.x + this.y * other.y;
      }
    }

    fun main() {
      const a = Vec2 { x : 1.0, y : 2.0 };
      const b = Vec2 { x : 3.0, y : 4.0 };
      const d: f64 = a.dot(b);
    }
  )"));
}

TEST(TypeInference, EnumMethodAndMatch) {
  EXPECT_TRUE(sema(R"(
    enum Result {
      Ok: i32,
      Err: i32,

      fun is_ok(const this) -> bool {
        return match this {
          .Ok => true,
          .Err => false,
        };
      }
    }

    fun main() {
      const r = Result { Ok : 42 };
      const ok: bool = r.is_ok();
    }
  )"));
}

TEST(TypeInference, NestedGeneric) {
  EXPECT_TRUE(sema(R"(
    struct Box<T> { public value: T }
    struct Pair<A, B> {
      public first: A,
      public second: B
    }

    fun main() {
      const p = Pair::<Box<i32>, Box<f64>> {
        first : Box::<i32> { value : 1 },
        second : Box::<f64> { value : 2.0 }
      };
    }
  )"));
}
