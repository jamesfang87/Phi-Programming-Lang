# Compiler Architecture
Broadly, the compiler's architecture can be described as such: 

```mermaid
flowchart LR
    subgraph FrontEnd[Front End - Analysis]
        A[Source Code] --> B[Lexer]
        B --> C[Tokens]
        C --> D[Parser]
        D --> E[Abstract Syntax Tree]
        E --> F[Name Resolution]
        F --> G[Abstract Syntax Tree<br>& Name Resolution Results]
        G --> H[Lowering #1]
        H --> I[High Intermediate Rep]
        I --> J[Type Checking]
        J --> K[High Intermediate Rep<br>& Type Checking Results]
        K --> L[Lowering #2]
        L --> M[Middle Intermediate Rep]
        M --> N[Borrow Checking]
    end

    subgraph BackEnd[Back End - Synthesis]
        N --> O[Optimizer]
        O --> P[Optimized IR]
        P --> Q[Lowering #3]
        Q --> R[LLVM IR]
        R --> S[Machine Code]
    end
    
    EH[Error Handling] -.-> B
    EH -.-> D
    EH -.-> F
    EH -.-> J
    EH -.-> N
```
In the future, there are plans to move to incremental compilation, as well as to parallelize the lexer, parser, and codegen.

## Driver and CLI
Phi supports the following CLI:
```
phi build --ast --hir --mir --llvm --emit-debug --no-emit-core
phi check --ast --hir --mir --llvm --emit-debug --no-emit-core
phi run
phi new project-name
phi init
phi --help
```

Every project is described by a `Phi.toml` manifest at its root, with sources collected only
from that project's `src/` directory. The manifest has a required `[project]` table with
`name`, `version`, and `edition`, and an optional `[profile]` table with `mode = "debug" |
"release"`, defaulting to `debug` when the table (or the key) is absent.

```toml
[project]
name = "example"
version = "0.1.0"
edition = "2026"

[profile]
mode = "debug"
```

The overall driver architecture is the following:
```
driver/
├── cli.rs                 # CLI and Phi.toml parsing, and dispatch
├── source.rs              # SrcSpan, SrcFile, SrcMap, SrcCollector
├── project.rs             # Project creation: `phi new` and `phi init`
├── pipeline.rs            # The compiler stages (lexer -> parser -> ...)
└── emit_debug.rs          # Human-readable dumps of each stage's output
```

The CLI and Phi.toml parsing are implemented by the `driver::cli` module. The code inside `driver::cli` collects the args given into two constructs:
1. `CliArgs`, an enum which contains all possible commands and tracks what options were given for the current command.
2. `Config`, a struct read from the project's `Phi.toml`, holding project name/version/edition, the compilation mode (release/debug), and the project's root and `src/` directories.
Based on what args are given, `driver::cli` either dispatches to the `driver::project` module, which handles project creation, or to the `driver::pipeline` module, which handles the actual compilation.

The public API in `driver::project` and `driver::pipeline` mirrors the CLI. In `project.rs`, there is `pub fn init()` and `pub fn new(project_name: &str)`, which mirror the two commands in the CLI with the same name. In `pipeline.rs`, there is `pub fn check(config: &Config, options: &BuildOptions)`, `pub fn build(config: &Config, options: &BuildOptions)`, and `pub fn run(config: &Config)`. `Config` and `BuildOptions` are kept as separate arguments rather than merged into one struct: `Config` carries the manifest -- what project this is -- while `BuildOptions` carries the flags given to the specific `build` or `check` invocation, and those two things vary independently.

`build` and `check` accept the same flags and differ only in that `build` additionally prints a note that code generation is not implemented yet and that `build` currently only checks; `run` builds first, then reports that there is no backend to run and exits with status 1. `--mir` and `--llvm` are both accepted by `build` and `check`, but since neither stage exists yet, passing either just prints a note that the stage is not implemented and has no other effect. `--emit-debug` dumps every stage that is actually implemented, which includes the `NameResolutions` and `TypeResolutions` dumps even though those have no flag of their own to request them individually; `--no-emit-core` never affects compilation itself, only whether the core library's definitions show up in those dumps.

The remaining module, `driver::source`, contains `SrcSpan`, `SrcFile`, `SrcMap`, and `SrcCollector`, which are used to track source files and source file contents, and to collect them from the project's `src/` directory.

`SrcSpan` represents a half-open span of character offsets in the source code. It is a pure data class and does essentially nothing else but hold these two indices. These offsets are global across all source files which removes the requirement to store a pointer/reference to the file inside `SrcSpan`. Meanwhile, `SrcFile` tracks the contents of a single file and some metadata about it. This is essentially the structure of `SrcFile`.

```rust
pub struct SrcFile {
    pub name: String,
    pub content: Vec<char>,
    /// Whether the file is a part of lib/core (which holds implementations of the core of the stl) or a user file
    pub origin: FileOrigin,
    /// The offset of this file's first char within the whole `SrcMap`'s global address space.
    pub global_offset: usize,
    /// The global offset at which each line of this file starts.
    pub line_starts: Vec<usize>,
}
```

Meanwhile, `SrcMap` keeps track of all `SrcFile`s in the project and allows us to query some information about the contents of each file. See the code implementation for what queries we can make. Files are added into `SrcMap` through the `SrcCollector` class which walks through the current repository searching for `.phi` files. 

## Lexer
The first stage in the pipeline is lexing, which converts the raw text of a `SrcFile` into `Token`s. The structure of the `lexer` module is the following:
```
src/
├── lexer.rs                 # Handles CLI parsing (clap/structopt)
└── lexer/
    └── token.rs            # The actual compiler stages (lexer -> parser -> ...)
```
`token.rs` holds the `Token` struct which comprises of two things: a `TokenKind` enum storing the type of token it is and a `SrcSpan` storing the token's location.

```rust
pub struct Token {
    pub kind: TokenKind,
    pub span: SrcSpan,
}
```

`lexer.rs` holds the actual implementation for the `Lexer`. The `Lexer` is a lightweight struct which operates over reference to the contents of a file. It exposes two functions as a part of its public-facing interface: `new` (which creates a new `Lexer` for a file) and `tokenize` (which lexes the file to produce a `Token` stream). Since one instance of `Lexer` only operates for one file, a new instance must be created for each file.

```rust
pub struct Lexer<'a> {
    /// [`Lexer::src`] is a &Vec<char> represent the raw source code.
    src: &'a Vec<char>,

    /// [`Lexer::file_offset`] allows the [`Lexer`] to produce global
    /// [`SrcSpan`]s for the [`Token`] stream it outputs.
    file_offset: usize,

    /// [`Lexer::cursor`] is the current position in [`Lexer::src`].
    cursor: usize,

    /// [`Lexer::lexeme_pos`] is position that the current lexeme (lexical unit)
    /// starts at in [`Lexer::src`].
    /// This is required for generating spans for multi-character tokens as
    /// [`Lexer::cursor`] is not the start of the token anymore.
    lexeme_pos: usize,
}

/// Public facing methods
pub fn new(src_text: &'a Vec<char>, file_offset: usize) -> Lexer<'a>;
pub fn tokenize(&mut self) -> Vec<Token>;
```
