//! Generated, differential end-to-end tests.
//!
//! Rather than hand-computing expected values, each test here generates a real Phi
//! program whose `main` contains hundreds of tiny assertions. The expected value of every
//! assertion is computed *independently in Rust* by the test itself; the compiler has to
//! reproduce it exactly for the program to write nothing to stdout.
//!
//! A wrong constant fold, operand order, width, signedness, comparison predicate, cast, or
//! loop lowering changes at least one assertion's outcome; the failing `;N;` marker written
//! to stdout then names the exact check. Every program is built, linked, and executed
//! through the real `phi` binary by [`support::run_checks`].

mod support;

use support::check;

// ---------------------------------------------------------------------------
// Integer arithmetic and comparison batteries
// ---------------------------------------------------------------------------

/// Returns `a <op> b` computed at `ty`'s width, or `None` when the operation overflows `ty`
/// or divides by zero.
fn checked_arith(ty: &str, op: char, a: i128, b: i128) -> Option<i128> {
    // `i128::checked_rem` reports `MIN % -1` as `0`; at the target width that quotient
    // overflows and traps, so the arithmetic has to be done in the target type.
    macro_rules! at_width {
        ($t:ty) => {{
            let (a, b) = (a as $t, b as $t);
            match op {
                '+' => a.checked_add(b),
                '-' => a.checked_sub(b),
                '*' => a.checked_mul(b),
                '/' => a.checked_div(b),
                '%' => a.checked_rem(b),
                other => panic!("unknown operator {other}"),
            }
            .map(i128::from)
        }};
    }

    match ty {
        "i8" => at_width!(i8),
        "i16" => at_width!(i16),
        "i32" => at_width!(i32),
        "i64" => at_width!(i64),
        "u8" => at_width!(u8),
        "u16" => at_width!(u16),
        "u32" => at_width!(u32),
        "u64" => at_width!(u64),
        other => panic!("unknown integer type {other}"),
    }
}

/// Values chosen to cross zero, hit ± boundaries for the narrow types, and stay small
/// enough that most operations in range-checked code do not overflow.
fn sample_values(ty: &str) -> Vec<i128> {
    match ty {
        "i8" => vec![-128, -100, -20, -7, -3, -1, 0, 1, 2, 3, 7, 20, 100, 127],
        "i16" => vec![-32768, -30000, -20, -7, -1, 0, 1, 2, 3, 7, 20, 30000, 32767],
        "i32" => vec![
            -1000000, -5000, -20, -7, -1, 0, 1, 2, 3, 7, 20, 5000, 1000000,
        ],
        "i64" => vec![
            -1000000000000,
            -5000,
            -20,
            -1,
            0,
            1,
            2,
            3,
            20,
            5000,
            1000000000000,
        ],
        "u8" => vec![0, 1, 2, 3, 7, 20, 100, 200, 254, 255],
        "u16" => vec![0, 1, 2, 3, 7, 20, 1000, 60000, 65534, 65535],
        "u32" => vec![0, 1, 2, 3, 7, 20, 1000000, 4294967290],
        "u64" => vec![0, 1, 2, 3, 7, 20, 1000000000, 1000000000000],
        other => panic!("unknown integer type {other}"),
    }
}

/// Emits one check per operand pair for `op`, skipping pairs whose result overflows the type
/// (which must abort, and is covered separately).
fn arith_checks(ty: &str, op: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut idx = 0;
    for a in sample_values(ty) {
        for b in sample_values(ty) {
            let Some(r) = checked_arith(ty, op, a, b) else {
                continue;
            };
            out.push(check(
                idx,
                &format!("(({a}_{ty}) {op} ({b}_{ty})) == ({r}_{ty})"),
            ));
            idx += 1;
        }
    }
    assert!(out.len() > 20, "{ty} {op}: only {} checks", out.len());
    out
}

/// Emits one check per operand pair for a comparison, computing the predicate in Rust.
fn cmp_checks(ty: &str, op: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut idx = 0;
    for a in sample_values(ty) {
        for b in sample_values(ty) {
            let expected = match op {
                "<" => a < b,
                "<=" => a <= b,
                ">" => a > b,
                ">=" => a >= b,
                "==" => a == b,
                "!=" => a != b,
                other => panic!("unknown comparison {other}"),
            };
            let cmp = format!("({a}_{ty}) {op} ({b}_{ty})");
            let condition = if expected { cmp } else { format!("!({cmp})") };
            out.push(check(idx, &condition));
            idx += 1;
        }
    }
    assert!(out.len() > 20, "{ty} {op}: only {} checks", out.len());
    out
}

// ---------------------------------------------------------------------------
// Floating-point batteries
// ---------------------------------------------------------------------------

fn float_samples(ty: &str) -> Vec<String> {
    let values = ["0.5", "1.0", "1.5", "2.0", "3.0", "4.0", "8.0", "0.25"];
    match ty {
        "f32" | "f64" => values.iter().map(|v| v.to_string()).collect(),
        other => panic!("unknown float type {other}"),
    }
}

fn render_float(ty: &str, value: f64) -> String {
    match ty {
        "f32" => format!("{:?}_f32", value as f32),
        "f64" => format!("{:?}_f64", value),
        other => panic!("unknown float type {other}"),
    }
}

fn float_arith_checks(ty: &str, op: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut idx = 0;
    for a in float_samples(ty) {
        for b in float_samples(ty) {
            let expected = if ty == "f32" {
                let (x, y): (f32, f32) = (a.parse().unwrap(), b.parse().unwrap());
                let r = match op {
                    '+' => x + y,
                    '-' => x - y,
                    '*' => x * y,
                    '/' => {
                        if y == 0.0 {
                            continue;
                        }
                        x / y
                    }
                    other => panic!("unknown operator {other}"),
                };
                render_float("f32", r as f64)
            } else {
                let (x, y): (f64, f64) = (a.parse().unwrap(), b.parse().unwrap());
                let r = match op {
                    '+' => x + y,
                    '-' => x - y,
                    '*' => x * y,
                    '/' => {
                        if y == 0.0 {
                            continue;
                        }
                        x / y
                    }
                    other => panic!("unknown operator {other}"),
                };
                render_float("f64", r)
            };
            out.push(check(
                idx,
                &format!("(({a}_{ty}) {op} ({b}_{ty})) == ({expected})"),
            ));
            idx += 1;
        }
    }
    assert!(out.len() > 20, "{ty} {op}: only {} checks", out.len());
    out
}

fn float_cmp_checks(ty: &str, op: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut idx = 0;
    for a in float_samples(ty) {
        for b in float_samples(ty) {
            let expected = if ty == "f32" {
                let (x, y): (f32, f32) = (a.parse().unwrap(), b.parse().unwrap());
                match op {
                    "<" => x < y,
                    "<=" => x <= y,
                    ">" => x > y,
                    ">=" => x >= y,
                    "==" => x == y,
                    "!=" => x != y,
                    other => panic!("unknown comparison {other}"),
                }
            } else {
                let (x, y): (f64, f64) = (a.parse().unwrap(), b.parse().unwrap());
                match op {
                    "<" => x < y,
                    "<=" => x <= y,
                    ">" => x > y,
                    ">=" => x >= y,
                    "==" => x == y,
                    "!=" => x != y,
                    other => panic!("unknown comparison {other}"),
                }
            };
            let cmp = format!("({a}_{ty}) {op} ({b}_{ty})");
            let condition = if expected { cmp } else { format!("!({cmp})") };
            out.push(check(idx, &condition));
            idx += 1;
        }
    }
    assert!(out.len() > 20, "{ty} {op}: only {} checks", out.len());
    out
}

// ---------------------------------------------------------------------------
// Casts: every widening pair the checker accepts, checked at several values.
// ---------------------------------------------------------------------------

const CAST_PAIRS: &[(&str, &str)] = &[
    ("i8", "i16"),
    ("i8", "i32"),
    ("i8", "i64"),
    ("i8", "f32"),
    ("i8", "f64"),
    ("i16", "i32"),
    ("i16", "i64"),
    ("i16", "f32"),
    ("i16", "f64"),
    ("i32", "i64"),
    ("i32", "f64"),
    ("u8", "i16"),
    ("u8", "i32"),
    ("u8", "i64"),
    ("u8", "u16"),
    ("u8", "u32"),
    ("u8", "u64"),
    ("u8", "f32"),
    ("u8", "f64"),
    ("u16", "i32"),
    ("u16", "i64"),
    ("u16", "u32"),
    ("u16", "u64"),
    ("u16", "f32"),
    ("u16", "f64"),
    ("u32", "i64"),
    ("u32", "u64"),
    ("u32", "f64"),
    ("f32", "f64"),
];

fn cast_checks(from: &str, to: &str) -> Vec<String> {
    let is_float = |ty: &str| ty.starts_with('f');
    let sources: Vec<String> = if is_float(from) {
        ["0.0", "1.0", "1.5", "-2.5", "1024.0", "65504.0"]
            .iter()
            .map(|v| v.to_string())
            .collect()
    } else {
        ["0", "1", "7", "42", "100", "127"]
            .iter()
            .map(|v| v.to_string())
            .collect()
    };

    let mut out = Vec::new();
    for (idx, value) in sources.into_iter().enumerate() {
        let source_literal = format!("{value}_{from}");
        let expected = if is_float(to) {
            let parsed: f64 = value.parse().expect("float literal");
            render_float(to, parsed)
        } else {
            let parsed: i128 = value.parse().expect("integer literal");
            format!("{parsed}_{to}")
        };
        out.push(check(
            idx,
            &format!("(({source_literal}) as {to}) == ({expected})"),
        ));
    }
    out
}

// ---------------------------------------------------------------------------
// Loop / recursion batteries: the expected result is computed in Rust and compared to a
// Phi implementation of the same algorithm.
// ---------------------------------------------------------------------------

const SUM_ITEMS: &str = "\
fun sum_to(n: i32) -> i32 {
    let mut i = 0;
    let mut acc = 0;
    while i < n {
        acc += i;
        i += 1;
    }
    return acc;
}
";

#[test]
fn generated_loop_sum_matches_formula() {
    let mut checks = Vec::new();
    for n in 0..300i32 {
        let expected = n * (n - 1) / 2;
        checks.push(check(n as usize, &format!("sum_to({n}) == {expected}")));
    }
    support::run_checks("gen_loops", "loop_sum", SUM_ITEMS, &checks);
}

const NESTED_ITEMS: &str = "\
fun nested_sum(n: i32, m: i32) -> i32 {
    let mut i = 0;
    let mut total = 0;
    while i < n {
        let mut j = 0;
        while j < m {
            total += i * j;
            j += 1;
        }
        i += 1;
    }
    return total;
}
";

#[test]
fn generated_nested_loop_sum() {
    let mut checks = Vec::new();
    let mut idx = 0;
    for n in 0..12i32 {
        for m in 0..12i32 {
            let expected: i32 = (0..n).map(|i| (0..m).map(|j| i * j).sum::<i32>()).sum();
            checks.push(check(idx, &format!("nested_sum({n}, {m}) == {expected}")));
            idx += 1;
        }
    }
    support::run_checks("gen_loops", "nested_loop_sum", NESTED_ITEMS, &checks);
}

const FACT_ITEMS: &str = "\
fun fact(n: i64) -> i64 {
    if n <= 1 { return 1; }
    return n * fact(n - 1);
}
";

#[test]
fn generated_factorial() {
    let mut checks = Vec::new();
    let mut expected: i64 = 1;
    for n in 0..=20i64 {
        if n > 1 {
            expected *= n;
        }
        checks.push(check(n as usize, &format!("fact({n}) == {expected}")));
    }
    support::run_checks("gen_recursion", "factorial", FACT_ITEMS, &checks);
}

const FIB_ITEMS: &str = "\
fun fib(n: i32) -> i32 {
    if n < 2 { return n; }
    return fib(n - 1) + fib(n - 2);
}
";

#[test]
fn generated_fibonacci() {
    let mut checks = Vec::new();
    for n in 0..=28i32 {
        let mut a = 0i32;
        let mut b = 1i32;
        for _ in 0..n {
            let next = a + b;
            a = b;
            b = next;
        }
        checks.push(check(n as usize, &format!("fib({n}) == {a}")));
    }
    support::run_checks("gen_recursion", "fibonacci", FIB_ITEMS, &checks);
}

const GCD_ITEMS: &str = "\
fun gcd(a: i32, b: i32) -> i32 {
    if b == 0 { return a; }
    return gcd(b, a % b);
}
";

#[test]
fn generated_gcd() {
    let mut checks = Vec::new();
    let mut idx = 0;
    for a in 1..=120i32 {
        for b in 1..=40i32 {
            let mut x = a;
            let mut y = b;
            while y != 0 {
                let t = x % y;
                x = y;
                y = t;
            }
            checks.push(check(idx, &format!("gcd({a}, {b}) == {x}")));
            idx += 1;
        }
    }
    support::run_checks("gen_recursion", "gcd", GCD_ITEMS, &checks);
}

const POWER_ITEMS: &str = "\
fun power(base: i64, exp: i32) -> i64 {
    if exp == 0 { return 1; }
    return base * power(base, exp - 1);
}
";

#[test]
fn generated_power() {
    let mut checks = Vec::new();
    let mut idx = 0;
    for base in 2..=5i64 {
        let mut expected: i64 = 1;
        for exp in 0..=18i32 {
            if exp > 0 {
                expected *= base;
            }
            if expected > i64::MAX / base {
                break;
            }
            checks.push(check(idx, &format!("power({base}, {exp}) == {expected}")));
            idx += 1;
        }
    }
    support::run_checks("gen_recursion", "power", POWER_ITEMS, &checks);
}

const MUTUAL_ITEMS: &str = "\
fun is_even(n: i32) -> bool {
    if n == 0 { return true; }
    return is_odd(n - 1);
}
fun is_odd(n: i32) -> bool {
    if n == 0 { return false; }
    return is_even(n - 1);
}
";

#[test]
fn generated_mutual_recursion() {
    let mut checks = Vec::new();
    for n in 0..=60i32 {
        let even = n % 2 == 0;
        let branch = if even {
            format!("is_even({n})")
        } else {
            format!("!is_even({n})")
        };
        checks.push(check(n as usize, &branch));
        let odd_branch = if even {
            format!("!is_odd({n})")
        } else {
            format!("is_odd({n})")
        };
        checks.push(check(1000 + n as usize, &odd_branch));
    }
    support::run_checks("gen_recursion", "mutual_recursion", MUTUAL_ITEMS, &checks);
}

const COLLATZ_ITEMS: &str = "\
fun collatz_steps(n: i64) -> i32 {
    let mut steps = 0;
    let mut value = n;
    while value != 1 {
        if value % 2 == 0 {
            value = value / 2;
        } else {
            value = 3 * value + 1;
        }
        steps += 1;
    }
    return steps;
}
";

#[test]
fn generated_collatz_lengths() {
    let mut checks = Vec::new();
    for n in 1..=120i64 {
        let mut value = n;
        let mut steps = 0;
        while value != 1 {
            value = if value % 2 == 0 {
                value / 2
            } else {
                3 * value + 1
            };
            steps += 1;
        }
        checks.push(check(n as usize, &format!("collatz_steps({n}) == {steps}")));
    }
    support::run_checks("gen_recursion", "collatz", COLLATZ_ITEMS, &checks);
}

const DIGIT_ITEMS: &str = "\
fun digit_sum(n: i32) -> i32 {
    let mut value = n;
    let mut total = 0;
    while value > 0 {
        total += value % 10;
        value = value / 10;
    }
    return total;
}
";

#[test]
fn generated_digit_sum() {
    let mut checks = Vec::new();
    for n in 0..=9999i32 {
        let expected: i32 = n.to_string().bytes().map(|b| (b - b'0') as i32).sum();
        checks.push(check(n as usize, &format!("digit_sum({n}) == {expected}")));
    }
    support::run_checks("gen_recursion", "digit_sum", DIGIT_ITEMS, &checks);
}

// ---------------------------------------------------------------------------
// Test declarations for the integer suites.
// ---------------------------------------------------------------------------

// `arith_*_i8` etc. Each invocation expands to five tests.
macro_rules! all_int_arith {
    ($($ty:literal => { $($op:literal => $name:ident),* $(,)? }),* $(,)?) => {
        $($(
            #[test]
            fn $name() {
                support::run_checks(
                    "gen_arith",
                    stringify!($name),
                    "",
                    &arith_checks($ty, $op),
                );
            }
        )*)*
    };
}

all_int_arith! {
    "i8"  => { '+' => arith_i8_add, '-' => arith_i8_sub, '*' => arith_i8_mul, '/' => arith_i8_div, '%' => arith_i8_rem },
    "i16" => { '+' => arith_i16_add, '-' => arith_i16_sub, '*' => arith_i16_mul, '/' => arith_i16_div, '%' => arith_i16_rem },
    "i32" => { '+' => arith_i32_add, '-' => arith_i32_sub, '*' => arith_i32_mul, '/' => arith_i32_div, '%' => arith_i32_rem },
    "i64" => { '+' => arith_i64_add, '-' => arith_i64_sub, '*' => arith_i64_mul, '/' => arith_i64_div, '%' => arith_i64_rem },
    "u8"  => { '+' => arith_u8_add, '-' => arith_u8_sub, '*' => arith_u8_mul, '/' => arith_u8_div, '%' => arith_u8_rem },
    "u16" => { '+' => arith_u16_add, '-' => arith_u16_sub, '*' => arith_u16_mul, '/' => arith_u16_div, '%' => arith_u16_rem },
    "u32" => { '+' => arith_u32_add, '-' => arith_u32_sub, '*' => arith_u32_mul, '/' => arith_u32_div, '%' => arith_u32_rem },
    "u64" => { '+' => arith_u64_add, '-' => arith_u64_sub, '*' => arith_u64_mul, '/' => arith_u64_div, '%' => arith_u64_rem },
}

macro_rules! all_int_cmp {
    ($($ty:literal => { $($op:literal => $name:ident),* $(,)? }),* $(,)?) => {
        $($(
            #[test]
            fn $name() {
                support::run_checks(
                    "gen_cmp",
                    stringify!($name),
                    "",
                    &cmp_checks($ty, $op),
                );
            }
        )*)*
    };
}

all_int_cmp! {
    "i8"  => { "<" => cmp_i8_lt, "<=" => cmp_i8_le, ">" => cmp_i8_gt, ">=" => cmp_i8_ge, "==" => cmp_i8_eq, "!=" => cmp_i8_ne },
    "i16" => { "<" => cmp_i16_lt, "<=" => cmp_i16_le, ">" => cmp_i16_gt, ">=" => cmp_i16_ge, "==" => cmp_i16_eq, "!=" => cmp_i16_ne },
    "i32" => { "<" => cmp_i32_lt, "<=" => cmp_i32_le, ">" => cmp_i32_gt, ">=" => cmp_i32_ge, "==" => cmp_i32_eq, "!=" => cmp_i32_ne },
    "i64" => { "<" => cmp_i64_lt, "<=" => cmp_i64_le, ">" => cmp_i64_gt, ">=" => cmp_i64_ge, "==" => cmp_i64_eq, "!=" => cmp_i64_ne },
    "u8"  => { "<" => cmp_u8_lt, "<=" => cmp_u8_le, ">" => cmp_u8_gt, ">=" => cmp_u8_ge, "==" => cmp_u8_eq, "!=" => cmp_u8_ne },
    "u16" => { "<" => cmp_u16_lt, "<=" => cmp_u16_le, ">" => cmp_u16_gt, ">=" => cmp_u16_ge, "==" => cmp_u16_eq, "!=" => cmp_u16_ne },
    "u32" => { "<" => cmp_u32_lt, "<=" => cmp_u32_le, ">" => cmp_u32_gt, ">=" => cmp_u32_ge, "==" => cmp_u32_eq, "!=" => cmp_u32_ne },
    "u64" => { "<" => cmp_u64_lt, "<=" => cmp_u64_le, ">" => cmp_u64_gt, ">=" => cmp_u64_ge, "==" => cmp_u64_eq, "!=" => cmp_u64_ne },
}

macro_rules! all_float_arith {
    ($($ty:literal => { $($op:literal => $name:ident),* $(,)? }),* $(,)?) => {
        $($(
            #[test]
            fn $name() {
                support::run_checks(
                    "gen_float_arith",
                    stringify!($name),
                    "",
                    &float_arith_checks($ty, $op),
                );
            }
        )*)*
    };
}

all_float_arith! {
    "f64" => { '+' => farith_f64_add, '-' => farith_f64_sub, '*' => farith_f64_mul, '/' => farith_f64_div },
    "f32" => { '+' => farith_f32_add, '-' => farith_f32_sub, '*' => farith_f32_mul, '/' => farith_f32_div },
}

macro_rules! all_float_cmp {
    ($($ty:literal => { $($op:literal => $name:ident),* $(,)? }),* $(,)?) => {
        $($(
            #[test]
            fn $name() {
                support::run_checks(
                    "gen_float_cmp",
                    stringify!($name),
                    "",
                    &float_cmp_checks($ty, $op),
                );
            }
        )*)*
    };
}

all_float_cmp! {
    "f64" => { "<" => fcmp_f64_lt, "<=" => fcmp_f64_le, ">" => fcmp_f64_gt, ">=" => fcmp_f64_ge, "==" => fcmp_f64_eq, "!=" => fcmp_f64_ne },
    "f32" => { "<" => fcmp_f32_lt, "<=" => fcmp_f32_le, ">" => fcmp_f32_gt, ">=" => fcmp_f32_ge, "==" => fcmp_f32_eq, "!=" => fcmp_f32_ne },
}

macro_rules! all_casts {
    ($($from:literal => $to:literal => $name:ident),* $(,)?) => {
        $(
            #[test]
            fn $name() {
                support::run_checks(
                    "gen_cast",
                    stringify!($name),
                    "",
                    &cast_checks($from, $to),
                );
            }
        )*
    };
}

all_casts! {
    "i8" => "i16" => cast_i8_to_i16,
    "i8" => "i32" => cast_i8_to_i32,
    "i8" => "i64" => cast_i8_to_i64,
    "i8" => "f32" => cast_i8_to_f32,
    "i8" => "f64" => cast_i8_to_f64,
    "i16" => "i32" => cast_i16_to_i32,
    "i16" => "i64" => cast_i16_to_i64,
    "i16" => "f32" => cast_i16_to_f32,
    "i16" => "f64" => cast_i16_to_f64,
    "i32" => "i64" => cast_i32_to_i64,
    "i32" => "f64" => cast_i32_to_f64,
    "u8" => "i16" => cast_u8_to_i16,
    "u8" => "i32" => cast_u8_to_i32,
    "u8" => "i64" => cast_u8_to_i64,
    "u8" => "u16" => cast_u8_to_u16,
    "u8" => "u32" => cast_u8_to_u32,
    "u8" => "u64" => cast_u8_to_u64,
    "u8" => "f32" => cast_u8_to_f32,
    "u8" => "f64" => cast_u8_to_f64,
    "u16" => "i32" => cast_u16_to_i32,
    "u16" => "i64" => cast_u16_to_i64,
    "u16" => "u32" => cast_u16_to_u32,
    "u16" => "u64" => cast_u16_to_u64,
    "u16" => "f32" => cast_u16_to_f32,
    "u16" => "f64" => cast_u16_to_f64,
    "u32" => "i64" => cast_u32_to_i64,
    "u32" => "u64" => cast_u32_to_u64,
    "u32" => "f64" => cast_u32_to_f64,
    "f32" => "f64" => cast_f32_to_f64,
}
