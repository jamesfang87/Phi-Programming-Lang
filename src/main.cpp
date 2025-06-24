#include "scanner.hpp"
#include <cctype>
#include <fstream>
#include <iostream>
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
        std::cout << "File content:\n" << source << "\n";

        Scanner scanner(source);
        auto tokens = scanner.scan();

        for (const auto& t : tokens) {
            std::cout << t.as_str() << '\n';
        }

    } catch (const std::exception& e) {
        std::cerr << "Error: " << e.what() << std::endl;
        return 1;
    }
    return 0;
}
