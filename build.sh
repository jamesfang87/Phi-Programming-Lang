#!/bin/bash

# Create build directory if it doesn't exist
mkdir -p build

# Configure and build the project
cd build
cmake ..
make -j$(nproc 2>/dev/null || echo 4)

# Copy compile_commands.json to root for static analysis
cp compile_commands.json ../

echo "Build complete! Executable: build/phi"
