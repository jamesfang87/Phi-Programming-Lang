#include <print>
#include <string>

#pragma once
class Diagnostic {
public:
    static void error(const std::string& message) { std::println("Error: {}", message); }
};
