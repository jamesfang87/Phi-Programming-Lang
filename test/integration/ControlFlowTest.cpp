#include <gtest/gtest.h>
#include <memory>
#include <string>

#include "Diagnostics/DiagnosticManager.hpp"
#include "Driver/Driver.hpp"
#include "SrcManager/SrcManager.hpp"

using namespace phi;

class ControlFlowTest : public ::testing::Test {
protected:
  void SetUp() override {
    SrcMan = std::make_shared<SrcManager>();
    DiagMan = std::make_shared<DiagnosticManager>(SrcMan);
  }

  // Helper method to compile Phi code and check for successful compilation
  bool compilePhiCode(const std::string &Code) {
    try {
      std::string FilePath = "test.phi";
      SrcMan->addSrcFile(FilePath, Code);

      PhiCompiler Compiler(Code, FilePath, DiagMan);
      Compiler.compile();
      return true;
    } catch (const std::exception &E) {
      LastError = E.what();
      return false;
    }
  }

  // Helper method to get compilation diagnostics
  std::string getLastError() const { return LastError; }

  std::shared_ptr<SrcManager> SrcMan;
  std::shared_ptr<DiagnosticManager> DiagMan;
  std::string LastError;
};

// Test suite for break statement functionality
class BreakStatementTest : public ControlFlowTest {};

TEST_F(BreakStatementTest, BasicWhileBreak) {
  std::string Code = R"(
        fun test_while_break() {
            var i = 0;
            while i < 10 {
                if i == 5 {
                    break;
                }
                i = i + 1;
            }
        }

        fun main() {
            test_while_break();
        }
    )";

  EXPECT_TRUE(compilePhiCode(Code))
      << "Failed to compile while loop with break: " << getLastError();
}

TEST_F(BreakStatementTest, BasicForBreak) {
  std::string Code = R"(
        fun test_for_break() {
            for i in 0..10 {
                if i == 5 {
                    break;
                }
            }
        }

        fun main() {
            test_for_break();
        }
    )";

  EXPECT_TRUE(compilePhiCode(Code))
      << "Failed to compile for loop with break: " << getLastError();
}

TEST_F(BreakStatementTest, NestedLoopBreak) {
  std::string Code = R"(
        fun test_nested_break() {
            for i in 0..5 {
                for j in 0..5 {
                    if j == 2 {
                        break; // Should only break inner loop
                    }
                }
            }
        }

        fun main() {
            test_nested_break();
        }
    )";

  EXPECT_TRUE(compilePhiCode(Code))
      << "Failed to compile nested loops with break: " << getLastError();
}

// Test suite for continue statement functionality
class ContinueStatementTest : public ControlFlowTest {};

TEST_F(ContinueStatementTest, BasicWhileContinue) {
  std::string Code = R"(
        fun test_while_continue() {
            var i = 0;
            while i < 5 {
                i = i + 1;
                if i == 3 {
                    continue;
                }
            }
        }

        fun main() {
            test_while_continue();
        }
    )";

  EXPECT_TRUE(compilePhiCode(Code))
      << "Failed to compile while loop with continue: " << getLastError();
}

TEST_F(ContinueStatementTest, BasicForContinue) {
  std::string Code = R"(
        fun test_for_continue() {
            for i in 0..5 {
                if i == 2 {
                    continue;
                }
            }
        }

        fun main() {
            test_for_continue();
        }
    )";

  EXPECT_TRUE(compilePhiCode(Code))
      << "Failed to compile for loop with continue: " << getLastError();
}

TEST_F(ContinueStatementTest, NestedLoopContinue) {
  std::string Code = R"(
        fun test_nested_continue() {
            for i in 0..3 {
                for j in 0..3 {
                    if j == 1 {
                        continue; // Should only continue inner loop
                    }
                }
            }
        }

        fun main() {
            test_nested_continue();
        }
    )";

  EXPECT_TRUE(compilePhiCode(Code))
      << "Failed to compile nested loops with continue: " << getLastError();
}

// Test suite for defer statement functionality
class DeferStatementTest : public ControlFlowTest {};

TEST_F(DeferStatementTest, BasicDefer) {
  std::string Code = R"(
        fun println(const msg: string) {}

        fun test_basic_defer() {
            defer println("This should execute at function exit");
            const x = 5;
        }

        fun main() {
            test_basic_defer();
        }
    )";

  EXPECT_TRUE(compilePhiCode(Code))
      << "Failed to compile basic defer: " << getLastError();
}

TEST_F(DeferStatementTest, MultipleDefers) {
  std::string Code = R"(
        fun println(const msg: string) {}

        fun test_multiple_defers() {
            defer println("First defer");
            defer println("Second defer");
            defer println("Third defer");
            // Should execute in reverse order: Third, Second, First
        }

        fun main() {
            test_multiple_defers();
        }
    )";

  EXPECT_TRUE(compilePhiCode(Code))
      << "Failed to compile multiple defers: " << getLastError();
}

TEST_F(DeferStatementTest, DeferWithEarlyReturn) {
  std::string Code = R"(
        fun println(const msg: string) {}

        fun test_defer_early_return() -> i64 {
            defer println("Defer should execute before return");

            if true {
                return 42;
            }

            return 0;
        }

        fun main() {
            const result = test_defer_early_return();
        }
    )";

  EXPECT_TRUE(compilePhiCode(Code))
      << "Failed to compile defer with early return: " << getLastError();
}

// Test suite for return statement functionality
class ReturnStatementTest : public ControlFlowTest {};

TEST_F(ReturnStatementTest, BasicReturn) {
  std::string Code = R"(
        fun test_return() -> i64 {
            return 42;
        }

        fun main() {
            const result = test_return();
        }
    )";

  EXPECT_TRUE(compilePhiCode(Code))
      << "Failed to compile basic return: " << getLastError();
}

TEST_F(ReturnStatementTest, VoidReturn) {
  std::string Code = R"(
        fun test_void_return() {
            const x = 5;
            if x > 0 {
                return;
            }
            const y = 10;
        }

        fun main() {
            test_void_return();
        }
    )";

  EXPECT_TRUE(compilePhiCode(Code))
      << "Failed to compile void return: " << getLastError();
}

TEST_F(ReturnStatementTest, ReturnWithDefer) {
  std::string Code = R"(
        fun println(const msg: string) {}

        fun test_return_with_defer() -> i64 {
            defer println("Defer executes before return");
            return 100;
        }

        fun main() {
            const result = test_return_with_defer();
        }
    )";

  EXPECT_TRUE(compilePhiCode(Code))
      << "Failed to compile return with defer: " << getLastError();
}

// Test suite for complex control flow scenarios
class ComplexControlFlowTest : public ControlFlowTest {};

TEST_F(ComplexControlFlowTest, BreakContinueInSameLoop) {
  std::string Code = R"(
        fun test_break_continue_mixed() {
            for i in 0..10 {
                if i == 3 {
                    continue;
                }
                if i == 7 {
                    break;
                }
            }
        }

        fun main() {
            test_break_continue_mixed();
        }
    )";

  EXPECT_TRUE(compilePhiCode(Code))
      << "Failed to compile mixed break/continue: " << getLastError();
}

TEST_F(ComplexControlFlowTest, DeferInLoop) {
  std::string Code = R"(
        fun println(const msg: string) {}

        fun test_defer_in_loop() {
            for i in 0..3 {
                if i == 1 {
                    continue;
                }
                if i == 2 {
                    break;
                }
            }
            defer println("Function level defer");
        }

        fun main() {
            test_defer_in_loop();
        }
    )";

  EXPECT_TRUE(compilePhiCode(Code))
      << "Failed to compile defer in loop: " << getLastError();
}

TEST_F(ComplexControlFlowTest, AllControlFlowFeatures) {
  std::string Code = R"(
        fun println(const msg: string) {}

        fun test_all_features() -> i64 {
            defer println("Function exit defer");

            var result = 0;
            for i in 0..5 {
                if i == 1 {
                    continue;
                }
                if i == 4 {
                    break;
                }

                var j = 0;
                while j < 3 {
                    if j == 2 {
                        break;
                    }
                    j = j + 1;
                    result = result + 1;
                }
            }

            if result > 5 {
                defer println("Conditional defer");
                return result;
            }

            return result * 2;
        }

        fun main() {
            const result = test_all_features();
        }
    )";

  EXPECT_TRUE(compilePhiCode(Code))
      << "Failed to compile complex control flow: " << getLastError();
}

// Error case tests
class ControlFlowErrorTest : public ControlFlowTest {};

TEST_F(ControlFlowErrorTest, BreakOutsideLoop) {
  std::string Code = R"(
        fun test_invalid_break() {
            break; // Should fail - break outside loop
        }

        fun main() {
            test_invalid_break();
        }
    )";

  // This should fail to compile because break is outside a loop
  EXPECT_FALSE(compilePhiCode(Code))
      << "Break outside loop should fail to compile";
}

TEST_F(ControlFlowErrorTest, ContinueOutsideLoop) {
  std::string Code = R"(
        fun test_invalid_continue() {
            continue; // Should fail - continue outside loop
        }

        fun main() {
            test_invalid_continue();
        }
    )";

  // This should fail to compile because continue is outside a loop
  EXPECT_FALSE(compilePhiCode(Code))
      << "Continue outside loop should fail to compile";
}

// Integration tests with existing language features
class ControlFlowIntegrationTest : public ControlFlowTest {};

TEST_F(ControlFlowIntegrationTest, ControlFlowWithVariables) {
  std::string Code = R"(
        fun test_with_variables() -> i64 {
            var sum = 0;
            var count = 0;

            for i in 0..10 {
                if i == 0 {
                    continue;
                }

                sum = sum + i;
                count = count + 1;

                if sum > 20 {
                    break;
                }
            }

            return sum;
        }

        fun main() {
            const result = test_with_variables();
        }
    )";

  EXPECT_TRUE(compilePhiCode(Code))
      << "Failed to compile control flow with variables: " << getLastError();
}

TEST_F(ControlFlowIntegrationTest, ControlFlowWithFunctionCalls) {
  std::string Code = R"(
        fun println(const msg: string) {}

        fun helper_function() -> i64 {
            defer println("Helper function defer");
            return 5;
        }

        fun test_with_calls() -> i64 {
            defer println("Main function defer");

            var result = 0;
            for i in 0..3 {
                const val = helper_function();
                result = result + val;

                if result > 10 {
                    break;
                }
            }

            return result;
        }

        fun main() {
            const result = test_with_calls();
        }
    )";

  EXPECT_TRUE(compilePhiCode(Code))
      << "Failed to compile control flow with function calls: "
      << getLastError();
}
