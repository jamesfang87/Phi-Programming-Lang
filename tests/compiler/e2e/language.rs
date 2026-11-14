use super::*;

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
fn for_loops_over_numeric_ranges() {
    run(
        "for_ranges",
        r#"module app;
fun main() {
    let mut sum = 0;
    for i in 0..5 {
        sum += i;
    }
    if sum == 10 { core::io::write_bytes(1, "a" as &[u8]); }

    let mut inclusive = 0;
    for i in 0..=5 {
        inclusive += i;
    }
    if inclusive == 15 { core::io::write_bytes(1, "b" as &[u8]); }

    let n: usize = 4;
    let mut count = 0;
    for _ in 0..n {
        count += 1;
    }
    if count == 4 { core::io::write_bytes(1, "c" as &[u8]); }
}
"#,
        "abc",
    );
}

#[test]
fn range_contains_and_is_empty() {
    run(
        "range_methods",
        r#"module app;
fun main() {
    let r = 2..8;
    if r.contains(&5) { core::io::write_bytes(1, "a" as &[u8]); }
    if !r.contains(&8) { core::io::write_bytes(1, "b" as &[u8]); }
    if !r.is_empty() { core::io::write_bytes(1, "c" as &[u8]); }

    let empty = 4..4;
    if empty.is_empty() { core::io::write_bytes(1, "d" as &[u8]); }

    let inclusive = 2..=8;
    if inclusive.contains(&8) { core::io::write_bytes(1, "e" as &[u8]); }
}
"#,
        "abcde",
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

#[test]
fn operators_build_on_the_primitive_layer() {
    run(
        "primitive_operators",
        r#"module app;

import core::ops::Add;
import core::ops::Comparable;
import core::ops::Eq;

fun plus<T: Add<T>>(a: T, b: T) -> T {
    return a + b;
}

fun main() {
    if plus(2, 3) == 5 {
        core::io::write_bytes(1, "a" as &[u8]);
    }
    if "abc" == "abc" {
        core::io::write_bytes(1, "b" as &[u8]);
    }
    if "abc" != "abd" {
        core::io::write_bytes(1, "c" as &[u8]);
    }
    if "abc" < "abd" {
        core::io::write_bytes(1, "d" as &[u8]);
    }
    if 'a' < 'b' {
        core::io::write_bytes(1, "e" as &[u8]);
    }
    if !(true == false) {
        core::io::write_bytes(1, "f" as &[u8]);
    }
}
"#,
        "abcdef",
    );
}
