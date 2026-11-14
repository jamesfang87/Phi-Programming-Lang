use super::*;

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

#[test]
fn a_for_loop_can_be_the_last_statement_of_a_unit_function() {
    run(
        "for_loop_last_statement",
        r#"module app;
fun main() {
    for i in 0..4 {
        core::io::write_bytes(1, "x" as &[u8]);
    }
}
"#,
        "xxxx",
    );
}

#[test]
fn a_drop_impl_runs_when_its_value_goes_out_of_scope() {
    run(
        "user_drop_runs",
        r#"module app;

import core::ops::Drop;

struct Loud {
    id: i32,
}

extend Loud with Drop {
    fun drop(self) {
        core::io::write_bytes(1, "l" as &[u8]);
    }
}

struct Holder {
    loud: Loud,
}

fun main() {
    let holder = Holder { loud: Loud { id: 1 } };
    core::io::write_bytes(1, "x" as &[u8]);
}
"#,
        "xl",
    );
}

#[test]
fn a_generic_drop_impl_runs_and_then_drops_its_fields() {
    run(
        "generic_drop_runs",
        r#"module app;

import core::ops::Drop;

struct Loud {
    id: i32,
}

extend Loud with Drop {
    fun drop(self) {
        core::io::write_bytes(1, "l" as &[u8]);
    }
}

struct Box<T> {
    value: T,
}

extend<T> Box<T> with Drop {
    fun drop(self) {
        core::io::write_bytes(1, "b" as &[u8]);
    }
}

fun main() {
    let boxed: Box<Loud> = Box { value: Loud { id: 1 } };
    core::io::write_bytes(1, "x" as &[u8]);
}
"#,
        "xbl",
    );
}

#[test]
fn a_vector_drops_its_live_elements() {
    run(
        "vector_drops_elements",
        r#"module app;

import core::ops::Drop;
import std::vector::Vector;

struct Loud {
    id: i32,
}

extend Loud with Drop {
    fun drop(self) {
        core::io::write_bytes(1, "l" as &[u8]);
    }
}

fun main() {
    let mut v: Vector<Loud> = Vector::empty();
    v.push(Loud { id: 1 });
    v.push(Loud { id: 2 });
    v.push(Loud { id: 3 });
    core::io::write_bytes(1, "x" as &[u8]);
}
"#,
        "xlll",
    );
}

#[test]
fn sizeof_and_alignof_report_the_layout_of_a_type() {
    run(
        "sizeof_and_alignof",
        r#"module app;

import core::libc::alignof;
import core::libc::sizeof;

fun main() {
    let small = 0_u8;
    let word = 0_i32;
    let wide = 0_i64;
    if sizeof(&small) == 1 {
        core::io::write_bytes(1, "a" as &[u8]);
    }
    if alignof(&small) == 1 {
        core::io::write_bytes(1, "b" as &[u8]);
    }
    if sizeof(&word) == 4 {
        core::io::write_bytes(1, "c" as &[u8]);
    }
    if alignof(&word) == 4 {
        core::io::write_bytes(1, "d" as &[u8]);
    }
    if sizeof(&wide) == 8 {
        core::io::write_bytes(1, "e" as &[u8]);
    }
    if alignof(&wide) == 8 {
        core::io::write_bytes(1, "f" as &[u8]);
    }
}
"#,
        "abcdef",
    );
}
