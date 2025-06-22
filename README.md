# Phi Language Compiler

A modern language compiler written in C++20.

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
├── build/           # Build output directory (generated)
├── CMakeLists.txt   # CMake build configuration
├── Makefile         # Convenience Makefile wrapper
├── .clangd          # Static analysis configuration
└── compile_commands.json  # For IDE/static analysis tools
```

## Prerequisites

- CMake (3.16 or higher)
- C++20 compatible compiler (GCC 10+, Clang 10+, or MSVC 2019+)
- Make (for the convenience Makefile)

## Building

### Using Make (Recommended)

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

### Using the build script

```bash
./build.sh
```

## IDE Setup for Static Code Analysis

### Neovim with LSP

The project is configured to work with clangd for static code analysis:

1. **compile_commands.json**: Automatically generated during build and copied to the root directory
2. **.clangd**: Configuration file for clangd LSP server
3. **Include paths**: Properly configured in CMake to include the `include/` directory

Your nvim LSP should automatically detect:
- Header files in the `include/` directory
- Proper C++20 standard
- Include paths and compilation flags

#### Required nvim plugins (examples):
- `neovim/nvim-lspconfig` for LSP support
- `clangd` language server installed

#### Installation of clangd:
```bash
# macOS
brew install llvm

# Ubuntu/Debian
sudo apt install clangd

# Or use your package manager
```

### VS Code

Install the C/C++ extension, and it should automatically use the `compile_commands.json`.

### Other IDEs

Most modern C++ IDEs support `compile_commands.json` for project configuration.

## Development

### Adding New Files

1. **Headers**: Add `.hpp` files to the `include/` directory
2. **Source**: Add `.cpp` files to the `src/` directory
3. **CMake**: The build system automatically detects new files using `GLOB_RECURSE`

### Code Style

- C++20 standard
- Use your existing `.clang-format` configuration
- Static analysis enabled with clang-tidy rules

### Debugging

```bash
# Build with debug symbols
make debug

# Use with gdb/lldb
lldb ./build/phi
```

## Troubleshooting

### Static Analysis Not Working

1. Ensure `compile_commands.json` exists in the project root
2. Check that clangd is installed and configured in your editor
3. Rebuild the project to regenerate compilation database

### Build Issues

1. Ensure you have a C++20 compatible compiler
2. Check CMake version (minimum 3.16 required)
3. Clean and rebuild: `make clean && make`
