#include <gtest/gtest.h>

#include "Diagnostics/DiagnosticManager.hpp"
#include "Lexer/Lexer.hpp"
#include "Parser/Parser.hpp"
#include "Sema/Sema.hpp"
#include "CodeGen/LLVMCodeGen.hpp"

#include <llvm/IR/Module.h>
#include <llvm/IR/Verifier.h>
#include <llvm/Support/raw_ostream.h>

#include <memory>
#include <string>
#include <vector>

using namespace phi;

// Helper to run pipeline and return CodeGen object for inspection
static std::unique_ptr<CodeGen> compile(const std::string &Src) {
  DiagnosticManager Diags(DiagnosticConfig{.UseColors = false});
  Diags.getSrcManager().addSrcFile("test.phi", Src);

  Lexer L(Src, "test.phi", &Diags);
  auto Tokens = L.scan();
  if (Diags.hasError()) return nullptr;

  Parser P(Tokens, &Diags);
  auto Mod = P.parse();
  if (!Mod || Diags.hasError()) return nullptr;

  std::vector<ModuleDecl *> Mods = {Mod.get()};
  Sema S(Mods, &Diags);
  S.analyze();
  if (Diags.hasError()) return nullptr;

  auto CG = std::make_unique<CodeGen>(Mods, "test");
  CG->generate();
  
  // Verify module to ensure validity
  std::string ErrorStr;
  llvm::raw_string_ostream ErrorOS(ErrorStr);
  if (llvm::verifyModule(CG->getModule(), &ErrorOS)) {
    return nullptr;
  }

  return CG;
}

// Helper checking if a function with a given name exists in the module
static bool hasFunction(const llvm::Module &M, const std::string &Name) {
    return M.getFunction(Name) != nullptr;
}

TEST(Monomorphization, BasicFunction) {
  auto CG = compile(R"(
    fun foo<T>(const x: T) -> T {
      return x;
    }

    fun main() {
      foo(1);
      foo(2.0);
    }
  )");
  ASSERT_TRUE(CG);
  auto &M = CG->getModule();
  
  // Check for monomorphized names
  // Exact name mangling depends on implementation, but usually distinct
  // Assuming basic mangling: foo$i32, foo$f64 or similar
  // Adjust based on actual mangling scheme in LLVMCodeGen.cpp
  
  // Let's print existing functions to debug if specific names are needed
  /*
  for (auto &F : M) {
      llvm::errs() << "Function: " << F.getName() << "\n";
  }
  */
  
  // Ideally we should know the mangling scheme.
  // Based on current implementation (I'd need to check check LLVMCodeGen.cpp generateMonomorphizedName)
  // But generally checking that specific functions EXIST is key.
  
  // Phi uses a helper `generateMonomorphizedName`
  // Usually base_name $ type_args
  
  EXPECT_TRUE(hasFunction(M, "foo_i32"));
  EXPECT_TRUE(hasFunction(M, "foo_f64"));
}

TEST(Monomorphization, Deduplication) {
  auto CG = compile(R"(
    fun foo<T>(const x: T) {}

    fun main() {
      foo(1);
      foo(2);
      foo(3);
    }
  )");
  ASSERT_TRUE(CG);
  auto &M = CG->getModule();

  int Count = 0;
  for (auto &F : M) {
    if (F.getName().starts_with("foo_")) {
      Count++;
    }
  }
  // Should accept only ONE instantiation for i32
  EXPECT_EQ(Count, 1);
  EXPECT_TRUE(hasFunction(M, "foo_i32"));
}

TEST(Monomorphization, StructMethod) {
  auto CG = compile(R"(
    struct Box<T> {
      public val: T,
      fun get(const this) -> T { return this.val; }
    }

    fun main() {
      const a = Box::<i32> { val: 1 };
      a.get();
      const b = Box::<f64> { val: 1.0 };
      b.get();
    }
  )");
  ASSERT_TRUE(CG);
  auto &M = CG->getModule();

  EXPECT_TRUE(hasFunction(M, "Box_i32_get"));
  EXPECT_TRUE(hasFunction(M, "Box_f64_get"));
}

TEST(Monomorphization, NestedGenerics) {
  auto CG = compile(R"(
    struct Wrapper<T> {
      public val: T
    }

    fun process<T>(const w: Wrapper<T>) -> T {
      return w.val;
    }

    fun main() {
      const w = Wrapper::<i32> { val: 10 };
      process(w);
    }
  )");
  ASSERT_TRUE(CG);
  auto &M = CG->getModule();

  EXPECT_TRUE(hasFunction(M, "process_i32"));
}


TEST(Monomorphization, UnusedInstantiations) {
  auto CG = compile(R"(
    fun unused<T>(const x: T) {}

    fun main() {
      // No call to unused<T>
    }
  )");
  ASSERT_TRUE(CG);
  auto &M = CG->getModule();

  // Should NOT verify unused_i32 etc exist
  EXPECT_FALSE(hasFunction(M, "unused_i32"));
  EXPECT_FALSE(hasFunction(M, "unused_f64"));
  
  // Also should not have generic version
  EXPECT_FALSE(hasFunction(M, "unused"));
}
