# Phi Compiler Testing Guide

## Overview

This guide covers the comprehensive testing framework for the Phi programming language compiler. The testing infrastructure is designed to ensure code quality, prevent regressions, and support continuous development while following LLVM coding conventions.

## Testing Architecture

### Test Structure

The Phi compiler uses a multi-layered testing approach:

```
Phi/tests/
├── unit/                    # Unit tests (C++ with Google Test)
│   ├── LexerTests.cpp      # Lexical analysis tests
│   ├── DiagnosticTests.cpp # Error reporting tests
│   ├── ASTTests.cpp        # Abstract syntax tree tests
│   ├── IntegrationTests.cpp # Component integration tests
│   ├── BasicTest.cpp       # Framework validation
│   ├── CMakeLists.txt      # Build configuration
│   ├── Makefile            # Convenient test runner
│   └── README.md           # Detailed unit test docs
├── correct/                # Valid Phi programs
└── incorrect/              # Invalid Phi programs (error cases)
```

### Test Categories

1. **Unit Tests** - Individual component testing using Google Test framework
2. **Integration Tests** - Multi-component interaction verification
3. **Acceptance Tests** - End-to-end program compilation testing
4. **Regression Tests** - Previously fixed bugs verification

## Quick Start

### Running All Tests

From the project root:

```bash
# Run all unit tests
make test

# Run with verbose output
make test-verbose

# Generate coverage report
make test-coverage

# Check for memory leaks
make test-memcheck
```

### Running Specific Tests

```bash
# Run only lexer tests
cd tests/unit && make test-suite SUITE=Lexer

# Run specific test case
cd tests/unit && ./build/phi_unit_tests --gtest_filter="LexerTest.BasicTokens"

# List all available tests
cd tests/unit && make list-tests
```

## Writing Unit Tests

### Test File Structure

Follow this pattern for new test files:

```cpp
/**
 * @file ComponentTests.cpp
 * @brief Unit tests for Component functionality
 */

#include <gtest/gtest.h>
#include <memory>
#include <string>
#include <vector>

#include "Component/Component.hpp"
#include "SrcManager/SrcManager.hpp"
#include "Diagnostics/DiagnosticManager.hpp"

using namespace phi;

class ComponentTest : public ::testing::Test {
protected:
  void SetUp() override {
    SrcManager = std::make_shared<SrcManager>();
    DiagManager = std::make_shared<DiagnosticManager>(SrcManager);
    // Initialize other test fixtures
  }

  // Test data members using LLVM naming conventions
  std::shared_ptr<SrcManager> SrcManager;
  std::shared_ptr<DiagnosticManager> DiagManager;
};

TEST_F(ComponentTest, BasicFunctionality) {
  // Arrange
  auto Input = CreateTestInput();
  
  // Act
  auto Result = ProcessInput(Input);
  
  // Assert
  EXPECT_TRUE(Result.isValid());
  EXPECT_EQ(Result.getValue(), ExpectedValue);
}
```

### Naming Conventions

Following LLVM coding standards:

- **Classes**: `ComponentTest`, `LexerTest`
- **Test Methods**: `TEST_F(ComponentTest, MethodDescription)`
- **Variables**: `CapitalCase` (e.g., `TokenList`, `ErrorCount`)
- **Local Variables**: `CapitalCase` (e.g., `Tokens`, `Diagnostics`)

### Test Patterns

#### Testing Success Cases

```cpp
TEST_F(LexerTest, TokenizeBasicExpression) {
  auto Tokens = LexString("1 + 2");
  
  ASSERT_EQ(Tokens.size(), 4); // int, +, int, EOF
  EXPECT_EQ(Tokens[0].getKind(), TokenKind::IntLiteral);
  EXPECT_EQ(Tokens[1].getKind(), TokenKind::Plus);
  EXPECT_EQ(Tokens[2].getKind(), TokenKind::IntLiteral);
}
```

#### Testing Error Cases

```cpp
TEST_F(LexerTest, HandleInvalidSyntax) {
  auto Tokens = LexString("invalid @@ syntax");
  
  // Should produce error tokens but not crash
  EXPECT_GT(Tokens.size(), 0);
  EXPECT_EQ(Tokens.back().getKind(), TokenKind::Eof);
  
  // Check that error was reported
  EXPECT_GT(DiagManager->error_count(), 0);
}
```

#### Testing AST Nodes

```cpp
TEST_F(ASTTest, BinaryExpressionCreation) {
  auto Left = std::make_unique<IntLiteral>(1, TestLoc);
  auto Right = std::make_unique<IntLiteral>(2, TestLoc);
  auto Expr = std::make_unique<BinaryOp>(
      std::move(Left), TokenKind::Plus, std::move(Right), TestLoc);

  EXPECT_EQ(Expr->getKind(), Expr::Kind::BinaryOpKind);
  EXPECT_EQ(Expr->getOp(), TokenKind::Plus);
  EXPECT_TRUE(BinaryOp::classof(Expr.get()));
}
```

## Integration Testing

### Full Pipeline Tests

Integration tests verify that multiple compiler components work together:

```cpp
TEST_F(IntegrationTest, CompileSimpleProgram) {
  std::string Source = R"(
    fun main() {
      var x = 42;
      return x;
    }
  )";
  
  auto Declarations = ParseProgram(Source);
  
  EXPECT_EQ(CountErrors(), 0);
  ASSERT_GE(Declarations.size(), 1);
  
  auto MainFunc = dynamic_cast<FunDecl*>(Declarations[0].get());
  ASSERT_NE(MainFunc, nullptr);
  EXPECT_EQ(MainFunc->getName(), "main");
}
```

## Test Development Guidelines

### Best Practices

1. **Test Independence**: Each test should run in isolation
2. **Clear Names**: Test names should describe what is being tested
3. **Focused Testing**: One concept per test method
4. **Both Paths**: Test both success and failure cases
5. **Edge Cases**: Include boundary condition testing

### Assertion Guidelines

Use appropriate Google Test macros:

- `EXPECT_EQ/ASSERT_EQ` - Equality comparison
- `EXPECT_TRUE/ASSERT_TRUE` - Boolean conditions  
- `EXPECT_GT/ASSERT_GT` - Greater than comparisons
- `EXPECT_NE/ASSERT_NE` - Inequality comparison
- `ASSERT_NE` - Null pointer checks before dereferencing

### Error Testing Strategy

```cpp
// Test that errors are properly reported
TEST_F(ComponentTest, ReportsSpecificError) {
  auto Result = ProcessInvalidInput();
  
  EXPECT_FALSE(Result.isValid());
  EXPECT_GT(DiagManager->error_count(), 0);
  
  auto Errors = DiagManager->getErrors();
  EXPECT_NE(Errors[0].getMessage().find("expected"), std::string::npos);
}
```

## Build System Integration

### CMake Configuration

The unit test build system automatically:

- Fetches Google Test via FetchContent
- Links against LLVM libraries
- Configures C++23 compilation
- Sets up include paths
- Enables test discovery

### Makefile Targets

The provided Makefile includes these convenient targets:

```bash
make test           # Run all tests
make test-verbose   # Detailed output
make test-coverage  # Generate coverage report
make test-memcheck  # Valgrind memory checking
make test-watch     # Continuous testing
make clean          # Clean build artifacts
```

## Coverage Analysis

### Generating Reports

```bash
cd tests/unit
make test-coverage
```

This creates an HTML report in `build/coverage_html/index.html`.

### Coverage Targets

- **Line Coverage**: >90%
- **Branch Coverage**: >85%
- **Function Coverage**: >95%

### Improving Coverage

1. Identify uncovered code in the report
2. Add tests for missing paths
3. Focus on error handling branches
4. Test edge cases and boundary conditions

## Memory Safety Testing

### Valgrind Integration

```bash
cd tests/unit
make test-memcheck
```

This runs tests under Valgrind to detect:
- Memory leaks
- Use of uninitialized memory
- Buffer overruns
- Double-free errors

### AddressSanitizer

For faster memory error detection:

```bash
export CXX=clang++
export CXXFLAGS="-fsanitize=address"
cd tests/unit && make test
```

## Continuous Integration

### GitHub Actions Example

```yaml
name: Unit Tests
on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
    - uses: actions/checkout@v2
    
    - name: Install Dependencies
      run: |
        sudo apt-get update
        sudo apt-get install -y cmake ninja-build
        
    - name: Run Unit Tests
      run: |
        cd tests/unit
        make test
        
    - name: Generate Coverage
      run: |
        cd tests/unit  
        make test-coverage
```

## Debugging Test Failures

### Running Individual Tests

```bash
# Run specific test with debugger
cd tests/unit
gdb ./build/phi_unit_tests
(gdb) run --gtest_filter="LexerTest.SpecificTest"
```

### Verbose Diagnostics

```bash
# Get detailed failure information
cd tests/unit
make test-verbose
```

### Common Issues

1. **Compilation Errors**
   - Check C++23 compiler support
   - Verify LLVM installation
   - Review include paths

2. **Runtime Failures**
   - Check test isolation
   - Verify mock object setup
   - Review assertion logic

3. **Memory Issues**
   - Run with Valgrind
   - Check smart pointer usage
   - Verify resource cleanup

## Adding New Tests

### For New Components

1. Create `ComponentTests.cpp` in `tests/unit/`
2. Follow the established patterns
3. Include both positive and negative tests
4. Add integration tests if the component interacts with others
5. Update documentation

### For Bug Fixes

1. Write failing test that reproduces the bug
2. Fix the bug
3. Verify test now passes
4. Commit both test and fix together

### For New Features

1. Write tests for the new functionality
2. Implement the feature to make tests pass
3. Add integration tests for feature interactions
4. Update existing tests if APIs changed

## Performance Considerations

### Test Execution Speed

- Keep individual tests under 100ms
- Use appropriate test fixtures for setup/teardown
- Avoid unnecessary file I/O in tests
- Mock expensive operations when possible

### Resource Usage

- Monitor memory consumption during tests
- Clean up resources in test teardown
- Use smart pointers for automatic cleanup
- Profile test execution for bottlenecks

## Quality Metrics

### Test Health Indicators

- **Pass Rate**: >99% (minimal flaky tests)
- **Execution Time**: Full suite under 30 seconds
- **Coverage**: Meet target percentages
- **Memory Usage**: No leaks detected

### Code Quality

- All tests follow LLVM conventions
- Clear, descriptive test names
- Appropriate assertion granularity
- Comprehensive error case coverage

This testing framework ensures the Phi compiler maintains high quality standards while supporting rapid development and refactoring. Regular test execution and maintenance keeps the codebase reliable and maintainable.