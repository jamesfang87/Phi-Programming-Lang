use crate::driver::src_map::SrcMapBuilder;
use std::fs;
use std::io;
use std::path::Path;

pub struct FileCollector;

impl FileCollector {
    /// Recursively find all `.phi` files under `root` and insert into `src_map`.
    pub fn collect(root: &Path) -> io::Result<()> {
        Self::visit_dir(root)?;
        Ok(())
    }

    fn visit_dir(dir: &Path) -> io::Result<()> {
        if !dir.is_dir() {
            return Ok(());
        }

        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                Self::visit_dir(&path)?;
            } else if path.extension().and_then(|s| s.to_str()) == Some("phi") {
                let name = path.to_string_lossy().into_owned();
                let content = fs::read_to_string(&path)? // read as UTF-8 string
                    .chars() // iterator of chars
                    .collect::<Vec<char>>(); // collect into Vec<char>
                SrcMapBuilder::new().add_file(name, content);
            }
        }
        Ok(())
    }
}
