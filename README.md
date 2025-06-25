# Phi Language Compiler

A modern language compiler written in C++26.

## Project Structure

```
Phi/
├── include/          # Header files (.hpp)
│   ├── token.hpp
│   ├── token_types.hpp
│   └── scanner.hpp
├── src/             # Source files (.cpp)
│   ├── main.cpp
│   ├── token.cpp
│   └── scanner.cpp
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
