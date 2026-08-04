//! This file defines `SrcMap`, the compiler's process-wide source map.
//!
//! Every source file the compiler reads is registered here under a shared, monotonically
//! increasing char-offset space. This lets a `SrcSpan` be resolved back to its owning file
//! and text without threading a source reference through every stage of the pipeline.

use crate::driver::src_file::{FileOrigin, SrcFile};
use crate::lexer::src_span::SrcSpan;
use std::sync::{Mutex, OnceLock};

/// Mutable state for the global source map.
///
/// Tracks every file added so far, plus the running offset at which the next file gets
/// appended. Files are never removed, and each `SrcFile` is leaked to `'static` when added,
/// so handing out `&'static SrcFile` references stays sound even as this list grows.
struct SrcMapState {
    files: Vec<&'static SrcFile>,
    cur_offset: usize,
}

static STATE: OnceLock<Mutex<SrcMapState>> = OnceLock::new();

fn state() -> &'static Mutex<SrcMapState> {
    STATE.get_or_init(|| {
        Mutex::new(SrcMapState {
            files: Vec::new(),
            cur_offset: 0,
        })
    })
}

/// Namespace for the process-wide source map.
///
/// Every source file the compiler has read is indexed here by a shared global char-offset
/// space, so spans can be resolved back to text without threading a source reference through
/// every stage of the pipeline.
pub struct SrcMap;

impl SrcMap {
    /// Returns every registered file, in the order it was added.
    pub fn files() -> Vec<&'static SrcFile> {
        state().lock().unwrap().files.clone()
    }

    /// Returns the file whose global offset range contains `offset`.
    ///
    /// Returns `None` if no registered file covers that offset.
    pub fn file_containing(offset: usize) -> Option<&'static SrcFile> {
        state()
            .lock()
            .unwrap()
            .files
            .iter()
            .find(|f| offset >= f.global_offset && offset < f.global_offset + f.content.len())
            .copied()
    }

    /// Returns the source text covered by `span` as an owned `String`.
    ///
    /// Returns `None` if `span` doesn't fall within any registered file.
    pub fn text_of(span: SrcSpan) -> Option<String> {
        Self::chars_of(span).map(|chars| chars.iter().collect())
    }

    /// Returns the chars covered by `span`, borrowed from the owning file's stored content.
    ///
    /// Returns `None` if `span` doesn't fall within any registered file.
    pub fn chars_of(span: SrcSpan) -> Option<&'static [char]> {
        let file = Self::file_containing(span.get_begin())?;
        let begin = span.get_begin() - file.global_offset;
        let end = span.get_end() - file.global_offset;
        Some(&file.content[begin..end])
    }

    /// Registers a new source file, returning the global offset its content starts at.
    pub fn add_file(name: String, content: Vec<char>, origin: FileOrigin) -> usize {
        let mut st = state().lock().unwrap();
        let offset = st.cur_offset;
        let len = content.len();
        let file: &'static SrcFile =
            Box::leak(Box::new(SrcFile::new(name, content, origin, offset)));
        st.files.push(file);
        st.cur_offset += len;
        offset
    }
}
