# Phi Unit Tests

This directory contains Google Test (GTest) unit tests for the Phi compiler, focusing on control flow statement functionality.

## Overview

The unit tests cover the following features:
- **Break statements** in while and for loops
- **Continue statements** in while and for loops  
- **Defer statements** with proper LIFO execution order
- **Return statements** with and without values
- **Complex control flow** scenarios combining multiple features
- **Error cases** for invalid usage
- **Integration tests** with other language features

## Test Structure

- `ControlFlowTest.cpp` - Main test suite with comprehensive control flow tests
- `main.cpp` - GTest main entry point

## Building and Running Tests

### Prerequisites

Install Google Test:
```bash
# macOS with Homebrew
brew install googletest

# Ubuntu/Debian
sudo apt-get install libgtest-dev

# Or build from source
git clone https://github.com/google/googletest.git
cd googletest
cmake -S . -B build
cmake --build build
sudo cmake --install build
```

### Build with Tests

From the project root directory:

```bash
# Configure with tests enabled
cmake -S . -B build -DBUILD_TESTS=ON

# Build everything including tests
cmake --build build

# Run the tests
cd build/test/unit
./phi_tests

# Or use CTest
cd build
ctest --output-on-failure
```

### Alternative: Run tests directly

```bash
# From project root
make && cd build/test/unit && ./phi_tests
```

## Test Categories

### Break Statement Tests
- Basic break in while loops
- Basic break in for loops
- Break in nested loops (should only break inner loop)

### Continue Statement Tests  
- Basic continue in while loops
- Basic continue in for loops
- Continue in nested loops (should only continue inner loop)

### Defer Statement Tests
- Basic defer execution at function exit
- Multiple defers (should execute in LIFO order)
- Defer with early return statements

### Return Statement Tests
- Basic return with values
- Void returns
- Return combined with defer statements

### Complex Scenarios
- Mixed break/continue in same loop
- Defer statements in loops
- All control flow features combined
- Integration with variables and function calls

### Error Cases
- Break outside of loop context (should fail compilation)
- Continue outside of loop context (should fail compilation)

## Adding New Tests

To add new tests:

1. Create a new test class inheriting from `ControlFlowTest`
2. Use the `compilePhiCode()` helper method to test Phi code compilation
3. Use GTest assertions (`EXPECT_TRUE`, `EXPECT_FALSE`, etc.)
4. Follow the existing naming patterns for test cases

Example:
```cpp
TEST_F(ControlFlowTest, MyNewTest) {
    std::string Code = R"(
        fun test_function() {
            // Your Phi code here
        }
        
        fun main() {
            test_function();
        }
    )";
    
    EXPECT_TRUE(compilePhiCode(Code)) 
        << "Failed to compile: " << getLastError();
}
```

## Test Philosophy

These tests focus on:
- **Compilation correctness** - Does the code compile without errors?
- **Control flow semantics** - Do control flow statements work as expected?
- **Error handling** - Are invalid usages properly rejected?
- **Integration** - Do control flow features work well with other language features?

The tests primarily verify that the compiler correctly processes control flow constructs rather than testing runtime execution (which would require an execution engine).