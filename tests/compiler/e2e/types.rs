use super::*;

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
