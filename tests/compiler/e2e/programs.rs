use super::*;

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
    let mut arr = new [0; 7];
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
