#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <memory>
#include <string>

#include <gtest/gtest.h>

#include "Diagnostics/DiagnosticManager.hpp"
#include "Driver/Driver.hpp"
#include "SrcManager/SrcManager.hpp"

using namespace phi;

class StructCodeGenTest : public ::testing::Test {
protected:
  void SetUp() override {
    SrcMan = std::make_shared<SrcManager>();
    DiagMan = std::make_shared<DiagnosticManager>(SrcMan);
    TempDir = std::filesystem::temp_directory_path();
  }

  std::string findPhiExecutable() {
    std::vector<std::string> Candidates = {
        std::string(std::getenv("HOME") ? std::getenv("HOME") : "") +
            "/Phi/build/phi",
        "build/bin/phi", "build/phi", "bin/phi", "./phi"};

    for (const auto &Path : Candidates) {
      if (!Path.empty() && std::filesystem::exists(Path) &&
          std::filesystem::is_regular_file(Path)) {
        return Path;
      }
    }

    throw std::runtime_error(
        "Could not find Phi executable (tried ~/Phi/build/phi, build/bin/phi, "
        "build/phi, bin/phi, ./phi).");
  }

  bool compilePhiCode(const std::string &Code, const std::string &FilePath) {
    try {
      SrcMan->addSrcFile(FilePath, Code);
      PhiCompiler Compiler(Code, FilePath, DiagMan);
      Compiler.compile();
      return true;
    } catch (const std::exception &E) {
      LastError = E.what();
      return false;
    }
  }

  void compileAndExpectOutput(const std::string &Code,
                              const std::string &ExpectedOutput) {
    std::string ExePath =
        (TempDir /
         std::filesystem::path("op_test_" + std::to_string(TestCounter++)))
            .string();

    // Write Phi source to a temporary file
    std::string PhiFile = ExePath + ".phi";
    {
      std::ofstream Ofs(PhiFile);
      ASSERT_TRUE(Ofs.good()) << "Failed to open file: " << PhiFile;
      Ofs << Code;
    }

    // Compile Phi source into a real executable
    try {
      PhiCompiler Compiler(Code, PhiFile, DiagMan);
      Compiler.compile();
    } catch (const std::exception &E) {
      FAIL() << "Compilation failed: " << E.what();
    }

    // Run the executable and capture stdout
    std::string RunOutput;
#if defined(_WIN32)
    std::string Cmd = "\"" + ExePath + ".exe\"";
#else
    std::string Cmd = "~/Phi/a.out";
#endif

    FILE *Pipe = popen(Cmd.c_str(), "r");
    ASSERT_TRUE(Pipe != nullptr) << "Failed to run executable: " << Cmd;
    char Buffer[128];
    while (fgets(Buffer, sizeof(Buffer), Pipe) != nullptr) {
      RunOutput += Buffer;
    }
    int RetCode = pclose(Pipe);
    ASSERT_EQ(RetCode, 0) << "Program exited with error code: " << RetCode;

    // Compare stdout
    ASSERT_EQ(RunOutput, ExpectedOutput);
  }

  std::shared_ptr<SrcManager> SrcMan;
  std::shared_ptr<DiagnosticManager> DiagMan;
  std::string LastError;
  std::filesystem::path TempDir;
  int TestCounter = 0;
};

// ---------------- Operator tests ----------------

// ---------------- Struct / Method codegen tests ----------------

TEST_F(StructCodeGenTest, StructFieldAccessSimple) {
  std::string Code = R"(
    fun println(const msg: f64) {}

    struct S {
      public a: f64;
      public b: f64;

      fun get_a(const this) -> f64 { return this.a; }
      fun get_b(const this) -> f64 { return this.b; }
    }

    fun main() {
      const s = S { a = 2.0, b = 3.0 };
      println(s.a);          // direct field access -> 2.0
      println(s.b);          // direct field access -> 3.0
      println(s.get_a());    // method reading a -> 2.0
      println(s.get_b());    // method reading b -> 3.0
    }
  )";
  // each println prints a line with %g format
  compileAndExpectOutput(Code, "2\n3\n2\n3\n");
}

TEST_F(StructCodeGenTest, MethodCall_WithByValueParameter) {
  std::string Code = R"(
    fun println(const msg: f64) {}

    struct V {
      public x: f64;
      public y: f64;

      fun get_x(const this) -> f64 { return this.x; }
      fun get_y(const this) -> f64 { return this.y; }

      // takes other by-value and returns other's y
      fun other_y(const this, const other: V) -> f64 {
        return other.y;
      }
    }

    fun main() {
      const a = V { x = 1.0, y = 1.0 };
      const b = V { x = 5.0, y = 1000.0 };
      // other is passed by-value; we expect to receive the correct y (1000)
      println(a.other_y(b));
    }
  )";
  compileAndExpectOutput(Code, "1000\n");
}

TEST_F(StructCodeGenTest, DotProduct_MethodsAndFieldAccess) {
  std::string Code = R"(
    fun println(const msg: f64) {}

    struct Vector2D {
      public x: f64;
      public y: f64;

      fun get_x(const this) -> f64 { return this.x; }
      fun get_y(const this) -> f64 { return this.y; }

      // this: pointer, other: by-value
      fun dot(const this, const other: Vector2D) -> f64 {
        return this.x * other.get_x() + this.get_y() * other.y;
      }
    }

    fun main() {
      const a = Vector2D { x = 1.0, y = 1.0 };
      const b = Vector2D { x = 5.0, y = 1000.0 };
      // expected: 1*5 + 1*1000 = 1005
      println(a.dot(b));
    }
  )";
  compileAndExpectOutput(Code, "1005\n");
}

TEST_F(StructCodeGenTest, FieldCopyingAndIndependence) {
  std::string Code = R"(
    fun println(const msg: f64) {}

    struct P { public x: f64; public y: f64; }

    fun main() {
      var a = P { x = 10.0, y = 20.0 };
      var b = a;        // copy struct
      b.y = 99.0;       // should only change b
      println(a.y);     // expect 20.0
      println(b.y);     // expect 99.0
    }
  )";
  compileAndExpectOutput(Code, "20\n99\n");
}

TEST_F(StructCodeGenTest, NestedStructsAndReturnedByValue) {
  std::string Code = R"(
    fun println(const msg: f64) {}

    struct Inner { public a: f64; public b: f64; }
    struct Outer {
      public i: Inner;
      fun inner_sum(const this) -> f64 {
        return this.i.a + this.i.b;
      }
      // return inner by value
      fun take_inner(const this) -> Inner { return this.i; }
    }

    fun main() {
      const o: Outer = Outer { i = Inner { a = 2.0, b = 3.0 } };
      println(o.inner_sum());             // 5
    }
  )";
  compileAndExpectOutput(Code, "5\n2\n3\n");
}

TEST_F(StructCodeGenTest, MethodReturnsStructAndCopySemantics) {
  std::string Code = R"(
    fun println(const msg: f64) {}

    struct Point { public x: f64; public y: f64; }

    struct Builder {
      public base: Point;

      // returns a new point by value
      fun make_point(const this, const dx: f64, const dy: f64) -> Point {
        return Point { x = this.base.x + dx, y = this.base.y + dy };
      }
    }

    fun main() {
      const b = Builder { base = Point { x = 1.0, y = 2.0 } };
      const p = b.make_point(5.0, 6.0);
      // verify returned copy values
      println(p.x);
      println(p.y);

      // ensure original builder.base unchanged
      println(b.base.x);
      println(b.base.y);
    }
  )";
  compileAndExpectOutput(Code, "6\n8\n1\n2\n");
}
