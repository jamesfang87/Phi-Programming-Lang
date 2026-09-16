//! End-to-end tests written to expose bugs in the compiler.
//!
//! Each test compiles (and, where relevant, runs) a real Phi program through the actual `phi`
//! binary, the same way `tests/cli.rs` does. The tests fall into two groups:
//!
//! * **Regression tests** (not ignored): behavior that is already correct. They guard against
//!   the bugs below ever being "fixed" in a way that breaks something else.
//! * **Bug tests** (`#[ignore = "BUG: ..."]`): the program's behavior today is wrong. They are
//!   ignored so the default `cargo test` run stays green, but they are fully runnable and
//!   reproduce a real defect:
//!
//!   ```text
//!   cargo test --test bugs -- --ignored
//!   ```
//!
//!   A bug test asserts the *correct* behavior. Once the underlying defect is fixed, remove its
//!   `#[ignore]` so it becomes an ordinary regression test. Do not "fix" a bug test by asserting
//!   the current, wrong behavior.
//!
//! Several tests below check that the compiler reports an error rather than crashing (a
//! `panicked at ...` line on stderr / exit status 101). A compiler crash on pathological input is
//! always a bug, independent of whether the input is itself valid.

mod support;

use std::path::{Path, PathBuf};
use support::Run;

fn run(dir: &Path, args: &[&str]) -> Run {
    support::exec(dir, args)
}

fn stderr(run: &Run) -> String {
    run.stderr.clone()
}

fn stdout(run: &Run) -> String {
    run.stdout.clone()
}

fn code(run: &Run) -> i32 {
    run.code
}

/// Sets up a scratch project named `name` whose `src/main.phi` is `source`.
fn project(name: &str, source: &str) -> PathBuf {
    support::project("bugs", name, source)
}

/// Asserts the compiler processed the input without an internal panic. A Rust panic exits 101 and
/// prints `panicked at`; a clean diagnostic exits 1.
fn assert_no_compiler_panic(run: &Run, context: &str) {
    support::expect_no_panic(run, context);
}

// ---------------------------------------------------------------------------
// Regression tests: behavior that is already correct.
// ---------------------------------------------------------------------------

#[test]
fn division_by_zero_aborts_with_a_diagnostic() {
    let dir = project(
        "division_by_zero",
        "module app;\n\n\
         fun main() {\n    \
             let a: i32 = 1;\n    \
             let b: i32 = 0;\n    \
             let c = a / b;\n    \
             if c == 0 { core::io::write_bytes(1, \"unreachable\" as &[u8]); }\n\
         }\n",
    );
    let output = run(&dir, &["run"]);
    assert_ne!(code(&output), 0, "dividing by zero must abort");
    assert!(
        stderr(&output).contains("divide by zero"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn nan_is_not_equal_to_itself() {
    // IEEE semantics: `==` on a NaN is false. This one already works; the `!=` direction (see
    // `nan_is_not_equal_itself_via_not_equal` below) does not.
    let dir = project(
        "nan_eq",
        "module app;\n\n\
         fun main() {\n    \
             let z: f64 = 0.0;\n    \
             let nan = z / z;\n    \
             if nan == nan {\n        \
                 core::io::write_bytes(1, \"eq\" as &[u8]);\n    \
             } else {\n        \
                 core::io::write_bytes(1, \"neq\" as &[u8]);\n    \
             }\n\
         }\n",
    );
    let output = run(&dir, &["run"]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "neq");
}

#[test]
fn out_of_bounds_literal_index_still_resolves_in_bounds_reads() {
    // A literal index in bounds is lowered to `ConstantIndex` and reads the element correctly.
    let dir = project(
        "index_literal_in_bounds",
        "module app;\n\n\
         fun main() {\n    \
             let a = new [7; 3];\n    \
             let x = (*a)[1];\n    \
             if x == 7 {\n        \
                 core::io::write_bytes(1, \"ok\" as &[u8]);\n    \
             } else {\n        \
                 core::io::write_bytes(1, \"bad\" as &[u8]);\n    \
             }\n\
         }\n",
    );
    let output = run(&dir, &["run"]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "ok");
}

#[test]
fn a_copy_bound_allows_repeated_reads_of_a_generic() {
    // A `T: Copy` parameter was classified by shape alone, so reading `x` twice lowered the
    // second read to a move and borrowck rejected the program with "use of moved value".
    let dir = project(
        "generic_copy_bound",
        "module app;\n\n\
         import core::ops::Copy;\n\n\
         fun duplicate<T: Copy>(x: T) -> T {\n    \
             let a = x;\n    \
             let b = x;\n    \
             return a;\n\
         }\n\n\
         fun main() {\n    \
             if duplicate(7) == 7 { core::io::write_bytes(1, \"ok\" as &[u8]); }\n\
         }\n",
    );
    let output = run(&dir, &["run"]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "ok");
}

#[test]
fn signed_remainder_truncates_toward_zero() {
    let dir = project(
        "remainder_truncates",
        "module app;\n\n\
         fun main() {\n    \
             let a: i32 = 0 - 7;\n    \
             let b: i32 = 3;\n    \
             if a % b == 0 - 1 { core::io::write_bytes(1, \"ok\" as &[u8]); }\n    \
             else { core::io::write_bytes(1, \"bad\" as &[u8]); }\n\
         }\n",
    );
    let output = run(&dir, &["run"]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "ok");
}

// ---------------------------------------------------------------------------
// Bug tests
// ---------------------------------------------------------------------------

/// BUG: `Literal::string`/`Literal::char` slice `token_text[1..len - 1]`. An unterminated literal
/// token that is only the opening quote has `len == 1`, so the slice is `1..0` and panics. The
/// lexer already reports "unterminated ...", so the compiler should recover, not crash.
#[test]
fn unterminated_string_literal_does_not_crash_the_compiler() {
    // The file must end exactly at the opening quote, with no trailing newline.
    let dir = project("unterminated_string", "fun main() { let x = \"");
    let output = run(&dir, &["check"]);
    assert_no_compiler_panic(&output, "unterminated string literal");
    assert_ne!(code(&output), 0);
    assert!(
        stderr(&output).contains("unterminated string"),
        "{}",
        stderr(&output)
    );
}

/// BUG: same slice panic as the string case, via `Literal::char`.
#[test]
fn unterminated_char_literal_does_not_crash_the_compiler() {
    let dir = project("unterminated_char", "fun main() { let x = '");
    let output = run(&dir, &["check"]);
    assert_no_compiler_panic(&output, "unterminated char literal");
    assert_ne!(code(&output), 0);
    assert!(
        stderr(&output).contains("unterminated character"),
        "{}",
        stderr(&output)
    );
}

/// BUG: a whole-number literal with a float suffix (`5_f64`) is typed as `f64` by `check_literal`
/// but lowered through the `Literal::Int` path, so codegen tries to make an integer constant of a
/// float type and panics ("ConstKind::Int has a non-integer primitive F64"). The type checker
/// deliberately allows this form, so codegen must emit `5.0`.
#[test]
fn integer_literal_with_a_float_suffix_compiles() {
    let dir = project(
        "int_float_suffix",
        "module app;\n\nfun main() { let y: f64 = 5_f64; }\n",
    );
    let output = run(&dir, &["build"]);
    assert_no_compiler_panic(&output, "5_f64");
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
}

/// BUG: `extend X with X { }` reports "a type cannot extend itself" in name resolution but
/// records no trait-path resolution; HIR lowering then unconditionally lowers that path and
/// panics with "owns no recorded resolution". The diagnostic should be reported, compilation
/// should stop, and the compiler should not crash.
#[test]
fn a_self_extend_is_reported_not_a_crash() {
    let dir = project(
        "self_extend",
        "module app;\n\nstruct V {}\nextend V with V {}\n\nfun main() {}\n",
    );
    let output = run(&dir, &["check"]);
    assert_no_compiler_panic(&output, "self-extend");
    assert_ne!(code(&output), 0);
}

/// BUG: a bound may name a generic parameter declared later (`T: Conv<U>, U`). Name resolution
/// accepts it, but HIR lowering translates the reference to `U` before `U` has been lowered and
/// panics with "expected to already have a HirId as a generic parameter".
#[test]
fn a_forward_reference_in_a_bound_is_reported_not_a_crash() {
    let dir = project(
        "forward_bound",
        "module app;\n\ntrait Conv<X> {}\nfun f<T: Conv<U>, U>() {}\n\nfun main() {}\n",
    );
    let output = run(&dir, &["check"]);
    assert_no_compiler_panic(&output, "forward bound reference");
    assert_ne!(code(&output), 0);
}

/// BUG: `resolve_bounds` deduplicates bounds by their path alone, ignoring type arguments. So
/// `Conv<i32> + Conv<u32>` is misreported as a duplicate, the second bound gets no resolution,
/// and HIR lowering panics. Distinct argument lists are distinct bounds and must both resolve.
#[test]
fn duplicate_paths_with_different_arguments_are_valid_bounds() {
    let dir = project(
        "dup_bound_args",
        "module app;\n\ntrait Conv<X> {}\nfun f<T: Conv<i32> + Conv<u32>>() {}\n\nfun main() {}\n",
    );
    let output = run(&dir, &["check"]);
    assert_no_compiler_panic(&output, "bounds with distinct arguments");
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
}

/// BUG: integer literals are never range-checked, so a value that does not fit its type wraps.
/// `300` in a `u8` becomes `44`; `200` in an `i8` becomes `-56`. Codegen's `const_int(v as u64)`
/// silently truncates. A literal that overflows its type must be a compile error.
#[test]
fn an_out_of_range_integer_literal_is_rejected() {
    for (name, src) in [
        ("u8_300", "module app;\nfun main() { let x: u8 = 300; }\n"),
        ("i8_200", "module app;\nfun main() { let x: i8 = 200; }\n"),
        (
            "i32_2pow32",
            "module app;\nfun main() { let x: i32 = 4294967296; }\n",
        ),
    ] {
        let dir = project(name, src);
        let output = run(&dir, &["check"]);
        assert_ne!(
            code(&output),
            0,
            "{name}: a literal out of range for its type must be rejected, not wrapped"
        );
    }
}

/// BUG: `-1` is accepted for an unsigned type and wraps to the type's maximum. Unary minus on an
/// *undefaulted* numeric literal is admitted without ever checking that the eventual type
/// implements `Neg` (there is no `extend u8 with Neg`), and then the literal is truncated.
#[test]
fn a_negative_literal_for_an_unsigned_type_is_rejected() {
    let dir = project(
        "unsigned_negative",
        "module app;\nfun main() { let x: u8 = -1; }\n",
    );
    let output = run(&dir, &["check"]);
    assert_ne!(code(&output), 0, "`-1` does not fit in `u8`");
}

/// BUG: a literal index is parsed straight to `u32` in MIR lowering and panics on failure.
/// Since literals are otherwise parsed as `i128` and never range-checked, an index above
/// `u32::MAX` reaches that conversion and crashes the compiler. It should be a diagnostic.
#[test]
fn a_huge_constant_array_index_does_not_crash_the_compiler() {
    let dir = project(
        "huge_index",
        "module app;\n\n\
         fun main() {\n    \
             let a = new [1; 3];\n    \
             let x = (*a)[9999999999999];\n\
         }\n",
    );
    let output = run(&dir, &["build"]);
    assert_no_compiler_panic(&output, "huge constant index");
    assert_ne!(code(&output), 0);
}

/// BUG: an integer literal too large for `i128` also panics in MIR lowering (`.parse::<i128>()
/// .unwrap_or_else(|_| panic!(...))`) rather than being reported as out of range.
#[test]
fn a_huge_integer_literal_does_not_crash_the_compiler() {
    let dir = project(
        "huge_literal",
        "module app;\n\nfun main() { let x = 99999999999999999999999999999999999999999999999999; }\n",
    );
    let output = run(&dir, &["build"]);
    assert_no_compiler_panic(&output, "huge integer literal");
    assert_ne!(code(&output), 0);
}

/// BUG: indexing an array by a *variable* whose type is `i32` (the default integer type!)
/// corrupts memory. MIR lowering sizes the index and length temporaries with the index's own
/// type (`i32`), but codegen's `Projection::Index` unconditionally loads/stores `i64`, so a
/// 64-bit load reads past the 4-byte slot. The program below segfaults instead of printing `ok`.
#[test]
fn array_index_by_an_i32_variable_does_not_corrupt_memory() {
    let dir = project(
        "index_i32_variable",
        "module app;\n\n\
         fun main() {\n    \
             let a = new [7; 3];\n    \
             let i: i32 = 1;\n    \
             let x = (*a)[i];\n    \
             if x == 7 {\n        \
                 core::io::write_bytes(1, \"ok\" as &[u8]);\n    \
             } else {\n        \
                 core::io::write_bytes(1, \"bad\" as &[u8]);\n    \
             }\n\
         }\n",
    );
    let output = run(&dir, &["run"]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "ok");
}

/// BUG: `lower_index_place` builds the bounds-check `Assert` with a condition that is the
/// constant `true`, never `index < len`. The failure branch (which would abort with "index out of
/// bounds") is therefore dead, and out-of-bounds reads execute unimpeded. This reads index `10`
/// of a 3-element array; the compiler must abort, not return heap garbage.
#[test]
fn an_out_of_bounds_array_read_aborts() {
    let dir = project(
        "oob_read",
        "module app;\n\n\
         fun main() {\n    \
             let a = new [7; 3];\n    \
             let i: i64 = 10;\n    \
             let x = (*a)[i];\n    \
             if x == 0 { core::io::write_bytes(1, \"zero\" as &[u8]); }\n    \
             else { core::io::write_bytes(1, \"oob-read\" as &[u8]); }\n\
         }\n",
    );
    let output = run(&dir, &["run"]);
    assert_ne!(
        code(&output),
        0,
        "reading past the end of an array must abort; got stdout {:?}",
        stdout(&output)
    );
    assert!(
        stderr(&output).contains("bounds"),
        "the abort should name the bounds violation: {}",
        stderr(&output)
    );
}

/// BUG: floating-point `!=` is lowered to `fcmp one` (ordered-not-equal) instead of `fcmp une`
/// (unordered-not-equal). `ONE` is false when either operand is NaN, so `nan != nan` evaluates
/// to `false`, contradicting IEEE 754 and the `==` direction, which is already correct.
#[test]
fn nan_is_not_equal_itself_via_not_equal() {
    let dir = project(
        "nan_ne",
        "module app;\n\n\
         fun main() {\n    \
             let z: f64 = 0.0;\n    \
             let nan = z / z;\n    \
             if nan != nan {\n        \
                 core::io::write_bytes(1, \"neq\" as &[u8]);\n    \
             } else {\n        \
                 core::io::write_bytes(1, \"eq\" as &[u8]);\n    \
             }\n\
         }\n",
    );
    let output = run(&dir, &["run"]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert_eq!(
        stdout(&output),
        "neq",
        "NaN is not equal to itself, so `nan != nan` must be true"
    );
}

/// BUG: the `&mut self` receiver check only looks at the outermost projection layer. It accepts
/// a call that reborrows mutably through a *shared* reference (`&mut &S`), which MIR lowering
/// then performs anyway. This must be a borrow error.
#[test]
fn a_mutable_method_cannot_be_called_through_a_shared_reference() {
    let dir = project(
        "mut_through_shared",
        "module app;\n\n\
         struct S { x: i32 }\n\
         extend S { fun set(&mut self, v: i32) { self.x = v; } }\n\n\
         fun f(r: &S) { let m: &mut &S = &mut r; m.set(1); }\n\n\
         fun main() {}\n",
    );
    let output = run(&dir, &["check"]);
    assert_ne!(
        code(&output),
        0,
        "borrowing `**m` mutably through a shared reference must be rejected"
    );
}

/// BUG: `is_place_expr` treats `base.member` as writable whenever `base` is not a type, even
/// when `base` is a call result. Assigning to a field of a temporary should be rejected (there is
/// no place to write); today it is accepted and the store is discarded.
#[test]
fn assigning_to_a_temporary_is_rejected() {
    let dir = project(
        "assign_temporary",
        "module app;\n\n\
         struct P { x: i32 }\n\
         fun make() -> P { return P { x: 1 }; }\n\
         fun f() { make().x = 2; }\n\n\
         fun main() {}\n",
    );
    let output = run(&dir, &["check"]);
    assert_ne!(code(&output), 0, "a temporary has no assignable place");
}

/// BUG: match exhaustiveness only checks that each top-level variant name appears in some arm.
/// It never verifies the arms cover the variant's *payload*, so a match that only handles
/// `.some(true)` is accepted even though `.some(false)` is unhandled. MIR lowering sends the
/// uncovered case to `unreachable`, which is undefined behavior at runtime.
#[test]
fn a_match_missing_a_variant_payload_case_is_rejected() {
    let dir = project(
        "match_payload_partial",
        "module app;\n\n\
         enum Opt { some: bool, none }\n\n\
         fun f(o: Opt) -> i32 {\n    \
             return match o {\n        \
                 .some(true) => 1,\n        \
                 .none => 3,\n    \
             };\n\
         }\n\n\
         fun main() {}\n",
    );
    let output = run(&dir, &["check"]);
    assert_ne!(
        code(&output),
        0,
        "`.some(false)` is not covered, so this match is not exhaustive"
    );
}

/// BUG: `pat_is_irrefutable` declares a single-variant enum pattern irrefutable without
/// recursing into its payload. `let .one(true) = o;` is therefore accepted even though it fails
/// for `.one(false)`, and the literal test is discarded during lowering, so `.one(false)`
/// silently takes the "matched" path.
#[test]
fn a_refutable_let_pattern_is_rejected() {
    let dir = project(
        "refutable_let",
        "module app;\n\n\
         enum Only { one: bool }\n\n\
         fun f(o: Only) { let .one(true) = o; }\n\n\
         fun main() {}\n",
    );
    let output = run(&dir, &["check"]);
    assert_ne!(
        code(&output),
        0,
        "`.one(true)` does not match `.one(false)`, so this `let` needs an `else`"
    );
}

/// BUG: name resolution inserts `if let` pattern bindings into the enclosing scope instead of
/// the branch's own scope. Using the binding after the `if let` therefore resolves (to a binding
/// that only exists on the taken branch) and fails with a confusing "use of moved value" instead
/// of "cannot find `x`".
#[test]
fn an_if_let_binding_does_not_escape_its_branch() {
    let dir = project(
        "if_let_leak",
        "module app;\n\n\
         enum E { v: i32 }\n\n\
         fun f(e: E) -> i32 {\n    \
             if let .v(x) = e {\n        \
                 return x;\n    \
             }\n    \
             return x;\n\
         }\n\n\
         fun main() {}\n",
    );
    let output = run(&dir, &["check"]);
    assert_ne!(code(&output), 0);
    assert!(
        stderr(&output).contains("cannot find"),
        "the trailing `x` is out of scope and should be reported as such: {}",
        stderr(&output)
    );
}

/// BUG: the type parser always builds a tuple for parenthesized types, so `(i32)` becomes the
/// one-element tuple `(i32,)`. The grammar's own tests and diagnostic renderer state that only a
/// trailing comma (`(T,)`) makes a one-element tuple; `(T)` should be `T`.
#[test]
fn a_parenthesized_type_is_not_a_one_element_tuple() {
    let dir = project(
        "paren_type",
        "module app;\n\nfun f(x: (i32)) -> i32 { return x; }\n\nfun main() {}\n",
    );
    let output = run(&dir, &["check"]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
}

/// BUG: the pattern parser has the same one-element-tuple confusion as the type parser, so
/// `let (x) = 1;` is parsed as a tuple pattern `(x,)` against an `i32` and rejected. Only `(x,)`
/// should be a one-element tuple pattern.
#[test]
fn a_parenthesized_pattern_is_not_a_one_element_tuple() {
    let dir = project(
        "paren_pattern",
        "module app;\n\n\
         fun main() {\n    \
             let (x) = 1;\n    \
             if x == 1 { core::io::write_bytes(1, \"ok\" as &[u8]); }\n\
         }\n",
    );
    let output = run(&dir, &["run"]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "ok");
}

/// BUG: `cast_allowed` omits `usize` from the integer targets of `char`, even though `usize` is
/// treated as 64-bit everywhere else (including `int_width`) and every `char` codepoint fits.
/// `char as u64` is allowed; `char as usize` must be too.
#[test]
fn a_char_can_be_cast_to_usize() {
    let dir = project(
        "char_usize",
        "module app;\n\nfun f(c: char) -> usize { return c as usize; }\n\nfun main() {}\n",
    );
    let output = run(&dir, &["check"]);
    assert_eq!(
        code(&output),
        0,
        "`char` always fits in a 64-bit `usize`: {}",
        stderr(&output)
    );
}
