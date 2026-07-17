pub mod compiler;
pub mod file_collector;
pub mod src_file;
pub mod src_map;

use crate::driver::src_map::SrcMap;
use std::env;
use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;

pub struct Driver;

impl Driver {
    /// Create a new Phi project directory.
    pub fn new_project(name: &str) -> io::Result<PathBuf> {
        let path = Path::new(name);
        if path.exists() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("destination `{}` already exists", name),
            ));
        }
        fs::create_dir(path)?;
        Self::init_project(path)?;
        Ok(path.to_path_buf())
    }

    /// Initialise a project in an existing directory.
    pub fn init_project(path: &Path) -> io::Result<()> {
        if !path.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "`init` requires a directory",
            ));
        }
        let src_dir = path.join("src");
        fs::create_dir_all(&src_dir)?;

        let main_phi = src_dir.join("main.phi");
        let template = b"// Hello, Phi!\nfn main() {\n    println(\"Hello, world!\");\n}\n";
        fs::write(&main_phi, template)?;

        let manifest_name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let manifest = format!(
            "[project]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2026\"\n",
            manifest_name
        );
        fs::write(path.join("phi.toml"), manifest)?;

        println!("Created new Phi project at: {}", path.display());
        Ok(())
    }

    pub fn print_usage() {
        let prog = env::args().next().unwrap_or_else(|| "phi".into());
        eprintln!("Usage:");
        eprintln!("  {} new <project_name>", prog);
        eprintln!("  {} init [path]", prog);
        eprintln!("  {} build [--ast]", prog);
        eprintln!("  {} help", prog);
    }
}
