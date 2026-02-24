#include <gtest/gtest.h>

#include "Diagnostics/DiagnosticManager.hpp"
#include "Lexer/Lexer.hpp"
#include "Parser/Parser.hpp"
#include "Sema/NameResolution/NameResolver.hpp"

#include "AST/Nodes/Decl.hpp"

#include <memory>
#include <string>
#include <vector>

using namespace phi;

// Helper: lex -> parse -> name-resolve, return success/failure
static bool resolve(const std::string &Src) {
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
  NameResolver NR(Mods, &Diags);
  NR.resolve();
  return !Diags.hasError();
}

//===----------------------------------------------------------------------===//
// Variable Resolution
//===----------------------------------------------------------------------===//

TEST(NameResolver, DefinedVariable) {
  EXPECT_TRUE(resolve(R"(
    fun main() {
      const x = 5;
      const y = x;
    }
  )"));
}

TEST(NameResolver, UndefinedVariable) {
  EXPECT_FALSE(resolve(R"(
    fun main() {
      const y = x;
    }
  )"));
}

TEST(NameResolver, VariableShadowing) {
  EXPECT_FALSE(resolve(R"(
    fun main() {
      const x = 5;
      if true {
        const x = 10;
        const y = x;
      }
    }
  )"));
}

TEST(NameResolver, BlockScoping) {
  EXPECT_FALSE(resolve(R"(
    fun main() {
      {
        const x = 5;
      }
      const y = x;
    }
  )"));
}

//===----------------------------------------------------------------------===//
// Function Resolution
//===----------------------------------------------------------------------===//

TEST(NameResolver, DefinedFunction) {
  EXPECT_TRUE(resolve(R"(
    fun foo() {}
    fun main() { foo(); }
  )"));
}

TEST(NameResolver, UndefinedFunction) {
  EXPECT_FALSE(resolve(R"(
    fun main() { bar(); }
  )"));
}

TEST(NameResolver, ForwardReference) {
  // Functions defined later should still be visible
  EXPECT_TRUE(resolve(R"(
    fun main() { helper(); }
    fun helper() {}
  )"));
}

TEST(NameResolver, RecursiveFunction) {
  EXPECT_TRUE(resolve(R"(
    fun fib(const n: i32) -> i32 {
      if n < 2 { return n; }
      return fib(n - 1) + fib(n - 2);
    }
  )"));
}

//===----------------------------------------------------------------------===//
// Struct Resolution
//===----------------------------------------------------------------------===//

TEST(NameResolver, StructTypeResolution) {
  EXPECT_TRUE(resolve(R"(
    struct Point { public x: f64, public y: f64 }
    fun main() { const p = Point { x : 1.0, y : 2.0 }; }
  )"));
}

TEST(NameResolver, UndefinedType) {
  EXPECT_FALSE(resolve(R"(
    fun main() { const p = Unknown { x : 1 }; }
  )"));
}

TEST(NameResolver, StructFieldAccess) {
  EXPECT_TRUE(resolve(R"(
    struct Point { public x: f64, public y: f64 }
    fun main() {
      const p = Point { x : 1.0, y : 2.0 };
      const val = p.x;
    }
  )"));
}

TEST(NameResolver, StructMethodCall) {
  EXPECT_TRUE(resolve(R"(
    struct Vec2 {
      public x: f64,
      public y: f64,

      fun len(const this) -> f64 {
        return this.x;
      }
    }

    fun main() {
      const v = Vec2 { x : 3.0, y : 4.0 };
      const l = v.len();
    }
  )"));
}

//===----------------------------------------------------------------------===//
// Enum Resolution
//===----------------------------------------------------------------------===//

TEST(NameResolver, EnumResolution) {
  EXPECT_TRUE(resolve(R"(
    enum Color { Red, Green, Blue }
    fun main() {
      const c = Color { Red };
    }
  )"));
}

TEST(NameResolver, EnumWithPayload) {
  EXPECT_TRUE(resolve(R"(
    enum Shape {
      Circle: f64,
      Square: f64
    }
    fun main() {
      const s = Shape { Circle : 5.0 };
    }
  )"));
}

TEST(NameResolver, EnumMatch) {
  EXPECT_TRUE(resolve(R"(
    enum Color { Red, Green, Blue }
    fun main() {
      const c = Color { Red };
      const x = match c {
        .Red => 1,
        .Green => 2,
        .Blue => 3,
      };
    }
  )"));
}

//===----------------------------------------------------------------------===//
// Redefinition Errors
//===----------------------------------------------------------------------===//

TEST(NameResolver, FunctionRedefinition) {
  EXPECT_FALSE(resolve(R"(
    fun foo() {}
    fun foo() {}
  )"));
}

TEST(NameResolver, StructRedefinition) {
  EXPECT_FALSE(resolve(R"(
    struct S { public x: i32 }
    struct S { public y: i32 }
  )"));
}

//===----------------------------------------------------------------------===//
// Generics
//===----------------------------------------------------------------------===//

TEST(NameResolver, GenericStruct) {
  EXPECT_TRUE(resolve(R"(
    struct Box<T> { public value: T }
    fun main() {
      const b = Box::<i32> { value : 42 };
    }
  )"));
}

TEST(NameResolver, GenericEnum) {
  EXPECT_TRUE(resolve(R"(
    enum Option<T> {
      Some: T,
      None
    }
    fun main() {
      const x = Option::<i32> { Some : 42 };
    }
  )"));
}

TEST(NameResolver, GenericFunction) {
  EXPECT_TRUE(resolve(R"(
    fun identity<T>(const x: T) -> T { return x; }
    fun main() {
      const y = identity::<i32>(5);
    }
  )"));
}

TEST(NameResolver, MultipleTypeArgs) {
  EXPECT_TRUE(resolve(R"(
    struct Pair<A, B> {
      public first: A,
      public second: B
    }
    fun main() {
      const p = Pair::<i32, f64> { first : 1, second : 2.0 };
    }
  )"));
}

//===----------------------------------------------------------------------===//
// Complex Scoping
//===----------------------------------------------------------------------===//



TEST(NameResolver, ForLoopScope) {
  EXPECT_TRUE(resolve(R"(
    fun main() {
      for i in 0..10 {
        const x = i;
      }
    }
  )"));
}

TEST(NameResolver, WhileLoopScope) {
  EXPECT_TRUE(resolve(R"(
    fun main() {
      var x = 0;
      while x < 10 {
        x++;
      }
    }
  )"));
}

//===----------------------------------------------------------------------===//
// Arrays
//===----------------------------------------------------------------------===//

TEST(NameResolver, ArrayLiteral) {
  EXPECT_TRUE(resolve(R"(
    fun main() {
      const arr = [1, 2, 3];
    }
  )"));
}

TEST(NameResolver, ArrayTypeParam) {
  EXPECT_TRUE(resolve(R"(
    fun sum(const arr: [i32]) -> i32 {
      return 0;
    }
    fun main() {
      const arr = [1, 2, 3];
      const s = sum(arr);
    }
  )"));
}

//===----------------------------------------------------------------------===//
// Tuples
//===----------------------------------------------------------------------===//

TEST(NameResolver, TupleLiteral) {
  EXPECT_TRUE(resolve(R"(
    fun main() {
      const t = (1, 2.0, true);
    }
  )"));
}

TEST(NameResolver, TupleReturn) {
  EXPECT_TRUE(resolve(R"(
    fun pair() -> (i32, f64) {
      return (1, 2.0);
    }
    fun main() {
      const p = pair();
    }
  )"));
}
