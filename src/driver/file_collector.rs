//! This file defines `FileCollector`, which discovers a project's `.phi` source files.
//!
//! It walks the directory tree under a given root and registers every `.phi` file it finds
//! in the global `SrcMap`, in a reproducible order.

use crate::driver::src_file::FileOrigin;
use crate::driver::src_map::SrcMapBuilder;
use std::fs;
use std::io;
use std::path::Path;

pub struct FileCollector;

impl FileCollector {
    /// Recursively finds all `.phi` files under `root` and inserts them into the source map.
    pub fn collect(root: &Path) -> io::Result<()> {
        Self::visit_dir(root)?;
        Ok(())
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
                SrcMapBuilder::new().add_file(name, content, FileOrigin::User);
            }
        }
        Ok(())
    }
}
