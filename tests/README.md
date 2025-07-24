# Phi Compiler Test Suite

This directory contains comprehensive tests for the Phi compiler, focusing on parsing and error recovery capabilities.

## Directory Structure

```
tests/
├── correct/          # Tests that should parse successfully
├── incorrect/        # Tests that should produce errors but continue parsing
└── README.md        # This file
```

## Test Categories

### Correct Tests (`tests/correct/`)

These files contain valid Phi code that should parse without any errors:

- **`basic_function.phi`** - Simple function definitions with parameters and return types
- **`control_flow.phi`** - If statements, while loops, for loops, and nested control structures
- **`variables.phi`** - Variable declarations, assignments, and operations
- **`expressions.phi`** - Arithmetic, comparison, boolean, and nested expressions
- **`complete_program.phi`** - A comprehensive program using all language features

### Incorrect Tests (`tests/incorrect/`)

These files contain various syntax errors to test error recovery:

- **`missing_semicolons.phi`** - Missing semicolons in various contexts
- **`missing_braces.phi`** - Missing opening/closing braces in functions and control structures
- **`invalid_variables.php`** - Invalid variable declarations (missing colons, types, etc.)
- **`invalid_control_flow.phi`** - Malformed if/while/for statements
- **`invalid_functions.phi`** - Function definition errors (missing names, parameters, etc.)
- **`mixed_errors.phi`** - Multiple error types combined to test comprehensive recovery
- **`edge_cases.phi`** - Unusual scenarios and edge cases
- **`complete_broken_program.phi`** - A large program with many different types of errors

## Running Tests

### Quick Start

```bash
# Run all tests
./run_tests.sh

# Run only tests that should succeed
./run_tests.sh correct

# Run only error recovery tests
./run_tests.sh incorrect

# Show detailed output for a specific test
./run_tests.sh show tests/correct/basic_function.phi
```

### Test Runner Options

The `run_tests.sh` script provides several modes:

- **`all`** (default) - Run all tests and report summary
- **`correct`** - Run only the valid syntax tests
- **`incorrect`** - Run only the error recovery tests
- **`show <file>`** - Display detailed compiler output for a specific test file
- **`help`** - Show usage information

### Expected Results

- **Correct tests**: Should compile without errors (exit code 0)
- **Incorrect tests**: Should detect errors but continue parsing (exit code != 0)

## Understanding Test Results

### Success Cases

When a test passes, you'll see:
```
Testing basic_function... PASS
Testing control_flow... PASS
```

### Error Recovery Cases

For error recovery tests, "PASS" means the compiler correctly detected errors:
```
Testing missing_semicolons... PASS (correctly detected errors)
Testing invalid_variables... PASS (correctly detected errors)
```

### Failure Cases

If a test fails, you'll see detailed output:
```
Testing some_test... FAIL (expected success but got errors)
Output:
  Error: missing semicolon after return statement
  ...
```

## Test Design Principles

### Correct Tests
- **Completeness**: Cover all language features
- **Clarity**: Easy for humans to verify correctness
- **Variety**: Different combinations of features
- **Size**: Neither too simple nor too complex

### Incorrect Tests
- **Error Types**: Cover all major syntax error categories
- **Recovery**: Test that parsing continues after errors
- **Realism**: Represent common programming mistakes
- **Multiple Errors**: Test handling of several errors in one file

## Error Recovery Verification

The error recovery tests verify that:

1. **Errors are detected** and reported with helpful messages
2. **Parsing continues** after encountering errors
3. **Multiple errors** in the same file are found
4. **No invalid AST nodes** are created (compilation still fails)
5. **Synchronization works** - parser recovers to continue parsing

## Adding New Tests

### For Correct Tests
1. Create a `.phi` file in `tests/correct/`
2. Ensure it uses valid Phi syntax
3. Keep it focused but comprehensive
4. Add comments explaining what's being tested

### For Incorrect Tests
1. Create a `.phi` file in `tests/incorrect/`
2. Include intentional syntax errors
3. Add comments explaining what errors to expect
4. Test that error recovery allows continued parsing

### Test File Guidelines
- **Short but not trivial**: ~20-100 lines is ideal
- **Well-commented**: Explain what should happen
- **Single focus**: Each file should test related features
- **Human-verifiable**: Easy to check by reading the code

## Example Test File

```phi
// Example correct test - basic arithmetic
fun calculate_sum(a: int, b: int) -> int {
    let result: int = a + b;
    return result;
}

fun test_calculations() -> int {
    let x: int = 10;
    let y: int = 20;
    let sum: int = calculate_sum(x, y);
    return sum;
}
```

## Interpreting Error Messages

When running incorrect tests, look for:

1. **Error detection**: Specific error messages for syntax problems
2. **Recovery evidence**: Parsing continues to find later errors
3. **Helpful suggestions**: Error messages include fix suggestions
4. **Source location**: Errors point to the right location in code

## Continuous Integration

These tests are designed to be run in CI/CD pipelines:
- Exit code 0 = all tests passed
- Exit code 1 = some tests failed
- Detailed output for debugging failures

## Troubleshooting

### Test Runner Issues
- Ensure you're in the Phi project root directory
- Make sure `build/phi` exists (run `make` first)
- Check that test files exist in the expected directories

### Unexpected Results
- Use `./run_tests.sh show <file>` to see detailed compiler output
- Check that the compiler binary is up to date
- Verify test file syntax matches your expectations

## Contributing

When adding new language features:
1. Add positive tests in `tests/correct/`
2. Add negative tests in `tests/incorrect/`
3. Update this README if needed
4. Ensure all tests pass before submitting changes