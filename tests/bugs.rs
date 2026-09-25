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

fn project(name: &str, source: &str) -> PathBuf {
    support::project("bugs", name, source)
}

fn assert_no_compiler_panic(run: &Run, context: &str) {
    support::expect_no_panic(run, context);
}

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

#[test]
fn unterminated_string_literal_does_not_crash_the_compiler() {
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

#[test]
fn a_negative_literal_for_an_unsigned_type_is_rejected() {
    let dir = project(
        "unsigned_negative",
        "module app;\nfun main() { let x: u8 = -1; }\n",
    );
    let output = run(&dir, &["check"]);
    assert_ne!(code(&output), 0, "`-1` does not fit in `u8`");
}

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

#[test]
fn a_parenthesized_type_is_not_a_one_element_tuple() {
    let dir = project(
        "paren_type",
        "module app;\n\nfun f(x: (i32)) -> i32 { return x; }\n\nfun main() {}\n",
    );
    let output = run(&dir, &["check"]);
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
}

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

#[test]
fn writing_through_a_shared_reference_is_rejected() {
    let cases = [
        "module app;\nfun main() { let x = 1; let r: &i32 = &x; *r = 2; }\n",
        "module app;\nstruct S { x: i32 }\nfun f(s: &S) { s.x = 1; }\nfun main() {}\n",
        "module app;\nstruct S { x: i32 }\nextend S { fun set(&self, v: i32) { self.x = v; } }\nfun main() {}\n",
        "module app;\nstruct S { x: i32 }\nextend S { fun bad(&self) -> &mut i32 { return &mut self.x; } }\nfun main() {}\n",
    ];
    for (idx, src) in cases.iter().enumerate() {
        let dir = project(&format!("shared_write_{idx}"), src);
        let output = run(&dir, &["check"]);
        assert_no_compiler_panic(&output, "write through a shared reference");
        assert_ne!(code(&output), 0, "case {idx} must be rejected: {src}");
        assert!(
            stderr(&output).contains("shared reference"),
            "case {idx}: {}",
            stderr(&output)
        );
    }
}

#[test]
fn signed_division_and_remainder_overflow_abort() {
    let cases = [
        "module app;\nfun overflow(a: i32, b: i32) -> i32 { return a / b; }\n\
         fun main() { let a: i32 = 0 - 2147483647 - 1; let b: i32 = 0 - 1; let c = overflow(a, b); \
         if c == 0 { core::io::write_bytes(1, \"x\" as &[u8]); } }\n",
        "module app;\nfun overflow(a: i32, b: i32) -> i32 { return a % b; }\n\
         fun main() { let a: i32 = 0 - 2147483647 - 1; let b: i32 = 0 - 1; let c = overflow(a, b); \
         if c == 0 { core::io::write_bytes(1, \"x\" as &[u8]); } }\n",
    ];
    for (idx, src) in cases.iter().enumerate() {
        let dir = project(&format!("div_overflow_{idx}"), src);
        let output = run(&dir, &["run"]);
        assert_ne!(code(&output), 0, "case {idx} must abort: {src}");
        assert!(
            stderr(&output).contains("overflow"),
            "case {idx}: {}",
            stderr(&output)
        );
    }
}

#[test]
fn returning_a_reference_to_a_local_is_rejected() {
    let cases = [
        "fun f() -> &i32 { let x = 5; return &x; }",
        "fun f() -> &i32 { let x = 5; let r = &x; return r; }",
        "fun f() -> &i32 { let x = 5; let r = &x; return &*r; }",
        "fun f(c: bool, p: &i32) -> &i32 { if c { let x = 5; return &x; } else { return p; } }",
    ];
    for (idx, function) in cases.iter().enumerate() {
        let source = format!("module app;\n\n{function}\n\nfun main() {{}}\n");
        let dir = project(&format!("return_local_ref_{idx}"), &source);
        let output = run(&dir, &["check"]);
        assert_no_compiler_panic(&output, "return a reference to a local");
        assert_ne!(code(&output), 0, "case {idx} must be rejected: {function}");
    }
}

#[test]
fn returning_a_call_result_that_projects_a_local_argument_is_rejected() {
    let dir = project(
        "return_call_local_ref",
        "module app;\n\nfun foo(x: &i32) -> &i32 { return x; }\n\
         fun bar() -> &i32 { let local = 5; return foo(&local); }\n\nfun main() {}\n",
    );
    let output = run(&dir, &["check"]);
    assert_no_compiler_panic(&output, "return a call result projecting a local");
    assert_ne!(
        code(&output),
        0,
        "the returned reference points into `bar`'s frame"
    );
}
