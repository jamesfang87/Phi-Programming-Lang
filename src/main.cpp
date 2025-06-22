#include "scanner.hpp"
#include "token.hpp"
#include <fstream>
#include <iostream>
#include <stdexcept>
#include <string>

std::string read_file_to_string(const std::string& file_path) {
    // Open file in binary mode to preserve all characters
    std::ifstream file(file_path, std::ios::binary);

    if (!file.is_open()) {
        throw std::runtime_error("Failed to open file: " + file_path);
    }

    // Find file size
    file.seekg(0, std::ios::end);
    size_t file_size = file.tellg();
    file.seekg(0, std::ios::beg);

    // Create string buffer and read content
    std::string content;
    content.resize(file_size);
    file.read(&content[0], file_size);

    // Check for read errors
    if (!file) {
        throw std::runtime_error("Error reading file: " + file_path);
    }

    return content;
}

int main() {
    try {
        std::string source = read_file_to_string("program.phi");
        std::cout << "File content:\n" << source << "\n";

        auto it = source.begin();
        while (it <= source.end()) {
            std::cout << *it << '\n';
            it++;
        }

        // Pass to scanner
        // Scanner scanner(source);
        // auto tokens = scanner.scan();

    } catch (const std::exception& e) {
        std::cerr << "Error: " << e.what() << std::endl;
        return 1;
    }
    return 0;
}
