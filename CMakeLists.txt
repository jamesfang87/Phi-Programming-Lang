cmake_minimum_required(VERSION 3.16)

project(Phi LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 23)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

# Export compile commands for static analysis tools
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

# Find LLVM installation
find_package(LLVM REQUIRED CONFIG PATHS ~/llvm-project/build)

message(STATUS "Found LLVM ${LLVM_PACKAGE_VERSION}")
message(STATUS "Using LLVMConfig.cmake in: ${LLVM_DIR}")

# Set up LLVM
include_directories(${LLVM_INCLUDE_DIRS})
separate_arguments(LLVM_DEFINITIONS_LIST NATIVE_COMMAND ${LLVM_DEFINITIONS})
add_definitions(${LLVM_DEFINITIONS_LIST})

# Add include directory
include_directories(include)

# Collect all source files
file(GLOB_RECURSE SOURCES "src/*.cpp")
file(GLOB_RECURSE HEADERS "include/*.hpp")

# Create the executable
add_executable(phi ${SOURCES} ${HEADERS})

# Set target properties
target_include_directories(phi PRIVATE include)

# Link against LLVM libraries
llvm_map_components_to_libnames(llvm_libs support core irreader)
target_link_libraries(phi ${llvm_libs})

# Set compiler flags for better debugging and warnings
target_compile_options(phi PRIVATE
    $<$<CXX_COMPILER_ID:GNU>:-Wall -Wextra -Wpedantic>
    $<$<CONFIG:Debug>:-g -O0>
    $<$<CONFIG:Release>:-O3>
)

# Enable different build types
if(NOT CMAKE_BUILD_TYPE)
    set(CMAKE_BUILD_TYPE Release)
endif()

message(STATUS "Build type: ${CMAKE_BUILD_TYPE}")
message(STATUS "C++ standard: ${CMAKE_CXX_STANDARD}")
