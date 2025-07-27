/**
 * @file token.hpp
 * @brief Token definitions and Token class for the Phi programming language
 * lexer
 *
 * This file contains the TokenType enumeration that defines all possible token
 * types that can be recognized by the scanner, and the Token class that
 * represents a single token with its type, lexeme, and source location
 * information.
 */
#pragma once

#include <string>

#include "Lexer/TokenType.hpp"
#include "SrcManager/SrcLocation.hpp"

/**
 * @brief Represents a single token in the Phi programming language
 *
 * A Token contains all the information needed to represent a lexical unit:
 * the token type, the original text (lexeme), and its position in the source
 * code. This class is immutable once constructed and provides accessor methods
 * for all token properties.
 */
class Token {
public:
    /**
     * @brief Constructs a new Token
     * @param start The starting source location where this token appears
     * @param end The ending source location where this token appears
     * @param type The type of this token
     * @param lexeme The actual text content of this token from the source
     */
    Token(SrcLocation start, SrcLocation end, const TokenType type, std::string lexeme)
        : start(std::move(start)),
          end(std::move(end)),
          type(type),
          lexeme(std::move(lexeme)) {}

    /**
     * @brief Gets the starting source location where this token appears
     * @return The SrcLocation object representing the token's starting position
     */
    [[nodiscard]] const SrcLocation& get_start() const { return start; }

    /**
     * @brief Gets the ending source location where this token appears
     * @return The SrcLocation object representing the token's ending position
     */
    [[nodiscard]] const SrcLocation& get_end() const { return end; }

    /**
     * @brief Gets the type of this token
     * @return The TokenType enumeration value
     */
    [[nodiscard]] TokenType get_type() const { return type; }

    [[nodiscard]] std::string get_name() const { return type_to_string(type); }

    /**
     * @brief Gets the lexeme (original text) of this token
     * @return The string content that was scanned to create this token
     */
    [[nodiscard]] std::string get_lexeme() const { return lexeme; }

    /**
     * @brief Converts this token to a human-readable string representation
     * @return A formatted string showing the token type, lexeme, and position
     */
    [[nodiscard]] std::string to_string() const;

private:
    SrcLocation start, end; ///< Source location (line and column numbers)
    TokenType type;         ///< The type of this token
    std::string lexeme;     ///< The original text content from source
};
