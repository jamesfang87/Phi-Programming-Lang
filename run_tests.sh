#!/bin/bash

# Test runner script for Phi compiler
# This script runs the compiler on various test files to verify parsing and error recovery

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Counters
TOTAL_TESTS=0
PASSED_TESTS=0
FAILED_TESTS=0

# Build the project first
echo -e "${BLUE}Building Phi compiler...${NC}"
if ! make -j4 > /dev/null 2>&1; then
    echo -e "${RED}Build failed! Cannot run tests.${NC}"
    exit 1
fi
echo -e "${GREEN}Build successful!${NC}"
echo

# Function to run a single test
run_test() {
    local file="$1"
    local expected_result="$2"  # "success" or "error"
    local test_name=$(basename "$file" .phi)

    TOTAL_TESTS=$((TOTAL_TESTS + 1))

    echo -n "Testing $test_name... "

    # Run the compiler and capture output
    if ./build/phi "$file" > /tmp/phi_test_output 2>&1; then
        # Compiler returned 0 (success)
        if [ "$expected_result" = "success" ]; then
            echo -e "${GREEN}PASS${NC}"
            PASSED_TESTS=$((PASSED_TESTS + 1))
        else
            echo -e "${RED}FAIL${NC} (expected errors but got success)"
            FAILED_TESTS=$((FAILED_TESTS + 1))
            echo -e "${YELLOW}Output:${NC}"
            head -20 /tmp/phi_test_output | sed 's/^/  /'
            echo
        fi
    else
        # Compiler returned non-zero (error)
        if [ "$expected_result" = "error" ]; then
            echo -e "${GREEN}PASS${NC} (correctly detected errors)"
            PASSED_TESTS=$((PASSED_TESTS + 1))
        else
            echo -e "${RED}FAIL${NC} (expected success but got errors)"
            FAILED_TESTS=$((FAILED_TESTS + 1))
            echo -e "${YELLOW}Output:${NC}"
            head -20 /tmp/phi_test_output | sed 's/^/  /'
            echo
        fi
    fi
}

# Function to run tests in a directory
run_test_directory() {
    local dir="$1"
    local expected_result="$2"
    local description="$3"

    echo -e "${BLUE}Running $description...${NC}"

    if [ ! -d "$dir" ]; then
        echo -e "${YELLOW}Directory $dir not found, skipping...${NC}"
        return
    fi

    for file in "$dir"/*.phi; do
        if [ -f "$file" ]; then
            run_test "$file" "$expected_result"
        fi
    done
    echo
}

# Function to show detailed output for a specific test
show_test_output() {
    local file="$1"
    echo -e "${BLUE}Detailed output for $(basename "$file" .phi):${NC}"
    echo -e "${YELLOW}----------------------------------------${NC}"
    ./build/phi "$file"
    echo -e "${YELLOW}----------------------------------------${NC}"
    echo
}

# Main test execution
main() {
    local mode="$1"
    local specific_test="$2"

    case "$mode" in
        "all"|"")
            echo -e "${BLUE}Running all tests...${NC}"
            echo

            run_test_directory "tests/correct" "success" "correct syntax tests"
            run_test_directory "tests/incorrect" "error" "incorrect syntax tests (error recovery)"

            # Summary
            echo -e "${BLUE}Test Summary:${NC}"
            echo -e "Total tests: $TOTAL_TESTS"
            echo -e "${GREEN}Passed: $PASSED_TESTS${NC}"
            echo -e "${RED}Failed: $FAILED_TESTS${NC}"

            if [ $FAILED_TESTS -eq 0 ]; then
                echo -e "${GREEN}All tests passed!${NC}"
                exit 0
            else
                echo -e "${RED}Some tests failed.${NC}"
                exit 1
            fi
            ;;

        "correct")
            run_test_directory "tests/correct" "success" "correct syntax tests"
            ;;

        "incorrect")
            run_test_directory "tests/incorrect" "error" "incorrect syntax tests (error recovery)"
            ;;

        "show")
            if [ -z "$specific_test" ]; then
                echo -e "${RED}Usage: $0 show <test_file.phi>${NC}"
                exit 1
            fi
            show_test_output "$specific_test"
            ;;

        "help"|"-h"|"--help")
            echo "Phi Compiler Test Runner"
            echo
            echo "Usage: $0 [mode] [options]"
            echo
            echo "Modes:"
            echo "  all         Run all tests (default)"
            echo "  correct     Run only tests that should succeed"
            echo "  incorrect   Run only tests that should fail"
            echo "  show <file> Show detailed output for a specific test file"
            echo "  help        Show this help message"
            echo
            echo "Examples:"
            echo "  $0                                    # Run all tests"
            echo "  $0 correct                            # Run only correct syntax tests"
            echo "  $0 incorrect                          # Run only error recovery tests"
            echo "  $0 show tests/correct/basic_function.phi  # Show detailed output"
            ;;

        *)
            echo -e "${RED}Unknown mode: $mode${NC}"
            echo "Use '$0 help' for usage information."
            exit 1
            ;;
    esac
}

# Check if we're in the right directory
if [ ! -f "build/phi" ]; then
    echo -e "${RED}Error: Phi compiler not found at build/phi${NC}"
    echo "Make sure you're running this from the Phi project root directory."
    exit 1
fi

# Run main function with all arguments
main "$@"
