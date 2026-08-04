//! [`BuildOptions`], the flags `phi build` accepts, parsed once instead of re-derived per stage.

/// Which of the pipeline's intermediate results to print.
///
/// `--debug` sets every one of these, so each stage asks a single flag rather than re-deriving
/// `print_ast || debug` for itself.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Dumps {
    pub ast: bool,
    pub hir: bool,
    pub nameres: bool,
    pub typeck: bool,
}

/// Everything `phi build` was asked to do, beyond compiling.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct BuildOptions {
    pub dumps: Dumps,

    /// Drops the core library -- which is linked into every build -- out of the `--hir`,
    /// nameres, and typeck dumps. It has no effect otherwise; the AST dump always excludes the
    /// core library already.
    pub exclude_core: bool,
}

impl BuildOptions {
    /// The flags accepted after `build`.
    const KNOWN: [&'static str; 4] = ["--ast", "--hir", "--debug", "--no-core"];

    /// Parses the arguments following `build`, or explains what was wrong with them.
    ///
    /// The unknown-argument check lives here rather than at the call site so that the list of
    /// flags and the message naming them cannot drift apart.
    pub fn from_args(args: &[String]) -> Result<Self, String> {
        if let Some(unknown) = args.iter().find(|a| !Self::KNOWN.contains(&a.as_str())) {
            let known = Self::KNOWN.join("', '");
            return Err(format!(
                "unknown argument '{unknown}' after 'build' (only '{known}' are accepted)"
            ));
        }

        let has = |flag: &str| args.iter().any(|a| a == flag);
        // `--debug` dumps every stage, including the two that nothing downstream consumes yet.
        let debug = has("--debug");

        Ok(BuildOptions {
            dumps: Dumps {
                ast: debug || has("--ast"),
                hir: debug || has("--hir"),
                nameres: debug,
                typeck: debug,
            },
            exclude_core: has("--no-core"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(args: &[&str]) -> Result<BuildOptions, String> {
        let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        BuildOptions::from_args(&args)
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

    /// The one flag that implies the others, which is why each stage can ask a single question.
    #[test]
    fn debug_dumps_every_stage() {
        let dumps = opts(&["--debug"]).expect("--debug is valid").dumps;
        assert_eq!(
            dumps,
            Dumps {
                ast: true,
                hir: true,
                nameres: true,
                typeck: true
            }
        );
    }

    #[test]
    fn flags_combine() {
        let options = opts(&["--hir", "--no-core"]).expect("both flags are valid");
        assert!(options.dumps.hir);
        assert!(!options.dumps.ast);
        assert!(options.exclude_core);
    }

    #[test]
    fn an_unknown_flag_is_named_in_the_error() {
        let err = opts(&["--nope"]).expect_err("--nope is not a flag");
        assert!(err.contains("--nope"), "{err}");
        assert!(err.contains("--ast"), "the message lists what is accepted: {err}");
    }
}
