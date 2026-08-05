use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub fn new(project_name: &str) -> io::Result<PathBuf> {
    let path = Path::new(project_name);
    if path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("destination `{}` already exists", project_name),
        ));
    }
    fs::create_dir(path)?;
    init_at(path)?;
    Ok(path.to_path_buf())
}

pub fn init() -> io::Result<()> {
    init_at(Path::new("."))
}

fn init_at(path: &Path) -> io::Result<()> {
    if !path.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "`init` requires a directory",
        ));
    }
    let src_dir = path.join("src");
    fs::create_dir_all(&src_dir)?;

    let main_phi = src_dir.join("main.phi");
    let template = b"// Hello, Phi!\nfun main() {\n    println(\"Hello, world!\");\n}\n";
    fs::write(&main_phi, template)?;

    // `.` has no useful file name, so fall back to what the directory actually resolves to.
    let manifest_name = fs::canonicalize(path)?
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let manifest = format!(
        "[project]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2026\"\n",
        manifest_name
    );
    fs::write(path.join("Phi.toml"), manifest)?;

    println!("Created new Phi project at: {}", path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh empty directory under `target/`, named after the calling test.
    fn scratch(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-scratch")
            .join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("could not create the scratch directory");
        dir
    }

    #[test]
    fn a_new_project_has_a_manifest_and_an_entry_point() {
        let dir = scratch("new_project");
        init_at(&dir).expect("initializing an empty directory works");

        assert!(
            dir.join("Phi.toml").is_file(),
            "the manifest is capitalized"
        );
        // `.exists()` alone can't tell "Phi.toml" from "phi.toml" on a case-insensitive
        // filesystem (the macOS default), so check the actual directory listing instead.
        let names: Vec<String> = fs::read_dir(&dir)
            .expect("readable")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            names.iter().any(|n| n == "Phi.toml"),
            "the manifest is written with this exact case: {names:?}"
        );
        assert!(
            !names.iter().any(|n| n == "phi.toml"),
            "and not written lowercase: {names:?}"
        );
        assert!(dir.join("src/main.phi").is_file());

        let manifest = fs::read_to_string(dir.join("Phi.toml")).expect("readable");
        assert!(manifest.contains("name = \"new_project\""), "{manifest}");
        assert!(manifest.contains("version = \"0.1.0\""), "{manifest}");
    }

    /// The manifest a fresh project gets must be one `Config::load` accepts, or `phi new`
    /// would produce a project that `phi build` immediately rejects.
    #[test]
    fn a_new_project_is_loadable() {
        let dir = scratch("loadable_project");
        init_at(&dir).expect("initializing an empty directory works");

        let config = crate::driver::cli::Config::load(&dir).expect("the manifest parses");
        assert_eq!(config.name, "loadable_project");
        assert_eq!(config.src_dir, dir.join("src"));
    }

    #[test]
    fn new_refuses_to_overwrite() {
        let dir = scratch("refuses_overwrite");
        let existing = dir.join("taken");
        fs::create_dir(&existing).expect("could create it");

        let err = new(existing.to_str().expect("utf-8 path")).expect_err("already exists");
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
    }
}
