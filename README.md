# The Phi Programming Language

A modern language compiler written in C++26.

## Project Structure

```
Phi/
├── include
│   ├── AST
│   │   ├── Decl.hpp
│   │   ├── Expr.hpp
│   │   ├── Stmt.hpp
│   │   └── Type.hpp
│   ├── ast.hpp
│   ├── Lexer
│   │   ├── Lexer.hpp
│   │   └── Token.hpp
│   ├── parser.hpp
│   ├── sema.hpp
│   └── SrcLocation.hpp
├── src
│   ├── AST
│   │   ├── Decl.cpp
│   │   ├── Expr.cpp
│   │   └── Stmt.cpp
│   ├── ast.cpp
│   ├── Lexer
│   │   ├── Lexer.cpp
│   │   └── token.cpp
│   ├── main.cpp
│   ├── parser.cpp
│   └── sema.cpp
├── example-code/    # Example Phi language files
├── build/
├── CMakeLists.txt
├── Makefile
└── compile_commands.json
```

## Prerequisites

- CMake (3.16 or higher)
- C++26 compatible compiler
- Make (for the convenience Makefile)

## Building

### Using the build script

```bash
./build.sh
```

### Using Make

```bash
# Build the project
make

# Clean build artifacts
make clean

# Build and run
make run

# Debug build
make debug

# Release build (default)
make release
```

### Using CMake directly

```bash
# Create build directory
mkdir build && cd build

# Configure
cmake ..

# Build
make -j4

# Run
./phi
```
