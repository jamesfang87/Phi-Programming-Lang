# The Phi Programming Language

A modern language compiler written in C++23.

## Prerequisites

- CMake (3.16 or higher)
- C++23 compatible compiler
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
