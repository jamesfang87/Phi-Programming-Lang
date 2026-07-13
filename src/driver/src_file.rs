pub struct SrcFile {
    pub name: String,
    pub content: Vec<char>,
    /// The offset of this file's first char in the whole `SrcMap`'s global address space.
    pub global_offset: usize,
    /// Global offsets at which each line starts.
    pub line_starts: Vec<usize>,
}

impl SrcFile {
    /// Creates a new SrcFile and precomputes the line boundaries.
    pub fn new(name: String, content: Vec<char>, global_offset: usize) -> Self {
        let mut line_starts = vec![global_offset]; // Line 1 starts at the file's global offset

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
        // Binary search to find the line where this position lives.
        // We find the largest line_start that is <= pos.
        let line_idx = match self.line_starts.binary_search(&pos) {
            Ok(idx) => idx,      // Exactly at a line start
            Err(idx) => idx - 1, // Between two line starts
        };

        let col_char_offset = pos - self.line_starts[line_idx];

        // Convert 0-based indices to 1-based user-friendly numbers.
        (line_idx + 1, col_char_offset + 1)
    }
}
