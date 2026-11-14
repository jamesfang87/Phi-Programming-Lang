use super::*;

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
fn option_try_propagates_none() {
    run(
        "option_try",
        r#"module app;
fun bump(o: Option<i32>) -> Option<i32> {
    let v = o?;
    return .some(v + 1);
}
fun main() {
    let a: Option<i32> = .some(41);
    if bump(a).unwrap() == 42 { core::io::write_bytes(1, "a" as &[u8]); }
    let b: Option<i32> = .none;
    let propagated = bump(b);
    if propagated.is_none() { core::io::write_bytes(1, "b" as &[u8]); }
}
"#,
        "ab",
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
