use super::*;

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
