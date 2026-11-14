use super::*;

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
fn with_lend_destructures_a_tuple_pattern() {
    run(
        "with_destructure",
        r#"module app;
fun main() {
    let mut p = (1, 2);
    with (a, b) = &mut p {
        *a = 10;
        *b = 20;
    }
    if p.0 == 10 { core::io::write_bytes(1, "a" as &[u8]); }
    if p.1 == 20 { core::io::write_bytes(1, "b" as &[u8]); }
}
"#,
        "ab",
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
fn any_parameter_receiver_calls_a_shared_method() {
    run(
        "any_receiver_shared",
        r#"module app;
struct Foo { x: i32 }
extend Foo { fun show(&self) -> i32 { return self.x; } }
fun relay(d: any Foo) -> i32 { return d.show(); }
fun main() {
    let f = Foo { x: 5 };
    if relay(f) == 5 { core::io::write_bytes(1, "a" as &[u8]); }
    let g = Foo { x: 7 };
    let r: &Foo = &g;
    if relay(r) == 7 { core::io::write_bytes(1, "b" as &[u8]); }
}
"#,
        "ab",
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

#[test]
fn array_element_read_and_write() {
    run(
        "array_index",
        r#"module app;
fun main() {
    let mut a = new [0; 4];
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

#[test]
fn an_iso_auto_derefs_for_methods_and_indexing() {
    run(
        "iso_auto_deref",
        r#"module app;
import std::vector::Vector;

fun main() {
    let mut v: iso Vector<i32> = new Vector::empty();
    v.push(10);
    v.push(20);

    if v.len() == 2 {
        core::io::write_bytes(1, "l" as &[u8]);
    }
    if *v[1] == 20 {
        core::io::write_bytes(1, "r" as &[u8]);
    }

    v[0] = 7;
    if *v[0] == 7 {
        core::io::write_bytes(1, "w" as &[u8]);
    }

    let last = v.pop();
    match last {
        .some(value) => if value == 20 { core::io::write_bytes(1, "p" as &[u8]); },
        .none => { },
    };
}
"#,
        "lrwp",
    );
}

#[test]
fn user_code_cannot_read_uninitialized_storage() {
    rejects(
        "untrusted_intrinsic",
        r#"module app;
import core::libc::malloc;
import core::mem::MaybeUninit;

fun main() {
    let buf: iso [MaybeUninit<i32>] = malloc(2);
}
"#,
        "standard library",
    );
}
