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

class OperatorCodeGenTest : public ::testing::Test {
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

TEST_F(OperatorCodeGenTest, ArithmeticBasic) {
  std::string Code = R"(
    fun println(const msg: i32) {}

    fun main() {
      println(1 + 2 * 3);
    }
  )";
  compileAndExpectOutput(Code, "7\n");
}

TEST_F(OperatorCodeGenTest, ArithmeticParentheses) {
  std::string Code = R"(
    fun println(const msg: i32) {}

    fun main() {
      println((1 + 2) * 3);
    }
  )";
  compileAndExpectOutput(Code, "9\n");
}

TEST_F(OperatorCodeGenTest, UnaryPlusMinus) {
  std::string Code = R"(
    fun println(const msg: i32) {}

    fun main() {
      println(-5 + 3);
    }
  )";
  compileAndExpectOutput(Code, "-2\n");
}

TEST_F(OperatorCodeGenTest, CompoundAssignmentLike) {
  std::string Code = R"(
    fun println(const msg: i32) {}

    fun main() {
      var a = 10;
      a = a + 5;
      println(a);
    }
  )";
  compileAndExpectOutput(Code, "15\n");
}

TEST_F(OperatorCodeGenTest, Comparisons) {
  std::string Code = R"(
    fun println(const msg: i32) {}

    fun main() {
      if (2 < 3) {
        println(1);
      } else {
        println(0);
      }
    }
  )";
  compileAndExpectOutput(Code, "1\n");
}

TEST_F(OperatorCodeGenTest, EqualityInequality) {
  std::string Code = R"(
    fun println(const msg: i32) {}

    fun main() {
      if (5 == 5 && !(5 != 5)) {
        println(1);
      } else {
        println(0);
      }
    }
  )";
  compileAndExpectOutput(Code, "1\n");
}

TEST_F(OperatorCodeGenTest, LogicalOps) {
  std::string Code = R"(
    fun println(const msg: i32) {}

    fun main() {
      if (true || false) {
        if (true && false) {
          println(0);
        } else {
          println(1);
        }
      }
    }
  )";
  compileAndExpectOutput(Code, "1\n");
}

TEST_F(OperatorCodeGenTest, IntegerDivision) {
  std::string Code = R"(
    fun println(const msg: i32) {}

    fun main() {
      println(7 / 2);
    }
  )";
  compileAndExpectOutput(Code, "3\n");
}

TEST_F(OperatorCodeGenTest, ComplexExpression) {
  std::string Code = R"(
    fun println(const msg: i32) {}

    fun main() {
      println(((4 + 3) * 2) - (4) + (9 / 3));
    }
  )";
  compileAndExpectOutput(Code, "13\n");
}
