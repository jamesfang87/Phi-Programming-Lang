#include <fstream>
#include <iostream>
#include <print>
#include <string>

#include "CodeGen/CodeGen.hpp"
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
    if (argc < 2 || argc > 3) {
        std::cerr << "Usage: " << argv[0] << " <filename> [--target=PLATFORM]\n";
        std::cerr << "Available targets: windows, linux, macos (default: native)\n";
        return 1;
    }

    std::string target_platform = "native";
    std::string filename;

    // Parse command line arguments
    for (int i = 1; i < argc; i++) {
        std::string arg = argv[i];
        if (arg.starts_with("--target=")) {
            target_platform = arg.substr(9); // Remove "--target="
        } else if (filename.empty()) {
            filename = arg;
        } else {
            std::cerr << "Unknown argument: " << arg << "\n";
            std::cerr << "Usage: " << argv[0] << " <filename> [--target=PLATFORM]\n";
            std::cerr << "Available targets: windows, linux, macos (default: native)\n";
            return 1;
        }
    }

    if (filename.empty()) {
        std::cerr << "Error: No filename provided\n";
        std::cerr << "Usage: " << argv[0] << " <filename> [--target=PLATFORM]\n";
        std::cerr << "Available targets: windows, linux, macos (default: native)\n";
        return 1;
    }

    try {

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

        // Code Generation
        std::println("Generating LLVM IR...");
        std::string target_triple = "";
        if (target_platform == "windows") {
            target_triple = "x86_64-pc-windows-msvc";
        } else if (target_platform == "linux") {
            target_triple = "x86_64-unknown-linux-gnu";
        } else if (target_platform == "macos") {
            target_triple = "x86_64-apple-darwin";
        } else if (target_platform != "native") {
            std::cerr << "Error: Unknown target platform '" << target_platform << "'\n";
            std::cerr << "Available targets: windows, linux, macos, native\n";
            return 1;
        }
        phi::CodeGen codegen(std::move(resolved_ast), filename, target_triple);
        codegen.generate();

        // Output IR to file
        std::string ir_filename = filename;
        size_t dot_pos = ir_filename.find_last_of('.');
        if (dot_pos != std::string::npos) {
            ir_filename = ir_filename.substr(0, dot_pos);
        }
        ir_filename += ".ll";

        codegen.output_ir(ir_filename);
        std::println("LLVM IR written to: {}", ir_filename);
        if (target_platform == "windows") {
            std::println("Windows target IR generated. You can cross-compile with:");
            std::println("clang --target=x86_64-pc-windows-msvc {} -o output.exe", ir_filename);
        } else if (target_platform == "linux") {
            std::println("Linux target IR generated. You can cross-compile with:");
            std::println("clang --target=x86_64-unknown-linux-gnu {} -o output", ir_filename);
        } else if (target_platform == "macos") {
            std::println("macOS target IR generated. You can cross-compile with:");
            std::println("clang --target=x86_64-apple-darwin {} -o output", ir_filename);
        } else {
            std::println("You can compile it with: clang {} -o output", ir_filename);
        }

    } catch (const std::exception& e) {
        std::println(std::cerr, "Error: {}", e.what());
        return 1;
    }
    return 0;
}
