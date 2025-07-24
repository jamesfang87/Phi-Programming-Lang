/**
 * @file Lexer.hpp
 * @brief Lexical analyzer (scanner) for the Phi programming language
 *
 * This file contains the Lexer class which performs lexical analysis on Phi
 * source code, converting a stream of characters into a sequence of tokens.
 * The scanner handles all lexical elements including keywords, operators,
 * literals, identifiers, comments, and whitespace.
 */
#pragma once

#include "token.hpp"
#include <string>
#include <vector>


/**
 * @brief Lexical analyzer for the Phi programming language
 *
 * The Lexer class takes a string of source code and converts it into a
 * sequence of tokens. It handles all aspects of lexical analysis including:
 * - Keyword recognition
 * - Operator tokenization
 * - String and character literal parsing with escape sequences
 * - Numeric literal parsing (integers and floats)
 * - Identifier recognition
 * - Comment skipping (both line and block comments)
 * - Error reporting with source location information
 *
 * The scanner maintains source location information (line and column numbers)
 * for each token, which is essential for error reporting and debugging.
 */
class Lexer {
public:
    /**
     * @brief Constructs a new Lexer for the given source code
     * @param src The source code string to scan
     * @param path The file path of the source (used for error reporting)
     */
    Lexer(std::string src, std::string path)
        : src(std::move(src)),
          path(std::move(path)) {
        cur_char = this->src.begin();
        cur_lexeme = this->src.begin();
        cur_line = this->src.begin();
        lexeme_line = this->src.begin();
    }

    /**
     * @brief Scans the source code and produces a sequence of tokens
     *
     * This is the main entry point for lexical analysis. It processes the
     * entire source string character by character, producing tokens and
     * handling comments and whitespace appropriately.
     *
     * @return A pair containing:
     *         - A vector of all tokens found in the source
     *         - A boolean indicating whether scanning was successful (no
     * errors)
     */
    std::pair<std::vector<Token>, bool> scan();

    /**
     * @brief Gets the source code being scanned
     * @return A copy of the source code string
     */
    [[nodiscard]] std::string get_src() const { return src; }

    /**
     * @brief Gets the file path of the source being scanned
     * @return A copy of the file path string
     */
    [[nodiscard]] std::string get_path() const { return path; }

private:
    std::string src;  ///< The source code being scanned
    std::string path; ///< File path for error reporting

    int line_num = 1;                  ///< Current line number (1-indexed)
    std::string::iterator cur_char;    ///< Current character position
    std::string::iterator cur_lexeme;  ///< Start of current lexeme
    std::string::iterator cur_line;    ///< Start of current line
    std::string::iterator lexeme_line; ///< Start current lexeme's line

    bool inside_str = false;

    bool successful = true; ///< Whether scanning completed without errors

    // MAIN SCANNING LOGIC

    /**
     * @brief Scans a single token from the current position
     * @return The next token in the source
     */
    Token scan_token();

    // TOKEN PARSING METHODS

    /**
     * @brief Parses a numeric literal (integer or floating point)
     * @return A token representing the parsed number
     */
    Token parse_number();

    /**
     * @brief Parses a string literal with escape sequence handling
     * @return A token representing the parsed string
     */
    Token parse_string();

    /**
     * @brief Parses a character literal with escape sequence handling
     * @return A token representing the parsed character
     */
    Token parse_char();

    /**
     * @brief Parses an identifier or keyword
     * @return A token representing either an identifier or recognized keyword
     */
    Token parse_identifier();

    // UTILITY FUNCTIONS

    /**
     * @brief Skips over comment text (both line and block comments)
     *
     * This method handles both single-line comments and multi-line
     * block comments. It properly tracks line numbers when
     * skipping over newlines within comments.
     */
    void skip_comment();

    /**
     * @brief Peeks at the current character without advancing
     * @return The current character, or '\0' if at end of file
     */
    [[nodiscard]] char peek_char() const { return reached_eof() ? '\0' : *cur_char; }

    /**
     * @brief Peeks at the next character without advancing
     * @return The next character, or '\0' if at or past end of file
     */
    [[nodiscard]] char peek_next() const {
        if (reached_eof() || cur_char + 1 >= src.end()) {
            return '\0';
        }
        return *(cur_char + 1);
    }

    /**
     * @brief Advances to the next character and returns the current one
     * @return The character that was advanced past, or '\0' if at EOF
     */
    char advance_char() {
        if (reached_eof()) return '\0';
        return *cur_char++;
    }

    /**
     * @brief Conditionally advances if the current character matches expected
     * @param next The character to match against
     * @return true if the character matched and scanner advanced, false
     * otherwise
     */
    bool match_next(const char next) {
        if (reached_eof() || peek_char() != next) {
            return false;
        }
        ++cur_char;
        return true;
    }

    std::string peek_next_n(const int n) const {
        auto temp = cur_char;
        for (int i = 0; i < n; ++i) {
            if (reached_eof()) {
                return "";
            }
            ++temp;
        }
        return std::string{cur_char, temp};
    }

    bool match_next_n(const std::string_view next) {
        auto temp = cur_char;
        for (const char& c : next) {
            if (reached_eof() || *temp != c) {
                return false;
            }
            ++temp;
        }
        cur_char = temp;
        return true;
    }

    /**
     * @brief Checks if the scanner has reached the end of the source
     * @return true if at end of file, false otherwise
     */
    [[nodiscard]] bool reached_eof() const { return cur_char >= src.end(); }

    /**
     * @brief Creates a token from the current lexeme
     * @param type The type of token to create
     * @return A new Token with the specified type and current lexeme
     */
    Token make_token(TokenType type) const {
        int start_col = static_cast<int>(cur_lexeme - lexeme_line) + 1;
        int end_col = std::distance(cur_char, cur_line) + 1;

        // TODO: correctly use start line
        int start_line = line_num;
        int end_line = line_num;

        return {SrcLocation{.path = this->path, .line = start_line, .col = start_col},
                SrcLocation{.path = this->path, .line = end_line, .col = end_col},
                type,
                std::string(cur_lexeme, cur_char)};
    }

    /**
     * @brief Parses an escape sequence within a string or character literal
     * @return The actual character value represented by the escape sequence
     */
    char parse_escape_sequence();

    /**
     * @brief Parses a hexadecimal escape sequence (\xNN)
     * @return The character value represented by the hex escape
     */
    char parse_hex_escape();

    // ERROR HANDLING

    /**
     * @brief Reports a scanning error with formatted output
     *
     * This method handles error reporting by printing a formatted error message
     * that includes the source location, error description, and a visual
     * indicator of where the error occurred in the source code.
     *
     * @param message The main error message describing what went wrong
     * @param expected_message Additional context about what was expected
     */
    void throw_lexer_error(std::string_view message, std::string_view expected_message);

    void resync_scanner();
};
