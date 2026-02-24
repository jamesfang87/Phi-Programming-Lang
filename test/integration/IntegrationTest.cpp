#include <gtest/gtest.h>

#include "Diagnostics/DiagnosticManager.hpp"
#include "Lexer/Lexer.hpp"
#include "Parser/Parser.hpp"
#include "Sema/Sema.hpp"
#include "CodeGen/LLVMCodeGen.hpp"

#include "AST/Nodes/Decl.hpp"

#include <llvm/IR/Module.h>
#include <llvm/IR/Verifier.h>

#include <memory>
#include <string>
#include <vector>

using namespace phi;

// -----------------------------------------------------------------------
// Test fixture holding DiagnosticManager + ModuleDecl ownership
// -----------------------------------------------------------------------

struct PipelineResult {
  DiagnosticManager Diags;
  std::unique_ptr<ModuleDecl> Mod;

  PipelineResult() : Diags(DiagnosticConfig{.UseColors = false}) {}
};

// Helper: run full frontend (lex → parse → sema), return result
static PipelineResult frontend(const std::string &Src) {
  PipelineResult R;
  R.Diags.getSrcManager().addSrcFile("test.phi", Src);

  Lexer L(Src, "test.phi", &R.Diags);
  auto Tokens = L.scan();
  if (R.Diags.hasError())
    return R;

  Parser P(Tokens, &R.Diags);
  R.Mod = P.parse();
  if (!R.Mod || R.Diags.hasError())
    return R;

  std::vector<ModuleDecl *> Mods = {R.Mod.get()};
  Sema S(Mods, &R.Diags);
  S.analyze();
  return R;
}

// Helper: frontend succeeds
static bool frontendOk(const std::string &Src) {
  auto R = frontend(Src);
  return R.Mod && !R.Diags.hasError();
}

// Helper: frontend and codegen succeed, LLVM module passes verification
static bool fullPipeline(const std::string &Src) {
  auto R = frontend(Src);
  if (!R.Mod || R.Diags.hasError())
    return false;

  std::vector<ModuleDecl *> Mods = {R.Mod.get()};
  CodeGen CG(Mods, "test");
  CG.generate();

  std::string ErrorStr;
  llvm::raw_string_ostream ErrorOS(ErrorStr);
  if (llvm::verifyModule(CG.getModule(), &ErrorOS)) {
    ADD_FAILURE() << "LLVM Module verification failed:\n" << ErrorStr;
    return false;
  }
  return true;
}

//===----------------------------------------------------------------------===//
// Simple Programs (full pipeline with codegen)
//===----------------------------------------------------------------------===//

TEST(Integration, EmptyMain) {
  EXPECT_TRUE(fullPipeline("fun main() {}"));
}

TEST(Integration, ReturnValue) {
  EXPECT_TRUE(fullPipeline(R"(
    fun main() -> i32 {
      return 0;
    }
  )"));
}

TEST(Integration, Arithmetic) {
  EXPECT_TRUE(fullPipeline(R"(
    fun main() -> i32 {
      return 1 + 2 * 3;
    }
  )"));
}

TEST(Integration, MultipleArithmetic) {
  EXPECT_TRUE(fullPipeline(R"(
    fun main() -> i32 {
      const a = 10;
      const b = 20;
      const c = a + b;
      const d = c * 2;
      return d - a;
    }
  )"));
}

//===----------------------------------------------------------------------===//
// Control Flow
//===----------------------------------------------------------------------===//

TEST(Integration, IfElse) {
  EXPECT_TRUE(fullPipeline(R"(
    fun max(const a: i32, const b: i32) -> i32 {
      if a > b {
        return a;
      } else {
        return b;
      }
    }

    fun main() -> i32 {
      return max(3, 5);
    }
  )"));
}

TEST(Integration, WhileLoop) {
  EXPECT_TRUE(fullPipeline(R"(
    fun main() -> i32 {
      var sum = 0;
      var i = 0;
      while i < 10 {
        sum += i;
        i++;
      }
      return sum;
    }
  )"));
}

TEST(Integration, ForLoop) {
  EXPECT_TRUE(fullPipeline(R"(
    fun main() -> i32 {
      var sum = 0;
      for i in 0..10 {
        sum += i;
      }
      return sum;
    }
  )"));
}

TEST(Integration, BreakInLoop) {
  EXPECT_TRUE(fullPipeline(R"(
    fun main() -> i32 {
      var i = 0;
      while true {
        if i == 5 {
          break;
        }
        i++;
      }
      return i;
    }
  )"));
}

//===----------------------------------------------------------------------===//
// Functions
//===----------------------------------------------------------------------===//

TEST(Integration, FunctionCalls) {
  EXPECT_TRUE(fullPipeline(R"(
    fun add(const a: i32, const b: i32) -> i32 {
      return a + b;
    }

    fun mul(const a: i32, const b: i32) -> i32 {
      return a * b;
    }

    fun main() -> i32 {
      return add(mul(2, 3), 4);
    }
  )"));
}

TEST(Integration, RecursiveFibonacci) {
  EXPECT_TRUE(fullPipeline(R"(
    fun fib(const n: i32) -> i32 {
      if n < 2 {
        return n;
      }
      return fib(n - 1) + fib(n - 2);
    }

    fun main() -> i32 {
      return fib(10);
    }
  )"));
}

//===----------------------------------------------------------------------===//
// Structs
//===----------------------------------------------------------------------===//

TEST(Integration, StructCreation) {
  EXPECT_TRUE(fullPipeline(R"(
    struct Point {
      public x: f64,
      public y: f64
    }

    fun main() {
      const p = Point { x : 1.0, y : 2.0 };
    }
  )"));
}

TEST(Integration, StructFieldAccess) {
  EXPECT_TRUE(fullPipeline(R"(
    struct Point {
      public x: f64,
      public y: f64
    }

    fun main() -> f64 {
      const p = Point { x : 3.0, y : 4.0 };
      return p.x + p.y;
    }
  )"));
}

TEST(Integration, StructWithMethod) {
  EXPECT_TRUE(fullPipeline(R"(
    struct Vec2 {
      public x: f64,
      public y: f64,

      fun dot(const this, const other: Vec2) -> f64 {
        return this.x * other.x + this.y * other.y;
      }
    }

    fun main() -> f64 {
      const a = Vec2 { x : 1.0, y : 2.0 };
      const b = Vec2 { x : 3.0, y : 4.0 };
      return a.dot(b);
    }
  )"));
}

//===----------------------------------------------------------------------===//
// Enums
//===----------------------------------------------------------------------===//

TEST(Integration, SimpleEnum) {
  EXPECT_TRUE(fullPipeline(R"(
    enum Color { Red, Green, Blue }

    fun main() {
      const c = Color { Red };
    }
  )"));
}

TEST(Integration, EnumWithMatch) {
  EXPECT_TRUE(fullPipeline(R"(
    enum Color { Red, Green, Blue }

    fun color_value(const c: Color) -> i32 {
      return match c {
        .Red => 0,
        .Green => 1,
        .Blue => 2,
      };
    }

    fun main() -> i32 {
      const c = Color { Green };
      return color_value(c);
    }
  )"));
}

TEST(Integration, EnumWithPayload) {
  EXPECT_TRUE(fullPipeline(R"(
    enum Shape {
      Circle: f64,
      Square: f64
    }

    fun area(const s: Shape) -> f64 {
      return match s {
        .Circle(r) => 3.14159 * r * r,
        .Square(side) => side * side,
      };
    }

    fun main() -> f64 {
      const s = Shape { Circle : 5.0 };
      return area(s);
    }
  )"));
}

TEST(Integration, EnumMethod) {
  EXPECT_TRUE(fullPipeline(R"(
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

    fun main() -> bool {
      const r = Result { Ok : 42 };
      return r.is_ok();
    }
  )"));
}

//===----------------------------------------------------------------------===//
// Tuples
//===----------------------------------------------------------------------===//

TEST(Integration, TupleCreation) {
  EXPECT_TRUE(fullPipeline(R"(
    fun main() {
      const t = (1, 2.0, true);
    }
  )"));
}

TEST(Integration, TupleReturn) {
  EXPECT_TRUE(fullPipeline(R"(
    fun swap(const a: i32, const b: i32) -> (i32, i32) {
      return (b, a);
    }

    fun main() -> i32 {
      const pair = swap(1, 2);
      return pair.0;
    }
  )"));
}

TEST(Integration, TupleIndex) {
  EXPECT_TRUE(fullPipeline(R"(
    fun main() -> i32 {
      const t = (10, 20, 30);
      return t.0 + t.1 + t.2;
    }
  )"));
}

//===----------------------------------------------------------------------===//
// Arrays
//===----------------------------------------------------------------------===//

TEST(Integration, ArrayLiteral) {
  EXPECT_TRUE(fullPipeline(R"(
    fun main() {
      const arr = [1, 2, 3, 4, 5];
    }
  )"));
}

TEST(Integration, ArrayIndex) {
  EXPECT_TRUE(fullPipeline(R"(
    fun main() -> i32 {
      const arr = [10, 20, 30];
      return arr[0] + arr[1] + arr[2];
    }
  )"));
}

TEST(Integration, ArrayWithLoop) {
  EXPECT_TRUE(fullPipeline(R"(
    fun main() -> i32 {
      const arr = [1, 2, 3, 4, 5];
      var sum = 0;
      for i in 0..5 {
        sum += arr[i];
      }
      return sum;
    }
  )"));
}

//===----------------------------------------------------------------------===//
// Generics with Monomorphization
//===----------------------------------------------------------------------===//

TEST(Integration, GenericStruct) {
  EXPECT_TRUE(fullPipeline(R"(
    struct Box<T> {
      public value: T
    }

    fun main() {
      const a = Box::<i32> { value : 42 };
      const b = Box::<f64> { value : 3.14 };
    }
  )"));
}

TEST(Integration, GenericEnum) {
  EXPECT_TRUE(fullPipeline(R"(
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

TEST(Integration, GenericEnumWithMatch) {
  EXPECT_TRUE(fullPipeline(R"(
    enum Option<T> {
      Some: T,
      None
    }

    fun unwrap_or(const opt: Option<i32>, const def: i32) -> i32 {
      return match opt {
        .Some(v) => v,
        .None => def,
      };
    }

    fun main() -> i32 {
      const x = Option::<i32> { Some : 42 };
      return unwrap_or(x, 0);
    }
  )"));
}

TEST(Integration, GenericStructWithMethod) {
  EXPECT_TRUE(fullPipeline(R"(
    struct Box<T> {
      public value: T,

      fun get(const this) -> T {
        return this.value;
      }
    }

    fun main() -> i32 {
      const b = Box::<i32> { value : 42 };
      return b.get();
    }
  )"));
}

TEST(Integration, MultipleGenericInstantiations) {
  EXPECT_TRUE(fullPipeline(R"(
    struct Pair<A, B> {
      public first: A,
      public second: B
    }

    fun main() {
      const p1 = Pair::<i32, f64> { first : 1, second : 2.0 };
      const p2 = Pair::<bool, i32> { first : true, second : 42 };
    }
  )"));
}

//===----------------------------------------------------------------------===//
// Complex Programs
//===----------------------------------------------------------------------===//

TEST(Integration, VectorDotProduct) {
  EXPECT_TRUE(fullPipeline(R"(
    struct Vec3 {
      public x: f64,
      public y: f64,
      public z: f64,

      fun dot(const this, const other: Vec3) -> f64 {
        return this.x * other.x + this.y * other.y + this.z * other.z;
      }

      fun add(const this, const other: Vec3) -> Vec3 {
        return Vec3 {
          x : this.x + other.x,
          y : this.y + other.y,
          z : this.z + other.z
        };
      }
    }

    fun main() -> f64 {
      const a = Vec3 { x : 1.0, y : 2.0, z : 3.0 };
      const b = Vec3 { x : 4.0, y : 5.0, z : 6.0 };
      const c = a.add(b);
      return a.dot(b);
    }
  )"));
}

TEST(Integration, NestedControlFlow) {
  EXPECT_TRUE(fullPipeline(R"(
    fun classify(const n: i32) -> i32 {
      if n < 0 {
        return -1;
      } else {
        if n == 0 {
          return 0;
        } else {
          return 1;
        }
      }
    }

    fun main() -> i32 {
      return classify(-5) + classify(0) + classify(5);
    }
  )"));
}

TEST(Integration, MutationAndAssignment) {
  EXPECT_TRUE(fullPipeline(R"(
    fun main() -> i32 {
      var x = 0;
      x = 10;
      x += 5;
      x -= 2;
      x *= 3;
      return x;
    }
  )"));
}

//===----------------------------------------------------------------------===//
// Semantic Error Cases (frontend should fail)
//===----------------------------------------------------------------------===//

TEST(Integration, UndefinedVariable) {
  EXPECT_FALSE(frontendOk(R"(
    fun main() -> i32 {
      return x;
    }
  )"));
}

TEST(Integration, UndefinedFunction) {
  EXPECT_FALSE(frontendOk(R"(
    fun main() {
      nonexistent();
    }
  )"));
}

TEST(Integration, TypeMismatch) {
  EXPECT_FALSE(frontendOk(R"(
    fun main() {
      const x: i32 = "hello";
    }
  )"));
}

TEST(Integration, ReturnTypeMismatch) {
  EXPECT_FALSE(frontendOk(R"(
    fun main() -> i32 {
      return "not an int";
    }
  )"));
}

TEST(Integration, Redefinition) {
  EXPECT_FALSE(frontendOk(R"(
    fun foo() {}
    fun foo() {}
  )"));
}

TEST(Integration, WrongArgCount) {
  EXPECT_FALSE(frontendOk(R"(
    fun add(const a: i32, const b: i32) -> i32 { return a + b; }
    fun main() {
      const x = add(1);
    }
  )"));
}
