use super::*;

#[test]
fn empty_and_from_build_the_expected_contents() {
    run(
        "string_construct",
        r#"module app;
import std::string::String;
fun main() {
    let empty = String::empty();
    println(empty.len());
    println(empty.is_empty());
    let text = String::from("abc");
    println(text.len());
    println(text.is_empty());
    println(text.as_str());
}
"#,
        "0\ntrue\n3\nfalse\nabc\n",
    );
}

#[test]
fn as_bytes_exposes_the_backing_bytes() {
    run(
        "string_as_bytes",
        r#"module app;
import std::string::String;
fun main() {
    let text = String::from("abc");
    let bytes = text.as_bytes();
    println(bytes.len());
    println(bytes[0]);
    println(bytes[2]);
}
"#,
        "3\n97\n99\n",
    );
}

#[test]
fn push_str_and_push_char_append_text() {
    run(
        "string_push_text",
        r#"module app;
import std::string::String;
fun main() {
    let mut text = String::from("ab");
    text.push_str("cd");
    text.push_char('!');
    println(text.as_str());
    println(text.len());
}
"#,
        "abcd!\n5\n",
    );
}

#[test]
fn push_integer_and_float_helpers_append_number_text() {
    run(
        "string_push_numbers",
        r#"module app;
import std::string::String;
fun main() {
    let mut text = String::empty();
    text.push_u64(42_u64);
    text.push_char(',');
    text.push_i64(-7_i64);
    text.push_char(',');
    text.push_i64(0_i64);
    println(text.as_str());
}
"#,
        "42,-7,0\n",
    );
}

#[test]
fn push_f32_and_push_f64_append_float_text() {
    run(
        "string_push_float",
        r#"module app;
import std::string::String;
fun main() {
    let mut text = String::empty();
    text.push_f32(1.5_f32);
    text.push_char(',');
    text.push_f64(2.25_f64);
    println(text.as_str());
}
"#,
        "1.5,2.25\n",
    );
}

#[test]
fn clear_empties_the_contents() {
    run(
        "string_clear",
        r#"module app;
import std::string::String;
fun main() {
    let mut text = String::from("abc");
    text.clear();
    println(text.len());
    println(text.is_empty());
    println(text.as_str());
}
"#,
        "0\ntrue\n\n",
    );
}

#[test]
fn equality_and_ordering_compare_the_contents() {
    run(
        "string_compare",
        r#"module app;
import std::string::String;
fun main() {
    let a = String::from("abc");
    let b = String::from("abc");
    let c = String::from("abd");
    println(a == b);
    println(a == c);
    println(a < c);
    println(c < a);
}
"#,
        "true\nfalse\ntrue\nfalse\n",
    );
}

#[test]
fn add_concatenates_every_string_and_str_combination() {
    run(
        "string_add",
        r#"module app;
import std::string::String;
fun main() {
    let a = String::from("ab");
    let b = String::from("cd");
    let first = a + b;
    println(first.as_str());
    let second = a + "!";
    println(second.as_str());
    let third = "pre:" + b;
    println(third.as_str());
}
"#,
        "abcd\nab!\npre:cd\n",
    );
}
