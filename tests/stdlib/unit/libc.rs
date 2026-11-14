use super::*;

#[test]
fn sizeof_reports_each_primitive_width() {
    run(
        "libc_sizeof",
        r#"module app;
fun main() {
    println(core::libc::sizeof(&1_i8));
    println(core::libc::sizeof(&1_i32));
    println(core::libc::sizeof(&1_i64));
    println(core::libc::sizeof(&true));
    println(core::libc::sizeof(&'x'));
}
"#,
        "1\n4\n8\n1\n4\n",
    );
}

#[test]
fn alignof_reports_each_primitive_alignment() {
    run(
        "libc_alignof",
        r#"module app;
fun main() {
    println(core::libc::alignof(&1_i8));
    println(core::libc::alignof(&1_i32));
    println(core::libc::alignof(&1_i64));
}
"#,
        "1\n4\n8\n",
    );
}

#[test]
fn sizeof_reports_a_struct_layout() {
    run(
        "libc_sizeof_struct",
        r#"module app;
struct Mixed { a: i8, b: i32, c: i8 }
fun main() {
    let value = Mixed { a: 1, b: 2, c: 3 };
    println(core::libc::sizeof(&value));
    println(core::libc::alignof(&value));
}
"#,
        "12\n4\n",
    );
}
