use super::*;

#[test]
fn std_string_and_vector_build_and_run() {
    run(
        "std_string_vector",
        r#"module app;
import std::string::String;
import std::vector::Vector;
import core::option::Option;

fun main() {
    let mut text: String = String::empty();
    text.push_str("ab");
    text.push_str("cd");
    if text == String::from("abcd") {
        core::io::write_bytes(1, "s" as &[u8]);
    }

    let mut numbers: Vector<i32> = Vector::empty();
    numbers.push(1);
    numbers.push(2);
    numbers.push(3);
    numbers.push(4);
    numbers.push(5);
    numbers[0] = 7;
    if *numbers[0] == 7 {
        core::io::write_bytes(1, "v" as &[u8]);
    }

    let last = numbers.pop();
    match last {
        .some(value) => if value == 5 { core::io::write_bytes(1, "p" as &[u8]); },
        .none => { },
    };
}
"#,
        "svp",
    );
}
