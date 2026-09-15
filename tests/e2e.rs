//! End-to-end tests for the Phi compiler.
//!
//! Every test builds and runs (or type-checks) a real `.phi` project through the actual
//! `phi` binary, then asserts on what the program *did*: the bytes it wrote to stdout, its
//! exit status, or the diagnostic it produced. These are deliberately coarse-grained
//! integration tests -- they exercise the whole pipeline rather than one stage -- and are
//! complemented by `tests/e2e_generated.rs`, which generates far larger programs whose
//! expected values are computed independently in Rust.
//!
//! Programs use `core::io::write_bytes(1, ...)` as their observable output, since the
//! language has no `print`/`println` yet.

mod support;

/// Builds and runs `source`, asserting it exits 0 and wrote exactly `expected` to stdout.
fn run(name: &str, source: &str, expected: &str) {
    let output = support::run_src("e2e", name, source);
    support::expect_stdout(&output, expected, name);
}

/// Builds and runs `source`, asserting only that it exited 0.
fn runs_ok(name: &str, source: &str) {
    let output = support::run_src("e2e", name, source);
    support::expect_ok(&output, name);
}

/// Type-checks `source`, asserting the compilation fails with `needle` somewhere on stderr.
fn rejects(name: &str, source: &str, needle: &str) {
    let output = support::check_src("e2e", name, source);
    support::expect_reject(&output, needle, name);
}

/// Builds and runs `source`, asserting the process aborts and names `needle` on stderr.
fn aborts(name: &str, source: &str, needle: &str) {
    let output = support::run_src("e2e", name, source);
    support::expect_abort(&output, needle, name);
}

/// Builds and runs a multi-file project, asserting stdout.
fn run_project(name: &str, files: &[(&str, &str)], expected: &str) {
    let output = support::run_files("e2e", name, files);
    support::expect_stdout(&output, expected, name);
}

// ===========================================================================
// Literals and expressions
// ===========================================================================

#[test]
fn integer_literals() {
    run(
        "integer_literals",
        r#"module app;
fun main() {
    let a: i32 = 42;
    let b: i32 = 1_000_000;
    let c: i32 = 0;
    if a == 42 { core::io::write_bytes(1, "a" as &[u8]); }
    if b == 1000000 { core::io::write_bytes(1, "b" as &[u8]); }
    if c == 0 { core::io::write_bytes(1, "c" as &[u8]); }
}
"#,
        "abc",
    );
}

#[test]
fn typed_suffix_literals() {
    run(
        "typed_suffix_literals",
        r#"module app;
fun main() {
    let a: i64 = 5_i64;
    let b: u8 = 200_u8;
    let c: f64 = 2.5_f64;
    let d: f32 = 1.5_f32;
    if a == 5 { core::io::write_bytes(1, "a" as &[u8]); }
    if b == 200 { core::io::write_bytes(1, "b" as &[u8]); }
    if c == 2.5 { core::io::write_bytes(1, "c" as &[u8]); }
    if d == 1.5_f32 { core::io::write_bytes(1, "d" as &[u8]); }
}
"#,
        "abcd",
    );
}

#[test]
fn bool_literals_and_not() {
    run(
        "bool_literals_and_not",
        r#"module app;
fun main() {
    let t = true;
    let f = false;
    if t { core::io::write_bytes(1, "a" as &[u8]); }
    if !f { core::io::write_bytes(1, "b" as &[u8]); }
    if !t { core::io::write_bytes(1, "bad" as &[u8]); } else { core::io::write_bytes(1, "c" as &[u8]); }
}
"#,
        "abc",
    );
}

#[test]
fn char_literal_casts_to_codepoint() {
    run(
        "char_literal_casts_to_codepoint",
        r#"module app;
fun main() {
    let c: char = 'A';
    if (c as u32) == 65 { core::io::write_bytes(1, "a" as &[u8]); }
    let z: char = 'z';
    if (z as u32) == 122 { core::io::write_bytes(1, "b" as &[u8]); }
}
"#,
        "ab",
    );
}

#[test]
fn string_literal_is_a_byte_slice() {
    run(
        "string_literal_is_a_byte_slice",
        r#"module app;
fun main() {
    core::io::write_bytes(1, "hello, phi" as &[u8]);
}
"#,
        "hello, phi",
    );
}

#[test]
fn operator_precedence_and_associativity() {
    run(
        "operator_precedence",
        r#"module app;
fun main() {
    if 2 + 3 * 4 == 14 { core::io::write_bytes(1, "a" as &[u8]); }
    if (2 + 3) * 4 == 20 { core::io::write_bytes(1, "b" as &[u8]); }
    if 10 - 2 - 3 == 5 { core::io::write_bytes(1, "c" as &[u8]); }
    if 20 / 2 / 5 == 2 { core::io::write_bytes(1, "d" as &[u8]); }
    if 2 + 3 * 4 - 6 / 2 == 11 { core::io::write_bytes(1, "e" as &[u8]); }
    if (1 + 2) * (3 + 4) == 21 { core::io::write_bytes(1, "f" as &[u8]); }
}
"#,
        "abcdef",
    );
}

#[test]
fn unary_negation() {
    run(
        "unary_negation",
        r#"module app;
fun main() {
    let a: i32 = -5;
    let b: i32 = 5;
    if a == 0 - 5 { core::io::write_bytes(1, "a" as &[u8]); }
    if -b == a { core::io::write_bytes(1, "b" as &[u8]); }
    if -(a + b) == 0 { core::io::write_bytes(1, "c" as &[u8]); }
}
"#,
        "abc",
    );
}

// ===========================================================================
// Control flow
// ===========================================================================

#[test]
fn if_else_selects_the_correct_branch() {
    run(
        "if_else_branch",
        r#"module app;
fun main() {
    if 5 > 10 {
        core::io::write_bytes(1, "then" as &[u8]);
    } else {
        core::io::write_bytes(1, "else" as &[u8]);
    }
}
"#,
        "else",
    );
}

#[test]
fn else_if_chain() {
    run(
        "else_if_chain",
        r#"module app;
fun grade(n: i32) -> i32 {
    if n >= 90 { return 4; }
    else if n >= 80 { return 3; }
    else if n >= 70 { return 2; }
    else { return 1; }
}
fun main() {
    if grade(95) == 4 { core::io::write_bytes(1, "a" as &[u8]); }
    if grade(85) == 3 { core::io::write_bytes(1, "b" as &[u8]); }
    if grade(75) == 2 { core::io::write_bytes(1, "c" as &[u8]); }
    if grade(10) == 1 { core::io::write_bytes(1, "d" as &[u8]); }
}
"#,
        "abcd",
    );
}

#[test]
fn if_as_an_expression() {
    run(
        "if_expression",
        r#"module app;
fun label(x: i32) -> i32 {
    return if x < 5 { 1 } else { 2 };
}
fun main() {
    if label(1) == 1 { core::io::write_bytes(1, "a" as &[u8]); }
    if label(9) == 2 { core::io::write_bytes(1, "b" as &[u8]); }
}
"#,
        "ab",
    );
}

#[test]
fn while_loop_sums() {
    run(
        "while_sum",
        r#"module app;
fun main() {
    let mut i = 0;
    let mut sum = 0;
    while i < 10 {
        sum += i;
        i += 1;
    }
    if sum == 45 { core::io::write_bytes(1, "ok" as &[u8]); }
}
"#,
        "ok",
    );
}

#[test]
fn while_break_exits_the_loop() {
    run(
        "while_break",
        r#"module app;
fun main() {
    let mut i = 0;
    while i < 100 {
        if i == 7 { break; }
        i += 1;
    }
    if i == 7 { core::io::write_bytes(1, "ok" as &[u8]); }
}
"#,
        "ok",
    );
}

#[test]
fn while_continue_skips_the_rest() {
    run(
        "while_continue",
        r#"module app;
fun main() {
    let mut i = 0;
    let mut sum = 0;
    while i < 10 {
        i += 1;
        if i % 2 == 0 { continue; }
        sum += i;
    }
    if sum == 25 { core::io::write_bytes(1, "ok" as &[u8]); }
}
"#,
        "ok",
    );
}

#[test]
fn nested_while_break_only_breaks_inner() {
    run(
        "nested_while_break",
        r#"module app;
fun main() {
    let mut outer = 0;
    let mut inner_total = 0;
    while outer < 5 {
        let mut inner = 0;
        while inner < 10 {
            if inner == 3 { break; }
            inner_total += 1;
            inner += 1;
        }
        outer += 1;
    }
    if inner_total == 15 { core::io::write_bytes(1, "ok" as &[u8]); }
}
"#,
        "ok",
    );
}

#[test]
fn early_return_short_circuits() {
    run(
        "early_return",
        r#"module app;
fun first_negative(a: i32, b: i32, c: i32) -> i32 {
    if a < 0 { return a; }
    if b < 0 { return b; }
    if c < 0 { return c; }
    return 0;
}
fun main() {
    if first_negative(1, 0 - 2, 3) == 0 - 2 { core::io::write_bytes(1, "a" as &[u8]); }
    if first_negative(1, 2, 3) == 0 { core::io::write_bytes(1, "b" as &[u8]); }
}
"#,
        "ab",
    );
}

#[test]
fn while_let_drains_a_value() {
    run(
        "while_let",
        r#"module app;
enum Step { more: i32, done }
fun drain(s: Step) -> i32 {
    let mut cur = s;
    let mut total = 0;
    while let .more(n) = cur {
        total += n;
        cur = Step.done;
    }
    return total;
}
fun main() {
    if drain(Step.more(9)) == 9 { core::io::write_bytes(1, "ok" as &[u8]); }
}
"#,
        "ok",
    );
}

#[test]
fn if_let_as_an_expression() {
    run(
        "if_let_expression",
        r#"module app;
enum E { v: i32, n }
fun get(e: E) -> i32 {
    return if let .v(x) = e { x } else { 0 - 1 };
}
fun main() {
    if get(E.v(5)) == 5 { core::io::write_bytes(1, "a" as &[u8]); }
    if get(E.n) == 0 - 1 { core::io::write_bytes(1, "b" as &[u8]); }
}
"#,
        "ab",
    );
}

#[test]
fn let_else_binding() {
    run(
        "let_else",
        r#"module app;
enum E { v: i32, n }
fun get(e: E) -> i32 {
    let .v(x) = e else { return 0 - 1; };
    return x;
}
fun main() {
    if get(E.v(3)) == 3 { core::io::write_bytes(1, "a" as &[u8]); }
    if get(E.n) == 0 - 1 { core::io::write_bytes(1, "b" as &[u8]); }
}
"#,
        "ab",
    );
}

// ===========================================================================
// Functions and recursion
// ===========================================================================

#[test]
fn functions_with_parameters() {
    run(
        "functions_params",
        r#"module app;
fun add3(a: i32, b: i32, c: i32) -> i32 { return a + b + c; }
fun nested(x: i32) -> i32 { return add3(x, x * 2, x * 3); }
fun main() {
    if add3(1, 2, 3) == 6 { core::io::write_bytes(1, "a" as &[u8]); }
    if nested(2) == 12 { core::io::write_bytes(1, "b" as &[u8]); }
}
"#,
        "ab",
    );
}

#[test]
fn recursion_factorial_and_fibonacci() {
    run(
        "recursion_fact_fib",
        r#"module app;
fun fact(n: i64) -> i64 {
    if n <= 1 { return 1; }
    return n * fact(n - 1);
}
fun fib(n: i32) -> i32 {
    if n < 2 { return n; }
    return fib(n - 1) + fib(n - 2);
}
fun main() {
    if fact(10) == 3628800 { core::io::write_bytes(1, "a" as &[u8]); }
    if fib(15) == 610 { core::io::write_bytes(1, "b" as &[u8]); }
}
"#,
        "ab",
    );
}

#[test]
fn recursion_ackermann() {
    run(
        "recursion_ackermann",
        r#"module app;
fun ack(m: i32, n: i32) -> i32 {
    if m == 0 { return n + 1; }
    if n == 0 { return ack(m - 1, 1); }
    return ack(m - 1, ack(m, n - 1));
}
fun main() {
    if ack(2, 3) == 9 { core::io::write_bytes(1, "a" as &[u8]); }
    if ack(3, 2) == 29 { core::io::write_bytes(1, "b" as &[u8]); }
}
"#,
        "ab",
    );
}

#[test]
fn mutual_recursion() {
    run(
        "mutual_recursion",
        r#"module app;
fun is_even(n: i32) -> bool {
    if n == 0 { return true; }
    return is_odd(n - 1);
}
fun is_odd(n: i32) -> bool {
    if n == 0 { return false; }
    return is_even(n - 1);
}
fun main() {
    if is_even(10) { core::io::write_bytes(1, "a" as &[u8]); }
    if !is_even(7) { core::io::write_bytes(1, "b" as &[u8]); }
    if is_odd(9) { core::io::write_bytes(1, "c" as &[u8]); }
}
"#,
        "abc",
    );
}

#[test]
fn higher_order_function_apply_twice() {
    run(
        "higher_order_apply_twice",
        r#"module app;
fun apply_twice(f: fun(i32) -> i32, x: i32) -> i32 {
    return f(f(x));
}
fun main() {
    if apply_twice(|x| x + 1, 5) == 7 { core::io::write_bytes(1, "a" as &[u8]); }
    if apply_twice(|x| x * 3, 2) == 18 { core::io::write_bytes(1, "b" as &[u8]); }
}
"#,
        "ab",
    );
}

// ===========================================================================
// Structs
// ===========================================================================

#[test]
fn struct_construction_and_field_reads() {
    run(
        "struct_field_reads",
        r#"module app;
struct Point { public x: i32, public y: i32 }
fun main() {
    let p = Point { x: 7, y: 9 };
    if p.x == 7 { core::io::write_bytes(1, "a" as &[u8]); }
    if p.y == 9 { core::io::write_bytes(1, "b" as &[u8]); }
}
"#,
        "ab",
    );
}

#[test]
fn nested_struct_field_access() {
    run(
        "nested_struct",
        r#"module app;
struct Inner { public v: i32 }
struct Outer { public inner: Inner, public n: i32 }
fun main() {
    let o = Outer { inner: Inner { v: 7 }, n: 3 };
    if o.inner.v == 7 { core::io::write_bytes(1, "a" as &[u8]); }
    if o.n == 3 { core::io::write_bytes(1, "b" as &[u8]); }
}
"#,
        "ab",
    );
}

#[test]
fn struct_methods_by_reference_and_value() {
    run(
        "struct_methods",
        r#"module app;
struct Counter { public count: i32 }
extend Counter {
    public fun get(&self) -> i32 { return self.count; }
    public fun bumped(&self) -> Counter { return Counter { count: self.count + 1 }; }
    public fun into(self) -> i32 { return self.count; }
}
fun main() {
    let c = Counter { count: 5 };
    if c.get() == 5 { core::io::write_bytes(1, "a" as &[u8]); }
    let bumped = c.bumped();
    if bumped.get() == 6 { core::io::write_bytes(1, "b" as &[u8]); }
    if c.into() == 5 { core::io::write_bytes(1, "c" as &[u8]); }
}
"#,
        "abc",
    );
}

#[test]
fn mutable_struct_method_changes_state() {
    run(
        "struct_mut_method",
        r#"module app;
struct Counter { public count: i32 }
extend Counter {
    public fun bump(&mut self) { self.count = self.count + 1; }
    public fun add(&mut self, n: i32) { self.count = self.count + n; }
}
fun main() {
    let mut c = Counter { count: 0 };
    c.bump();
    c.bump();
    c.add(10);
    if c.count == 12 { core::io::write_bytes(1, "ok" as &[u8]); }
}
"#,
        "ok",
    );
}

#[test]
fn generic_struct_method() {
    run(
        "generic_struct_method",
        r#"module app;
struct Wrap<T> { public value: T }
extend<T> Wrap<T> {
    public fun get(self) -> T { return self.value; }
    public fun same(self) -> Self { return self; }
}
fun main() {
    let w: Wrap<i32> = Wrap { value: 7 };
    if w.get() == 7 { core::io::write_bytes(1, "a" as &[u8]); }
    let b: Wrap<bool> = Wrap { value: true };
    if b.same().get() { core::io::write_bytes(1, "b" as &[u8]); }
}
"#,
        "ab",
    );
}

#[test]
fn struct_with_two_type_parameters() {
    run(
        "struct_two_params",
        r#"module app;
struct Pair<A, B> { public first: A, public second: B }
extend<A, B> Pair<A, B> {
    public fun swap(self) -> Pair<B, A> { return Pair { first: self.second, second: self.first }; }
}
fun main() {
    let p = Pair { first: 1, second: true };
    if p.first == 1 { core::io::write_bytes(1, "a" as &[u8]); }
    let q = p.swap();
    if q.second == 1 { core::io::write_bytes(1, "b" as &[u8]); }
}
"#,
        "ab",
    );
}

// ===========================================================================
// Enums and pattern matching
// ===========================================================================

#[test]
fn enum_unit_variants() {
    run(
        "enum_unit_variants",
        r#"module app;
enum Color { red, green, blue }
fun code(c: Color) -> i32 {
    return match c {
        .red => 1,
        .green => 2,
        .blue => 3,
    };
}
fun main() {
    if code(Color.red) == 1 { core::io::write_bytes(1, "a" as &[u8]); }
    if code(Color.green) == 2 { core::io::write_bytes(1, "b" as &[u8]); }
    if code(Color.blue) == 3 { core::io::write_bytes(1, "c" as &[u8]); }
}
"#,
        "abc",
    );
}

#[test]
fn enum_payload_shapes() {
    run(
        "enum_payload_shapes",
        r#"module app;
struct Rect { public l: i32, public w: i32 }
enum Shape {
    nothing,
    circle: i32,
    rectangle: Rect,
    square: { side: i32 },
    pair: (i32, i32),
}
fun area(s: Shape) -> i32 {
    return match s {
        .nothing => 0,
        .circle(r) => r * r,
        .rectangle(rect) => rect.l * rect.w,
        .square { side } => side * side,
        .pair((a, b)) => a * b,
    };
}
fun main() {
    if area(Shape.nothing) == 0 { core::io::write_bytes(1, "a" as &[u8]); }
    if area(Shape.circle(3)) == 9 { core::io::write_bytes(1, "b" as &[u8]); }
    if area(Shape.rectangle(Rect { l: 4, w: 5 })) == 20 { core::io::write_bytes(1, "c" as &[u8]); }
    if area(Shape.square { side: 6 }) == 36 { core::io::write_bytes(1, "d" as &[u8]); }
    if area(Shape.pair((2, 7))) == 14 { core::io::write_bytes(1, "e" as &[u8]); }
}
"#,
        "abcde",
    );
}

#[test]
fn enum_match_with_wildcard() {
    run(
        "enum_wildcard",
        r#"module app;
enum E { a, b, c }
fun classify(e: E) -> i32 {
    return match e {
        .a => 1,
        _ => 2,
    };
}
fun main() {
    if classify(E.a) == 1 { core::io::write_bytes(1, "a" as &[u8]); }
    if classify(E.b) == 2 { core::io::write_bytes(1, "b" as &[u8]); }
    if classify(E.c) == 2 { core::io::write_bytes(1, "c" as &[u8]); }
}
"#,
        "abc",
    );
}

#[test]
fn enum_match_through_a_reference() {
    run(
        "enum_match_ref",
        r#"module app;
enum Opt { some: i32, none }
fun peek(o: &Opt) -> i32 {
    return match o {
        .some(v) => *v,
        .none => 0 - 1,
    };
}
fun main() {
    let a: Opt = .some(5);
    let b: Opt = .none;
    if peek(&a) == 5 { core::io::write_bytes(1, "a" as &[u8]); }
    if peek(&b) == 0 - 1 { core::io::write_bytes(1, "b" as &[u8]); }
}
"#,
        "ab",
    );
}

#[test]
fn if_let_extracts_payload() {
    run(
        "if_let_payload",
        r#"module app;
enum E { v: i32, n }
fun f(e: E) -> i32 {
    if let .v(x) = e { return x; }
    return 0 - 1;
}
fun main() {
    if f(E.v(8)) == 8 { core::io::write_bytes(1, "a" as &[u8]); }
    if f(E.n) == 0 - 1 { core::io::write_bytes(1, "b" as &[u8]); }
}
"#,
        "ab",
    );
}

// ===========================================================================
// Option and Result
// ===========================================================================

#[test]
fn option_basics_and_unwrap() {
    run(
        "option_basics",
        r#"module app;
fun main() {
    let a: Option<i32> = .some(4);
    let b: Option<i32> = .none;
    if a.is_some() { core::io::write_bytes(1, "a" as &[u8]); }
    if b.is_none() { core::io::write_bytes(1, "b" as &[u8]); }
    if a.unwrap() == 4 { core::io::write_bytes(1, "c" as &[u8]); }
    let c: Option<i32> = .some(2);
    if c.unwrap_or(9) == 2 { core::io::write_bytes(1, "d" as &[u8]); }
    let d: Option<i32> = .none;
    if d.unwrap_or(9) == 9 { core::io::write_bytes(1, "e" as &[u8]); }
}
"#,
        "abcde",
    );
}

#[test]
fn option_map_and_and_then() {
    run(
        "option_map_chain",
        r#"module app;
fun main() {
    let a: Option<i32> = .some(4);
    if a.map(|x| x * 2).unwrap() == 8 { core::io::write_bytes(1, "a" as &[u8]); }
    let b: Option<i32> = .none;
    let doubled = b.map(|x| x * 2);
    if doubled.is_none() { core::io::write_bytes(1, "b" as &[u8]); }
    let c: Option<i32> = .some(3);
    if c.and_then(|x| Option.some(x + 1)).unwrap() == 4 { core::io::write_bytes(1, "c" as &[u8]); }
    let d: Option<i32> = .none;
    if d.unwrap_or_else(|| 11) == 11 { core::io::write_bytes(1, "d" as &[u8]); }
}
"#,
        "abcd",
    );
}

#[test]
fn option_as_ref_and_ok_or() {
    run(
        "option_as_ref_ok_or",
        r#"module app;
fun main() {
    let a: Option<i32> = .some(4);
    if *a.as_ref().unwrap() == 4 { core::io::write_bytes(1, "a" as &[u8]); }
    let r: Result<i32, bool> = a.ok_or(false);
    if r.unwrap() == 4 { core::io::write_bytes(1, "b" as &[u8]); }
    let b: Option<i32> = .none;
    let e: Result<i32, bool> = b.ok_or(true);
    if e.is_err() { core::io::write_bytes(1, "c" as &[u8]); }
}
"#,
        "abc",
    );
}

#[test]
fn result_basics() {
    run(
        "result_basics",
        r#"module app;
fun main() {
    let a: Result<i32, bool> = .ok(4);
    let b: Result<i32, bool> = .err(true);
    if a.is_ok() { core::io::write_bytes(1, "a" as &[u8]); }
    if b.is_err() { core::io::write_bytes(1, "b" as &[u8]); }
    if a.unwrap() == 4 { core::io::write_bytes(1, "c" as &[u8]); }
    if b.unwrap_or(7) == 7 { core::io::write_bytes(1, "d" as &[u8]); }
}
"#,
        "abcd",
    );
}

#[test]
fn result_map_and_error_accessors() {
    run(
        "result_map_map_err",
        r#"module app;
fun main() {
    let a: Result<i32, bool> = .ok(4);
    if a.map(|x| x * 2).unwrap() == 8 { core::io::write_bytes(1, "a" as &[u8]); }
    let b: Result<i32, bool> = .err(true);
    if b.map_err(|e| if e { 9 } else { 8 }).unwrap_or(0) == 0 { core::io::write_bytes(1, "b" as &[u8]); }
    let c: Result<i32, bool> = .ok(1);
    if c.ok().unwrap() == 1 { core::io::write_bytes(1, "c" as &[u8]); }
    let d: Result<i32, bool> = .err(false);
    if !d.err().unwrap() { core::io::write_bytes(1, "d" as &[u8]); }
}
"#,
        "abcd",
    );
}

#[test]
fn result_and_then() {
    run(
        "result_and_then",
        r#"module app;
fun main() {
    let a: Result<i32, bool> = .ok(3);
    let r = a.and_then(|x| Result.ok(x + 1));
    if r.unwrap() == 4 { core::io::write_bytes(1, "a" as &[u8]); }
}
"#,
        "a",
    );
}

// ===========================================================================
// Traits, generics, dynamic dispatch
// ===========================================================================

#[test]
fn trait_static_dispatch() {
    run(
        "trait_static_dispatch",
        r#"module app;
trait Shape { fun area(&self) -> i32; }
struct Square { public side: i32 }
struct Rect { public w: i32, public h: i32 }
extend Square with Shape { fun area(&self) -> i32 { return self.side * self.side; } }
extend Rect with Shape { fun area(&self) -> i32 { return self.w * self.h; } }
fun main() {
    let s = Square { side: 4 };
    let r = Rect { w: 3, h: 5 };
    if s.area() == 16 { core::io::write_bytes(1, "a" as &[u8]); }
    if r.area() == 15 { core::io::write_bytes(1, "b" as &[u8]); }
}
"#,
        "ab",
    );
}

#[test]
fn trait_default_method_calls_the_override() {
    run(
        "trait_default_method",
        r#"module app;
trait Cloner {
    fun clone_it(&self) -> Self;
    fun twice(&self) -> Self { return self.clone_it(); }
}
struct N { public value: i32 }
struct M { public value: i32 }
extend N with Cloner { fun clone_it(&self) -> Self { return N { value: self.value + 1 }; } }
extend M with Cloner { fun clone_it(&self) -> Self { return M { value: self.value + 2 }; } }
fun main() {
    let n: N = N { value: 1 };
    let m: M = M { value: 1 };
    if n.twice().value == 2 { core::io::write_bytes(1, "a" as &[u8]); }
    if m.twice().value == 3 { core::io::write_bytes(1, "b" as &[u8]); }
}
"#,
        "ab",
    );
}

#[test]
fn generic_function_with_bound() {
    run(
        "generic_bound",
        r#"module app;
trait Conv<From> { fun conv(&self) -> From; fun boxed(&self) -> From { return self.conv(); } }
struct W { public v: i32 }
extend W with Conv<i32> { fun conv(&self) -> i32 { return self.v; } }
fun call_boxed<T: Conv<i32>>(x: &T) -> i32 { return x.boxed(); }
fun main() {
    let w = W { v: 5 };
    if call_boxed(&w) == 5 { core::io::write_bytes(1, "ok" as &[u8]); }
}
"#,
        "ok",
    );
}

#[test]
fn trait_with_multiple_bounds() {
    run(
        "trait_multiple_bounds",
        r#"module app;
trait X { fun x(&self) -> i32; }
trait Y { fun y(&self) -> i32; }
struct S { public v: i32 }
extend S with X { fun x(&self) -> i32 { return self.v; } }
extend S with Y { fun y(&self) -> i32 { return self.v + 1; } }
fun combine<T: X + Y>(t: &T) -> i32 { return t.x() + t.y(); }
fun main() {
    let s = S { v: 10 };
    if combine(&s) == 21 { core::io::write_bytes(1, "ok" as &[u8]); }
}
"#,
        "ok",
    );
}

#[test]
fn generic_identity_function() {
    run(
        "generic_identity",
        r#"module app;
fun id<T>(x: T) -> T { return x; }
fun main() {
    if id(5) == 5 { core::io::write_bytes(1, "a" as &[u8]); }
    if id(3.5) == 3.5 { core::io::write_bytes(1, "b" as &[u8]); }
    if id(true) { core::io::write_bytes(1, "c" as &[u8]); }
}
"#,
        "abc",
    );
}

#[test]
fn dyn_dispatch_over_two_types() {
    run(
        "dyn_dispatch",
        r#"module app;
trait Sh { fun sh(&self) -> i32; }
struct A { public v: i32 }
struct B { public v: i32 }
extend A with Sh { fun sh(&self) -> i32 { return self.v; } }
extend B with Sh { fun sh(&self) -> i32 { return self.v * 10; } }
fun draw(s: &dyn Sh) -> i32 { return s.sh(); }
fun main() {
    let a = A { v: 7 };
    let b = B { v: 3 };
    if draw(&a) == 7 { core::io::write_bytes(1, "a" as &[u8]); }
    if draw(&b) == 30 { core::io::write_bytes(1, "b" as &[u8]); }
    let d: &dyn Sh = &b;
    if draw(d) == 30 { core::io::write_bytes(1, "c" as &[u8]); }
}
"#,
        "abc",
    );
}

#[test]
fn dyn_reference_returned_from_a_function() {
    run(
        "dyn_return",
        r#"module app;
trait Counter { fun count(&self) -> i32; }
struct N { public n: i32 }
extend N with Counter { fun count(&self) -> i32 { return self.n; } }
fun pick(n: &N) -> &dyn Counter { return n; }
fun main() {
    let n = N { n: 42 };
    let c: &dyn Counter = pick(&n);
    if c.count() == 42 { core::io::write_bytes(1, "ok" as &[u8]); }
}
"#,
        "ok",
    );
}

// ===========================================================================
// Closures
// ===========================================================================

#[test]
fn closures_capture_and_compute() {
    run(
        "closure_capture",
        r#"module app;
fun main() {
    let base = 10;
    let f = |x| x + base;
    if f(5) == 15 { core::io::write_bytes(1, "a" as &[u8]); }
    let a = 2;
    let b = 3;
    let g = |x| x * a + b;
    if g(4) == 11 { core::io::write_bytes(1, "b" as &[u8]); }
    let h = || 42;
    if h() == 42 { core::io::write_bytes(1, "c" as &[u8]); }
}
"#,
        "abc",
    );
}

#[test]
fn closures_with_explicit_types() {
    run(
        "closure_types",
        r#"module app;
fun apply(f: fun(i32, i32) -> i32, a: i32, b: i32) -> i32 { return f(a, b); }
fun main() {
    let n = apply(|x: i32, y: i32| -> i32 { x + y }, 3, 4);
    if n == 7 { core::io::write_bytes(1, "ok" as &[u8]); }
}
"#,
        "ok",
    );
}

#[test]
fn closure_passed_to_option_map() {
    run(
        "closure_option_map",
        r#"module app;
fun main() {
    let a: Option<bool> = .some(true);
    if a.map(|x| 9).unwrap_or(0) == 9 { core::io::write_bytes(1, "ok" as &[u8]); }
}
"#,
        "ok",
    );
}

// ===========================================================================
// References and projections
// ===========================================================================

#[test]
fn shared_and_mutable_references() {
    run(
        "references",
        r#"module app;
fun read(p: &i32) -> i32 { return *p; }
fun write(p: &mut i32) { *p = *p + 10; }
fun main() {
    let mut x = 5;
    if read(&x) == 5 { core::io::write_bytes(1, "a" as &[u8]); }
    write(&mut x);
    if x == 15 { core::io::write_bytes(1, "b" as &[u8]); }
}
"#,
        "ab",
    );
}

#[test]
fn reference_to_reference_double_deref() {
    run(
        "double_reference",
        r#"module app;
fun main() {
    let a: i32 = 42;
    let r1: &i32 = &a;
    let r2: &&i32 = &r1;
    if **r2 == 42 { core::io::write_bytes(1, "ok" as &[u8]); }
}
"#,
        "ok",
    );
}

#[test]
fn method_call_through_a_reference() {
    run(
        "method_through_reference",
        r#"module app;
struct Wrap<T> { public value: T }
extend<T> Wrap<T> {
    public fun ping(&self) -> i32 { return 3; }
    public fun bump(&mut self) -> i32 { return 5; }
}
fun main() {
    let mut w: Wrap<i32> = Wrap { value: 100 };
    let r = &w;
    let total = r.ping() + w.ping() + w.bump();
    if total == 11 { core::io::write_bytes(1, "ok" as &[u8]); }
}
"#,
        "ok",
    );
}

#[test]
fn compound_assignment_through_a_dereference() {
    run(
        "deref_compound_assign",
        r#"module app;
fun main() {
    let mut x = 5;
    let p: &mut i32 = &mut x;
    *p += 7;
    *p *= 2;
    if x == 24 { core::io::write_bytes(1, "ok" as &[u8]); }
}
"#,
        "ok",
    );
}

#[test]
fn with_block_projection() {
    runs_ok(
        "with_projection",
        r#"module app;
struct P { public x: i32, public y: i32 }
fun main() {
    let mut p: P = P { x: 1, y: 2 };
    with px = &mut p.x, py = &mut p.y {
        *px = 10;
        *py = 20;
    }
}
"#,
    );
}

// ===========================================================================
// Arrays and tuples
// ===========================================================================

#[test]
fn array_element_read_and_write() {
    run(
        "array_index",
        r#"module app;
fun main() {
    let a = new [0; 4];
    (*a)[0] = 5;
    (*a)[1] = 7;
    (*a)[2] = 9;
    if (*a)[0] == 5 { core::io::write_bytes(1, "a" as &[u8]); }
    if (*a)[1] + (*a)[2] == 16 { core::io::write_bytes(1, "b" as &[u8]); }
}
"#,
        "ab",
    );
}

#[test]
fn array_index_by_variable() {
    run(
        "array_variable_index",
        r#"module app;
fun main() {
    let a = new [7; 3];
    let i: i32 = 1;
    let j: i64 = 2;
    if (*a)[i] == 7 { core::io::write_bytes(1, "a" as &[u8]); }
    if (*a)[j] == 7 { core::io::write_bytes(1, "b" as &[u8]); }
}
"#,
        "ab",
    );
}

#[test]
fn array_sum_in_a_loop() {
    run(
        "array_sum_loop",
        r#"module app;
fun main() {
    let a = new [5; 4];
    let mut i = 0;
    let mut sum = 0;
    while i < 4 {
        sum += (*a)[i];
        i += 1;
    }
    if sum == 20 { core::io::write_bytes(1, "ok" as &[u8]); }
}
"#,
        "ok",
    );
}

#[test]
fn tuple_field_and_destructure() {
    run(
        "tuple_ops",
        r#"module app;
fun main() {
    let t = (1, 2, 3);
    if t.0 == 1 { core::io::write_bytes(1, "a" as &[u8]); }
    if t.2 == 3 { core::io::write_bytes(1, "b" as &[u8]); }
    let (x, y) = (10, 20);
    if x + y == 30 { core::io::write_bytes(1, "c" as &[u8]); }
}
"#,
        "abc",
    );
}

// ===========================================================================
// Strings and characters
// ===========================================================================

#[test]
fn string_parameter_is_writable() {
    run(
        "string_parameter",
        r#"module app;
fun emit(s: &[u8]) {
    core::io::write_bytes(1, s);
}
fun main() {
    emit("via-parameter" as &[u8]);
}
"#,
        "via-parameter",
    );
}

#[test]
fn str_copy_preserves_bytes() {
    run(
        "str_copy",
        r#"module app;
fun main() {
    let a: str = "hello";
    let b = a.copy();
    core::io::write_bytes(1, b as &[u8]);
}
"#,
        "hello",
    );
}

// ===========================================================================
// Larger integration programs
// ===========================================================================

#[test]
fn integration_prime_checker() {
    run(
        "integration_primes",
        r#"module app;
fun is_prime(n: i32) -> bool {
    if n < 2 { return false; }
    let mut d = 2;
    while d * d <= n {
        if n % d == 0 { return false; }
        d += 1;
    }
    return true;
}
fun main() {
    if is_prime(97) { core::io::write_bytes(1, "a" as &[u8]); }
    if !is_prime(100) { core::io::write_bytes(1, "b" as &[u8]); }
    if is_prime(2) { core::io::write_bytes(1, "c" as &[u8]); }
    if !is_prime(1) { core::io::write_bytes(1, "d" as &[u8]); }
}
"#,
        "abcd",
    );
}

#[test]
fn integration_sort_three_values() {
    run(
        "integration_sort3",
        r#"module app;
fun main() {
    let mut a = 3;
    let mut b = 1;
    let mut c = 2;
    let tmp = a;
    if a > b { a = b; b = tmp; }
    let tmp2 = b;
    if b > c { b = c; c = tmp2; }
    let tmp3 = a;
    if a > b { a = b; b = tmp3; }
    if a == 1 { core::io::write_bytes(1, "a" as &[u8]); }
    if b == 2 { core::io::write_bytes(1, "b" as &[u8]); }
    if c == 3 { core::io::write_bytes(1, "c" as &[u8]); }
}
"#,
        "abc",
    );
}

#[test]
fn integration_binary_search() {
    run(
        "integration_binary_search",
        r#"module app;
fun search(a: &[i32; 7], key: i32) -> i32 {
    return 0;
}
fun main() {
    let arr = new [0; 7];
    (*arr)[0] = 2;
    (*arr)[1] = 4;
    (*arr)[2] = 6;
    (*arr)[3] = 8;
    (*arr)[4] = 10;
    (*arr)[5] = 12;
    (*arr)[6] = 14;
    let mut lo = 0;
    let mut hi = 7;
    let mut found = 0 - 1;
    while lo < hi {
        let mid = (lo + hi) / 2;
        if (*arr)[mid] == 10 {
            found = mid;
            break;
        } else if (*arr)[mid] < 10 {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    if found == 4 { core::io::write_bytes(1, "ok" as &[u8]); }
}
"#,
        "ok",
    );
}

#[test]
fn integration_fizzbuzz() {
    // Writes a distinct letter for each of the four cases of FizzBuzz, repeated across a
    // range, and checks the exact multiset of outcomes.
    run(
        "integration_fizzbuzz",
        r#"module app;
fun main() {
    let mut i = 1;
    let mut fizz = 0;
    let mut buzz = 0;
    let mut both = 0;
    let mut plain = 0;
    while i <= 30 {
        if i % 15 == 0 { both += 1; }
        else if i % 3 == 0 { fizz += 1; }
        else if i % 5 == 0 { buzz += 1; }
        else { plain += 1; }
        i += 1;
    }
    if fizz == 8 { core::io::write_bytes(1, "a" as &[u8]); }
    if buzz == 4 { core::io::write_bytes(1, "b" as &[u8]); }
    if both == 2 { core::io::write_bytes(1, "c" as &[u8]); }
    if plain == 16 { core::io::write_bytes(1, "d" as &[u8]); }
}
"#,
        "abcd",
    );
}

#[test]
fn integration_state_machine() {
    run(
        "integration_state_machine",
        r#"module app;
enum State { idle, running: i32, done }
fun step(s: State) -> State {
    return match s {
        .idle => State.running(1),
        .running(n) => if n >= 3 { State.done } else { State.running(n + 1) },
        .done => State.done,
    };
}
fun main() {
    let mut s: State = State.idle;
    let mut i = 0;
    while i < 5 {
        s = step(s);
        i += 1;
    }
    match s {
        .done => core::io::write_bytes(1, "ok" as &[u8]),
        .idle => core::io::write_bytes(1, "bad" as &[u8]),
        .running(n) => core::io::write_bytes(1, "bad" as &[u8]),
    };
}
"#,
        "ok",
    );
}

#[test]
fn integration_stack_machine() {
    run(
        "integration_stack",
        r#"module app;
struct Stack { public top: i32, public depth: i32 }
extend Stack {
    public fun push(&mut self, v: i32) {
        self.top = v;
        self.depth = self.depth + 1;
    }
    public fun depth(&self) -> i32 { return self.depth; }
}
fun main() {
    let mut s = Stack { top: 0, depth: 0 };
    let mut i = 0;
    while i < 10 {
        s.push(i * i);
        i += 1;
    }
    if s.depth() == 10 { core::io::write_bytes(1, "a" as &[u8]); }
    if s.top == 81 { core::io::write_bytes(1, "b" as &[u8]); }
}
"#,
        "ab",
    );
}

#[test]
fn integration_generic_max() {
    run(
        "integration_generic_max",
        r#"module app;
trait Comparable2 { fun less_than(&self, other: &Self) -> bool; }
struct Thing { public rank: i32 }
extend Thing with Comparable2 { fun less_than(&self, other: &Self) -> bool { return self.rank < other.rank; } }
fun pick_greater<T: Comparable2>(a: T, b: T) -> T {
    if a.less_than(&b) { return b; }
    return a;
}
fun main() {
    let a = Thing { rank: 3 };
    let b = Thing { rank: 9 };
    if pick_greater(a, b).rank == 9 { core::io::write_bytes(1, "ok" as &[u8]); }
}
"#,
        "ok",
    );
}

// ===========================================================================
// Multi-file projects
// ===========================================================================

#[test]
fn multi_file_function_import() {
    run_project(
        "multi_file_function",
        &[
            (
                "src/main.phi",
                r#"module app;
import math::triple;
fun main() {
    if triple(4) == 12 { core::io::write_bytes(1, "ok" as &[u8]); }
}
"#,
            ),
            (
                "src/math.phi",
                r#"module math;
public fun triple(x: i32) -> i32 { return x * 3; }
"#,
            ),
        ],
        "ok",
    );
}

#[test]
fn multi_file_struct_import() {
    run_project(
        "multi_file_struct",
        &[
            (
                "src/main.phi",
                r#"module app;
import shapes::Point;
fun main() {
    let p = Point { x: 3, y: 4 };
    if p.x + p.y == 7 { core::io::write_bytes(1, "ok" as &[u8]); }
}
"#,
            ),
            (
                "src/shapes.phi",
                r#"module shapes;
public struct Point { public x: i32, public y: i32 }
"#,
            ),
        ],
        "ok",
    );
}

#[test]
fn multi_file_trait_and_impl() {
    run_project(
        "multi_file_trait",
        &[
            (
                "src/main.phi",
                r#"module app;
import geometry::Shape;
import geometry::Circle;
fun main() {
    let c = Circle { radius: 3 };
    if c.area() == 9 { core::io::write_bytes(1, "ok" as &[u8]); }
}
"#,
            ),
            (
                "src/geometry.phi",
                r#"module geometry;
public trait Shape { fun area(&self) -> i32; }
public struct Circle { public radius: i32 }
extend Circle with Shape { fun area(&self) -> i32 { return self.radius * self.radius; } }
"#,
            ),
        ],
        "ok",
    );
}

// ===========================================================================
// Scale / performance regressions
// ===========================================================================

/// A program with thousands of scalar operations must compile and run in seconds.
///
/// The move analyses (`definite_init`, `never_read`, and drop elaboration) once tracked every
/// temporary in a state that grew with the function and was copied block to block, making
/// compilation roughly cubic: this program took over a minute before those passes learned to
/// ignore locals that can never move or be reported on. The generous bound is a regression
/// guard, not a benchmark.
#[test]
fn a_large_scalar_program_compiles_in_reasonable_time() {
    let mut source = String::from("module app;\nfun main() {\n    let mut failures = 0;\n");
    for i in 0..2000i64 {
        source.push_str(&format!(
            "    if !(({i} * 3) == {}) {{ failures += 1; }}\n",
            i * 3
        ));
    }
    source.push_str("    if failures > 0 { core::io::write_bytes(1, \"F\" as &[u8]); }\n}\n");

    let start = std::time::Instant::now();
    let output = support::run_src("e2e", "large_scalar_program", &source);
    let elapsed = start.elapsed();

    support::expect_ok(&output, "large_scalar_program");
    assert_eq!(output.stdout, "", "no check should have failed");
    assert!(
        elapsed < std::time::Duration::from_secs(60),
        "compiling 2000 scalar checks took {elapsed:?}; a move analysis has regressed to superlinear"
    );
}

// ===========================================================================
// Diagnostics: programs the compiler must reject with a clear message
// ===========================================================================

#[test]
fn reject_type_mismatch_in_return() {
    rejects(
        "reject_return_type",
        "module app;\nfun broken() -> bool { return 1; }\nfun main() {}\n",
        "E0302",
    );
}

#[test]
fn reject_unknown_name() {
    rejects(
        "reject_unknown_name",
        "module app;\nfun main() { let x = nope; }\n",
        "E0201",
    );
}

#[test]
fn reject_wrong_arity() {
    rejects(
        "reject_wrong_arity",
        "module app;\nfun f(x: i32) -> i32 { return x; }\nfun main() { let y = f(1, 2); }\n",
        "E0402",
    );
}

#[test]
fn reject_unknown_field() {
    rejects(
        "reject_unknown_field",
        "module app;\nstruct S { public x: i32 }\nfun main() { let s = S { x: 1 }; let y = s.z; }\n",
        "E0306",
    );
}

#[test]
fn reject_non_exhaustive_match() {
    rejects(
        "reject_nonexhaustive",
        r#"module app;
enum E { a, b }
fun f(e: E) -> i32 { return match e { .a => 1, }; }
fun main() {}
"#,
        "E0310",
    );
}

#[test]
fn reject_use_after_move() {
    rejects(
        "reject_use_after_move",
        r#"module app;
struct S { public x: i32 }
fun main() { let a = S { x: 1 }; let b = a; let c = a; }
"#,
        "E0501",
    );
}

#[test]
fn reject_assignment_to_immutable() {
    rejects(
        "reject_immutable_assign",
        "module app;\nfun main() { let x = 1; x = 2; }\n",
        "not declared `mut`",
    );
}

#[test]
fn reject_conflicting_trait_impls() {
    rejects(
        "reject_conflicting_impls",
        r#"module app;
trait T { fun t(&self) -> i32; }
struct S { public x: i32 }
extend S with T { fun t(&self) -> i32 { return 1; } }
extend S with T { fun t(&self) -> i32 { return 2; } }
fun main() {}
"#,
        "conflicting implementations",
    );
}

#[test]
fn reject_missing_return_on_a_path() {
    rejects(
        "reject_missing_return",
        "module app;\nfun f() -> i32 { let x = 1; }\nfun main() {}\n",
        "does not return its declared return type",
    );
}

#[test]
fn reject_unknown_method() {
    rejects(
        "reject_unknown_method",
        "module app;\nstruct S { public x: i32 }\nfun main() { let s = S { x: 1 }; let y = s.missing(); }\n",
        "E0401",
    );
}

#[test]
fn reject_missing_trait_member() {
    rejects(
        "reject_missing_trait_member",
        r#"module app;
trait T { fun t(&self) -> i32; }
struct S { public x: i32 }
extend S with T {}
fun main() {}
"#,
        "t",
    );
}

#[test]
fn reject_operator_on_wrong_type() {
    rejects(
        "reject_bad_operator",
        "module app;\nstruct S { public x: i32 }\nfun main() { let a = S { x: 1 }; let b = S { x: 2 }; let c = a + b; }\n",
        "Add",
    );
}

#[test]
fn reject_negative_literal_for_unsigned() {
    rejects(
        "reject_unsigned_negative",
        "module app;\nfun main() { let x: u8 = -1; }\n",
        "",
    );
}

/// A struct may not declare the same field name twice.
#[test]
fn duplicate_struct_fields_are_rejected() {
    rejects(
        "reject_duplicate_field",
        "module app;\nstruct S { public x: i32, public x: i32 }\nfun main() { let s = S { x: 1, x: 2 }; let y = s.x; }\n",
        "x",
    );
}

// ===========================================================================
// Runtime safety: programs that must abort, not corrupt state
// ===========================================================================

#[test]
fn abort_on_i32_addition_overflow() {
    aborts(
        "abort_add_overflow",
        r#"module app;
fun main() {
    let a: i32 = 2147483647;
    let b: i32 = 1;
    let c = a + b;
    if c == 0 { core::io::write_bytes(1, "unreachable" as &[u8]); }
}
"#,
        "arithmetic overflow",
    );
}

#[test]
fn abort_on_i32_multiplication_overflow() {
    aborts(
        "abort_mul_overflow",
        r#"module app;
fun main() {
    let a: i32 = 100000;
    let b: i32 = 100000;
    let c = a * b;
    if c == 0 { core::io::write_bytes(1, "unreachable" as &[u8]); }
}
"#,
        "arithmetic overflow",
    );
}

#[test]
fn abort_on_u8_addition_overflow() {
    aborts(
        "abort_u8_overflow",
        r#"module app;
fun main() {
    let a: u8 = 200;
    let b: u8 = 100;
    let c = a + b;
    if c == 0 { core::io::write_bytes(1, "unreachable" as &[u8]); }
}
"#,
        "arithmetic overflow",
    );
}

/// Integer negation is overflow-checked in debug mode. Adding and multiplying already abort on
/// overflow; this covers `-x` at the type's minimum.
#[test]
fn unary_negation_overflow_aborts() {
    aborts(
        "abort_neg_overflow",
        r#"module app;
fun main() {
    let a: i8 = 127;
    let b: i8 = (0 - a) - 1;
    let c = -b;
    if c == 0 { core::io::write_bytes(1, "unreachable" as &[u8]); }
}
"#,
        "arithmetic overflow",
    );
}

#[test]
fn abort_on_division_by_zero() {
    aborts(
        "abort_div_zero",
        r#"module app;
fun main() {
    let a: i32 = 1;
    let b: i32 = 0;
    let c = a / b;
    if c == 0 { core::io::write_bytes(1, "unreachable" as &[u8]); }
}
"#,
        "divide by zero",
    );
}

#[test]
fn abort_on_remainder_by_zero() {
    aborts(
        "abort_rem_zero",
        r#"module app;
fun main() {
    let a: i32 = 1;
    let b: i32 = 0;
    let c = a % b;
    if c == 0 { core::io::write_bytes(1, "unreachable" as &[u8]); }
}
"#,
        "divisor of zero",
    );
}

#[test]
fn abort_on_failed_assert() {
    aborts(
        "abort_assert",
        r#"module app;
fun main() {
    assert(1 + 1 == 3, "math is broken");
    core::io::write_bytes(1, "unreachable" as &[u8]);
}
"#,
        "math is broken",
    );
}

#[test]
fn abort_on_explicit_panic() {
    aborts(
        "abort_panic",
        r#"module app;
fun main() {
    panic("boom");
    core::io::write_bytes(1, "unreachable" as &[u8]);
}
"#,
        "boom",
    );
}

#[test]
fn abort_on_unwrap_none() {
    aborts(
        "abort_unwrap_none",
        r#"module app;
fun main() {
    let o: Option<i32> = .none;
    let x = o.unwrap();
    if x == 0 { core::io::write_bytes(1, "unreachable" as &[u8]); }
}
"#,
        "none",
    );
}

#[test]
fn abort_on_unwrap_err() {
    aborts(
        "abort_unwrap_err",
        r#"module app;
fun main() {
    let r: Result<i32, bool> = .err(true);
    let x = r.unwrap();
    if x == 0 { core::io::write_bytes(1, "unreachable" as &[u8]); }
}
"#,
        "err",
    );
}

#[test]
fn abort_on_unreachable() {
    aborts(
        "abort_unreachable",
        r#"module app;
fun main() {
    unreachable("hit it");
    core::io::write_bytes(1, "dead" as &[u8]);
}
"#,
        "hit it",
    );
}

// ===========================================================================
// Known bugs: these assert the *correct* behavior and are ignored until fixed.
// ===========================================================================

/// A compound assignment to a projected field (`self.x += 1`, `p.x += 1`). Codegen used to
/// panic lowering the field place inside the binary operation; `crate::mir::place_ty` now
/// resolves ADT fields, not just tuple fields.
#[test]
fn compound_assignment_to_a_struct_field_runs() {
    run(
        "compound_field_assignment",
        r#"module app;
struct Counter { public count: i32 }
extend Counter {
    public fun bump(&mut self) { self.count += 1; }
}
fun main() {
    let mut c = Counter { count: 0 };
    c.bump();
    c.bump();
    if c.count == 2 { core::io::write_bytes(1, "ok" as &[u8]); }
}
"#,
        "ok",
    );
}

/// An enum that contains itself by value has infinite size; the checker rejects it during type
/// checking (the language provides `iso` for indirection).
#[test]
fn value_recursive_enum_is_rejected() {
    rejects(
        "recursive_value_type",
        r#"module app;
enum List { cons: { head: i32, tail: List }, nil }
fun main() {}
"#,
        "recursive",
    );
}
