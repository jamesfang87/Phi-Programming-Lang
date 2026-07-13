use crate::driver::src_file::SrcFile;
use crate::lexer::src_span::SrcSpan;
use std::sync::{Mutex, OnceLock};

/// Mutable state backing the global source map: every file added so far, plus the running
/// offset new files get appended at. Files are never removed, and each `SrcFile` is leaked to
/// `'static` when added, so handing out `&'static SrcFile` references remains sound even as
/// this list grows.
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

/// Namespace for the process-wide source map: every source file the compiler has read, indexed
/// by a shared global char-offset space so spans can be resolved back to text without threading
/// a source reference through every stage of the pipeline.
pub struct SrcMap;

impl SrcMap {
    pub fn files() -> Vec<&'static SrcFile> {
        state().lock().unwrap().files.clone()
    }

    pub fn file_containing(offset: usize) -> Option<&'static SrcFile> {
        state()
            .lock()
            .unwrap()
            .files
            .iter()
            .find(|f| offset >= f.global_offset && offset < f.global_offset + f.content.len())
            .copied()
    }

    pub fn text_of(span: SrcSpan) -> Option<String> {
        Self::chars_of(span).map(|chars| chars.iter().collect())
    }

    pub fn chars_of(span: SrcSpan) -> Option<&'static [char]> {
        let file = Self::file_containing(span.get_begin())?;
        let begin = span.get_begin() - file.global_offset;
        let end = span.get_end() - file.global_offset;
        Some(&file.content[begin..end])
    }

    /// Registers a new source file, returning the global offset its content starts at.
    pub fn add_file(name: String, content: Vec<char>) -> usize {
        let mut st = state().lock().unwrap();
        let offset = st.cur_offset;
        let len = content.len();
        let file: &'static SrcFile = Box::leak(Box::new(SrcFile::new(name, content, offset)));
        st.files.push(file);
        st.cur_offset += len;
        offset
    }
}

/// A builder for assembling files into the global `SrcMap`. Each `add_file` call registers the
/// file immediately; `finish` is a no-op kept for call-site readability.
pub struct SrcMapBuilder;

impl SrcMapBuilder {
    pub fn new() -> Self {
        SrcMapBuilder
    }

    pub fn add_file(self, name: String, content: Vec<char>) -> Self {
        SrcMap::add_file(name, content);
        self
    }

    pub fn finish(self) {}
}
