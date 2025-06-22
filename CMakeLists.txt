cmake_minimum_required(VERSION 3.16)
project(Phi LANGUAGES CXX)

# Set C++ standard
set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Export compile commands for static analysis tools
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

# Add include directory
include_directories(include)

# Collect all source files
file(GLOB_RECURSE SOURCES "src/*.cpp")
file(GLOB_RECURSE HEADERS "include/*.hpp")

# Create the executable
add_executable(phi ${SOURCES} ${HEADERS})

# Set target properties
target_include_directories(phi PRIVATE include)

# Set compiler flags for better debugging and warnings
target_compile_options(phi PRIVATE
    $<$<CXX_COMPILER_ID:GNU,Clang>:-Wall -Wextra -Wpedantic>
    $<$<CONFIG:Debug>:-g -O0>
    $<$<CONFIG:Release>:-O3>
)

# Enable different build types
if(NOT CMAKE_BUILD_TYPE)
    set(CMAKE_BUILD_TYPE Release)
endif()

message(STATUS "Build type: ${CMAKE_BUILD_TYPE}")
message(STATUS "C++ standard: ${CMAKE_CXX_STANDARD}")
