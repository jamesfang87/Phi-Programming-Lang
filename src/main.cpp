#include <fstream>
#include <iostream>
#include <print>
#include <string>

#include "Lexer/Lexer.hpp"
#include "Parser/Parser.hpp"
#include "Sema/Sema.hpp"

std::string read_file_to_string(const std::string& filename) {
    std::ifstream file(filename);
    if (!file.is_open()) {
        throw std::runtime_error("Could not open file: " + filename);
    }
    return std::string{std::istreambuf_iterator(file), std::istreambuf_iterator<char>()};
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

        auto source_manager = std::make_shared<phi::SrcManager>();
        auto diagnostic_manager = std::make_shared<phi::DiagnosticManager>(source_manager);

        // Register source content for diagnostic display
        source_manager->add_source_file(filename, source);

        phi::Lexer scanner(source, filename, diagnostic_manager);
        auto [tokens, scan_success] = scanner.scan();

        if (!scan_success) {
            std::println("\033[31;1;4merror:\033[0m exiting due to previous error(s)");
            return 1;
        }

        std::println("Tokens:");
        for (const auto& t : tokens) {
            std::cout << t.to_string() << '\n';
        }

        std::println("\nParsing results: ");
        phi::Parser parser(source, filename, tokens, diagnostic_manager);
        auto [ast, parse_success] = parser.parse();
        if (!parse_success) {
            std::println("\033[31;1;4merror:\033[0m exiting due to previous error(s)");
            return 1;
        }

        for (const auto& fun : ast) {
            fun->info_dump(0);
        }

        std::println("Semantic Analysis:");
        phi::Sema analyzer(std::move(ast));
        auto [success, resolved_ast] = analyzer.resolve_ast();
        if (!success) {
            std::println("\033[31;1;4merror:\033[0m exiting due to previous error(s)");
            return 1;
        }

        for (const auto& fun : resolved_ast) {
            fun->info_dump(0);
        }

        std::println("Semantic analysis completed successfully!");

    } catch (const std::exception& e) {
        std::println(std::cerr, "Error: {}", e.what());
        return 1;
    }
    return 0;
}
