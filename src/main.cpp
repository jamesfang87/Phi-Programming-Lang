#include "scanner.hpp"
#include <cctype>
#include <fstream>
#include <iostream>
#include <print>
#include <string>

std::string read_file_to_string(const std::string& filename) {
    std::ifstream file(filename);
    if (!file.is_open()) {
        throw std::runtime_error("Could not open file: " + filename);
    }
    return std::string((std::istreambuf_iterator<char>(file)), std::istreambuf_iterator<char>());
}

int main(int argc, char* argv[]) {
    if (argc != 2) {
        std::cerr << "Usage: " << argv[0] << " <filename>\n";
        return 1;
    }

    try {
        std::string filename = argv[1];
        std::string source = read_file_to_string(filename);
        std::println("File content:\n{}", source);

        Scanner scanner(source, filename);
        auto [success, tokens] = scanner.scan();

        if (!success) {
            std::println("\033[31;1;4merror:\033[0m exiting due to previous error(s)");
            return 0;
        }

        for (const auto& t : tokens) {
            std::cout << t.as_str() << '\n';
        }

    } catch (const std::exception& e) {
        std::println(std::cerr, "Error: {}", e.what());
        return 1;
    }
    return 0;
}
