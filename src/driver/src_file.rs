//! This file defines `SrcFile`, a single source file tracked by the compiler.
//!
//! Each `SrcFile` stores its full content, its offset within the global `SrcMap` address
//! space, and precomputed line-start offsets. Together these let a global offset be turned
//! back into a line/column position without rescanning the file.

/// A single source file the compiler has read, addressed within the shared `SrcMap` offset
/// space rather than at file-local offsets.
pub struct SrcFile {
    pub name: String,
    pub content: Vec<char>,
    /// The offset of this file's first char within the whole `SrcMap`'s global address space.
    pub global_offset: usize,
    /// The global offset at which each line of this file starts.
    pub line_starts: Vec<usize>,
}

impl SrcFile {
    /// Creates a new `SrcFile` and precomputes its line-start offsets.
    ///
    /// Precomputing lets [`SrcFile::line_col`] locate a position with a binary search instead
    /// of rescanning the file on every call.
    pub fn new(name: String, content: Vec<char>, global_offset: usize) -> Self {
        // Line 1 starts at the file's own global offset.
        let mut line_starts = vec![global_offset];

        // Scan the file once to find every newline.
        for (i, &char) in content.iter().enumerate() {
            if char == '\n' {
                // The next line starts right after the newline, in global offset space.
                line_starts.push(global_offset + i + 1);
            }
        }

        SrcFile {
            name,
            content,
            global_offset,
            line_starts,
        }
    }

    /// Converts a *global* offset that falls within this file into a 1-based (line, column).
    pub fn line_col(&self, pos: usize) -> (usize, usize) {
        // Binary search for the line this position falls on: the largest line start that is
        // less than or equal to `pos`.
        let line_idx = match self.line_starts.binary_search(&pos) {
            // `pos` sits exactly at a line start.
            Ok(idx) => idx,
            // `pos` sits between two line starts, so the enclosing line is the one before it.
            Err(idx) => idx - 1,
        };

        let col_char_offset = pos - self.line_starts[line_idx];

        // Converts the 0-based indices into 1-based, user-facing line and column numbers.
        (line_idx + 1, col_char_offset + 1)
    }
}
