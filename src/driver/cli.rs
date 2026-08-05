use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::driver::{pipeline, project};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Dumps {
    pub ast: bool,
    pub hir: bool,
    pub mir: bool,
    pub llvm: bool,
    pub nameres: bool,
    pub typeck: bool,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct BuildOptions {
    pub dumps: Dumps,
    pub exclude_core_in_emit: bool,
}

impl BuildOptions {
    /// The flags accepted after `build` and `check`.
    const KNOWN: [&'static str; 6] = [
        "--ast",
        "--hir",
        "--mir",
        "--llvm",
        "--emit-debug",
        "--no-emit-core",
    ];

    pub fn from_args(args: &[String]) -> Result<Self, String> {
        if let Some(unknown) = args.iter().find(|a| !Self::KNOWN.contains(&a.as_str())) {
            let known = Self::KNOWN.join("', '");
            return Err(format!(
                "unknown argument '{unknown}' (only '{known}' are accepted)"
            ));
        }

        let has = |flag: &str| args.iter().any(|a| a == flag);
        let debug = has("--emit-debug");

        Ok(BuildOptions {
            dumps: Dumps {
                ast: debug || has("--ast"),
                hir: debug || has("--hir"),
                mir: has("--mir"),
                llvm: has("--llvm"),
                nameres: debug,
                typeck: debug,
            },
            exclude_core_in_emit: has("--no-emit-core"),
        })
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Debug,
    Release,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub name: String,
    pub version: String,
    pub edition: String,
    pub mode: Mode,
    /// The directory holding `Phi.toml`.
    pub root: PathBuf,
    /// `root/src` -- the only directory source files are collected from.
    pub src_dir: PathBuf,
}

#[derive(Deserialize)]
struct Manifest {
    project: ManifestProject,
    profile: Option<ManifestProfile>,
}

#[derive(Deserialize)]
struct ManifestProject {
    name: String,
    version: String,
    edition: String,
}

#[derive(Deserialize)]
struct ManifestProfile {
    mode: Option<String>,
}

impl Config {
    /// The manifest's file name, capitalized.
    pub const MANIFEST: &'static str = "Phi.toml";

    /// Reads and parses `cwd/Phi.toml`.
    pub fn load(cwd: &Path) -> Result<Config, String> {
        let path = cwd.join(Self::MANIFEST);
        let text = fs::read_to_string(&path)
            .map_err(|e| format!("could not read `{}`: {e}", path.display()))?;
        Self::from_str(&text, cwd).map_err(|e| format!("in `{}`: {e}", path.display()))
    }

    fn from_str(text: &str, root: &Path) -> Result<Config, String> {
        let manifest: Manifest = toml::from_str(text).map_err(|e| e.to_string())?;

        let mode = match manifest.profile.and_then(|p| p.mode).as_deref() {
            None | Some("debug") => Mode::Debug,
            Some("release") => Mode::Release,
            Some(other) => {
                return Err(format!(
                    "unknown profile mode '{other}' (expected \"debug\" or \"release\")"
                ));
            }
        };

        Ok(Config {
            name: manifest.project.name,
            version: manifest.project.version,
            edition: manifest.project.edition,
            mode,
            root: root.to_path_buf(),
            src_dir: root.join("src"),
        })
    }
}

/// Every command `phi` accepts
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliArgs {
    Build(BuildOptions),
    Check(BuildOptions),
    Run,
    New(String),
    Init,
    Help,
}

impl CliArgs {
    /// Parses the arguments following the program name.
    pub fn from_args(args: &[String]) -> Result<CliArgs, String> {
        let Some(command) = args.first() else {
            return Err("no command given".to_string());
        };
        let rest = &args[1..];

        /// Rejects trailing arguments for the commands that take none.
        fn no_args(command: &str, rest: &[String]) -> Result<(), String> {
            match rest.first() {
                None => Ok(()),
                Some(extra) => Err(format!("'{command}' accepts no arguments, got '{extra}'")),
            }
        }

        match command.as_str() {
            "build" => Ok(CliArgs::Build(BuildOptions::from_args(rest)?)),
            "check" => Ok(CliArgs::Check(BuildOptions::from_args(rest)?)),
            "run" => no_args("run", rest).map(|()| CliArgs::Run),
            "init" => no_args("init", rest).map(|()| CliArgs::Init),
            "new" => match rest {
                [name] => Ok(CliArgs::New(name.clone())),
                _ => Err("'new' requires exactly one argument (the project name)".to_string()),
            },
            "help" | "--help" | "-h" => Ok(CliArgs::Help),
            other => Err(format!("unknown command '{other}'")),
        }
    }
}

pub fn print_usage() {
    let prog = env::args().next().unwrap_or_else(|| "phi".into());
    eprintln!("Usage:");
    eprintln!("  {prog} build [--ast] [--hir] [--mir] [--llvm] [--emit-debug] [--no-emit-core]");
    eprintln!("  {prog} check [--ast] [--hir] [--mir] [--llvm] [--emit-debug] [--no-emit-core]");
    eprintln!("  {prog} run");
    eprintln!("  {prog} new <project_name>");
    eprintln!("  {prog} init");
    eprintln!("  {prog} help");
}

pub fn main(args: &[String]) -> i32 {
    let parsed = match CliArgs::from_args(args) {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("error: {message}");
            print_usage();
            return 1;
        }
    };

    match parsed {
        CliArgs::Help => {
            print_usage();
            0
        }
        CliArgs::New(name) => exit_code(project::new(&name).map(|_| true)),
        CliArgs::Init => exit_code(project::init().map(|()| true)),
        CliArgs::Build(options) => with_config(|config| pipeline::build(config, &options)),
        CliArgs::Check(options) => with_config(|config| pipeline::check(config, &options)),
        CliArgs::Run => with_config(pipeline::run),
    }
}

/// Turns a command's result into an exit code, printing any error.
fn exit_code(result: io::Result<bool>) -> i32 {
    match result {
        Ok(true) => 0,
        Ok(false) => 1,
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}

/// Loads the current directory's manifest and hands it to `command`.
fn with_config(command: impl FnOnce(&Config) -> io::Result<bool>) -> i32 {
    let config = match Config::load(Path::new(".")) {
        Ok(config) => config,
        Err(message) => {
            eprintln!("error: {message}");
            return 1;
        }
    };
    exit_code(command(&config))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(args: &[&str]) -> Vec<String> {
        args.iter().map(|a| a.to_string()).collect()
    }

    fn opts(flags: &[&str]) -> Result<BuildOptions, String> {
        BuildOptions::from_args(&args(flags))
    }

    #[test]
    fn no_arguments_dumps_nothing() {
        let options = opts(&[]).expect("no arguments is valid");
        assert_eq!(options, BuildOptions::default());
    }

    #[test]
    fn ast_dumps_only_the_ast() {
        let dumps = opts(&["--ast"]).expect("--ast is valid").dumps;
        assert_eq!(
            dumps,
            Dumps {
                ast: true,
                ..Dumps::default()
            }
        );
    }

    /// `--emit-debug` implies every stage that exists, which is why each stage can ask a
    /// single question. It leaves `mir` and `llvm` alone because those stages don't exist.
    #[test]
    fn emit_debug_dumps_every_implemented_stage() {
        let dumps = opts(&["--emit-debug"])
            .expect("--emit-debug is valid")
            .dumps;
        assert_eq!(
            dumps,
            Dumps {
                ast: true,
                hir: true,
                mir: false,
                llvm: false,
                nameres: true,
                typeck: true,
            }
        );
    }

    #[test]
    fn flags_combine() {
        let options = opts(&["--hir", "--no-emit-core"]).expect("both flags are valid");
        assert!(options.dumps.hir);
        assert!(!options.dumps.ast);
        assert!(options.exclude_core_in_emit);
    }

    #[test]
    fn an_unknown_flag_is_named_in_the_error() {
        let err = opts(&["--nope"]).expect_err("--nope is not a flag");
        assert!(err.contains("--nope"), "{err}");
        assert!(
            err.contains("--ast"),
            "the message lists what is accepted: {err}"
        );
    }

    #[test]
    fn the_unimplemented_stage_flags_parse() {
        let dumps = opts(&["--mir", "--llvm"]).expect("both are accepted").dumps;
        assert!(dumps.mir);
        assert!(dumps.llvm);
    }

    #[test]
    fn build_and_check_take_the_same_options() {
        let build = CliArgs::from_args(&args(&["build", "--ast"])).expect("valid");
        let check = CliArgs::from_args(&args(&["check", "--ast"])).expect("valid");
        match (build, check) {
            (CliArgs::Build(b), CliArgs::Check(c)) => assert_eq!(b, c),
            other => panic!("expected Build and Check, got {other:?}"),
        }
    }

    #[test]
    fn new_requires_exactly_one_name() {
        assert_eq!(
            CliArgs::from_args(&args(&["new", "demo"])).expect("valid"),
            CliArgs::New("demo".to_string())
        );
        CliArgs::from_args(&args(&["new"])).expect_err("no name given");
        CliArgs::from_args(&args(&["new", "a", "b"])).expect_err("two names given");
    }

    #[test]
    fn init_and_run_reject_arguments() {
        assert_eq!(
            CliArgs::from_args(&args(&["init"])).expect("valid"),
            CliArgs::Init
        );
        assert_eq!(
            CliArgs::from_args(&args(&["run"])).expect("valid"),
            CliArgs::Run
        );
        let err = CliArgs::from_args(&args(&["init", "somewhere"])).expect_err("takes none");
        assert!(err.contains("somewhere"), "{err}");
    }

    #[test]
    fn help_has_three_spellings() {
        for spelling in ["help", "--help", "-h"] {
            assert_eq!(
                CliArgs::from_args(&args(&[spelling])).expect("valid"),
                CliArgs::Help
            );
        }
    }

    #[test]
    fn no_command_is_an_error() {
        CliArgs::from_args(&[]).expect_err("a command is required");
    }

    #[test]
    fn an_unknown_command_is_named_in_the_error() {
        let err = CliArgs::from_args(&args(&["frobnicate"])).expect_err("not a command");
        assert!(err.contains("frobnicate"), "{err}");
    }

    #[test]
    fn a_manifest_without_a_profile_defaults_to_debug() {
        let config = Config::from_str(
            "[project]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2026\"\n",
            Path::new("/tmp/demo"),
        )
        .expect("a manifest with only [project] is valid");
        assert_eq!(config.name, "demo");
        assert_eq!(config.version, "0.1.0");
        assert_eq!(config.edition, "2026");
        assert_eq!(config.mode, Mode::Debug);
        assert_eq!(config.src_dir, Path::new("/tmp/demo/src"));
    }

    #[test]
    fn a_profile_selects_the_mode() {
        let config = Config::from_str(
            "[project]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2026\"\n\
             [profile]\nmode = \"release\"\n",
            Path::new("."),
        )
        .expect("a release profile is valid");
        assert_eq!(config.mode, Mode::Release);
    }

    #[test]
    fn an_unknown_mode_is_named_in_the_error() {
        let err = Config::from_str(
            "[project]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2026\"\n\
             [profile]\nmode = \"turbo\"\n",
            Path::new("."),
        )
        .expect_err("turbo is not a mode");
        assert!(err.contains("turbo"), "{err}");
    }

    #[test]
    fn a_manifest_missing_the_project_table_is_an_error() {
        Config::from_str("[profile]\nmode = \"debug\"\n", Path::new("."))
            .expect_err("[project] is required");
    }
}
