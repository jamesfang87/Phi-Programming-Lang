use std::fs;
use std::io;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

/// [`SrcSpan`] is a half-open range of character (not byte) offsets into the
/// compiler's source map.
///
/// Offsets stored in [`SrcSpan`] are global. Thus, the offsets of a span not
/// only record information about a position in a file, but also which file.
/// This removes the requirement to carry a separate file id, reducing the
/// memory footprint of the compiler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SrcSpan {
    begin: usize,
    end: usize,
}

#[allow(dead_code)]
impl SrcSpan {
    pub fn new(begin: usize, end: usize) -> SrcSpan {
        SrcSpan { begin, end }
    }

    pub fn get_begin(&self) -> usize {
        self.begin
    }

    pub fn get_end(&self) -> usize {
        self.end
    }

    pub fn as_tuple(&self) -> (usize, usize) {
        (self.begin, self.end)
    }

    /// Returns the smallest span that covers both `self` and `other`.
    ///
    /// Used to build a span for a larger syntax node out of its parts' spans, e.g. a whole
    /// binary expression from its left and right operand spans.
    pub fn merge(self, other: SrcSpan) -> SrcSpan {
        SrcSpan::new(
            self.begin.min(other.get_begin()),
            self.end.max(other.get_end()),
        )
    }
}

/// Where a source file came from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FileOrigin {
    /// A file the user wrote, found under the project root by the `SrcCollector`.
    User,
    /// A file of the core library, embedded in the compiler binary itself.
    Core,
}

pub struct SrcFile {
    pub name: String,
    pub content: Vec<char>,
    pub origin: FileOrigin,
    /// The offset of this file's first char within the whole `SrcMap`'s global address space.
    pub global_offset: usize,
    /// The global offset at which each line of this file starts.
    pub line_starts: Vec<usize>,
}

impl SrcFile {
    pub fn new(name: String, content: Vec<char>, origin: FileOrigin, global_offset: usize) -> Self {
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
            origin,
            global_offset,
            line_starts,
        }
    }

    /// Converts a global offset that falls within this file into a 1-based (line, column).
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

/// Mutable state for the global source map.
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

/// Every file of the core library as `(name, source)`.
const CORE_FILES: &[(&str, &str)] = &[
    ("core/iter.phi", include_str!("../../lib/core/iter.phi")),
    ("core/ops.phi", include_str!("../../lib/core/ops.phi")),
    ("core/option.phi", include_str!("../../lib/core/option.phi")),
    (
        "core/prelude.phi",
        include_str!("../../lib/core/prelude.phi"),
    ),
    ("core/result.phi", include_str!("../../lib/core/result.phi")),
];

/// Namespace for discovering source files and registering them with the [`SrcMap`].
pub struct SrcCollector;

impl SrcCollector {
    /// Recursively finds all `.phi` files under `root` and inserts them into the source map.
    pub fn collect(root: &Path) -> io::Result<()> {
        Self::visit_dir(root)
    }

    /// Registers every core library file with the [`SrcMap`], in the order [`CORE_FILES`]
    /// lists them, and returns exactly the [`SrcFile`]s this call registered.
    ///
    /// Phi has no notion of a separately compiled library yet, so `core` is compiled into the
    /// same unit as the user's own files, from source, on every build. Its files carry
    /// ordinary `module core::..;` declarations, so lowering assembles them into the module
    /// tree exactly as it does the user's -- nothing downstream needs to know `core` is
    /// special.
    ///
    /// The one thing that is special is when they're registered: this runs after
    /// [`SrcCollector::collect`] has walked the project, so `core` sits at the end of the
    /// offset space and editing it doesn't shift the span of every user file in the build.
    ///
    /// Which items `core` is expected to declare is not recorded here but in
    /// [`crate::langitems`], which resolves each one to its `DefId` after name resolution has
    /// built `core`'s namespace.
    ///
    /// The return value identifies "the files just registered" by the files themselves, not by
    /// a before/after count of [`SrcMap::files`] -- `SrcMap` is a process-wide map behind a
    /// shared lock (unlike `Interner` and `DiagCtx`, which are thread-local), so under a
    /// multi-threaded test runner some other thread can register a file of its own in between
    /// two calls here. A length snapshot taken before this call and compared to one taken after
    /// would be racy: it can capture a file this call never registered. Each `add_file` call
    /// instead reports back exactly the offset -- and so exactly the file -- it created, which
    /// stays correct no matter what any other thread does concurrently.
    pub fn collect_core() -> Vec<&'static SrcFile> {
        CORE_FILES
            .iter()
            .map(|&(name, source)| {
                let offset =
                    SrcMap::add_file(name.to_string(), source.chars().collect(), FileOrigin::Core);
                SrcMap::file_containing(offset)
                    .expect("the file this call just registered at `offset`")
            })
            .collect()
    }

    fn visit_dir(dir: &Path) -> io::Result<()> {
        if !dir.is_dir() {
            return Ok(());
        }

        // `read_dir`'s order is OS-dependent.
        //
        // Sort by file name so file collection, and therefore every downstream stage that
        // depends on it, such as diagnostic output and `--ast` output, stays reproducible.
        let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<Result<_, _>>()?;
        entries.sort_by_key(|entry| entry.file_name());

        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                Self::visit_dir(&path)?;
            } else if path.extension().and_then(|s| s.to_str()) == Some("phi") {
                let name = path.to_string_lossy().into_owned();
                let content = fs::read_to_string(&path)?.chars().collect::<Vec<char>>();
                SrcMap::add_file(name, content, FileOrigin::User);
            }
        }
        Ok(())
    }
}
