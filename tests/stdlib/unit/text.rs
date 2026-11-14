use super::*;

#[test]
fn len_and_is_empty_report_the_text_length() {
    run(
        "text_len",
        r#"module app;
fun main() {
    let text: str = "abc";
    println(text.len());
    println(text.is_empty());
    let empty: str = "";
    println(empty.len());
    println(empty.is_empty());
}
"#,
        "3\nfalse\n0\ntrue\n",
    );
}

#[test]
fn equality_compares_the_bytes() {
    run(
        "text_eq",
        r#"module app;
fun main() {
    println("abc" == "abc");
    println("abc" == "abd");
    println("abc" != "abd");
    println("abc" != "abc");
}
"#,
        "true\nfalse\ntrue\nfalse\n",
    );
}

#[test]
fn ordering_compares_lexicographically() {
    run(
        "text_order",
        r#"module app;
fun main() {
    println("abc" < "abd");
    println("abd" < "abc");
    println("abc" < "abcd");
    println("abd" < "abc");
}
"#,
        "true\nfalse\ntrue\nfalse\n",
    );
}
