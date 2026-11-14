use super::*;

#[test]
fn print_and_println_write_text() {
    run(
        "print_stdout",
        r#"module app;
fun emit(s: str) { println(s); }
fun main() {
    print("a");
    println("b");
    emit("c");
}
"#,
        "ab\nc\n",
    );
}

#[test]
fn display_formats_text_bools_and_integers() {
    run(
        "display_builtin",
        r#"module app;
import std::string::String;
fun main() {
    print("a");
    println(true);
    println(false);
    println(42);
    println(-7);
    println(300);
    let text = String::from("text");
    println(text);
}
"#,
        "atrue\nfalse\n42\n-7\n300\ntext\n",
    );
}

#[test]
fn display_formats_integer_boundaries() {
    run(
        "display_integer_boundaries",
        r#"module app;
fun main() {
    println(0);
    println(9223372036854775807_i64);
    println(-9223372036854775807_i64);
    println(18446744073709551615_u64);
    let smallest = 0_i64 - 9223372036854775807_i64 - 1;
    println(smallest);
}
"#,
        "0\n9223372036854775807\n-9223372036854775807\n18446744073709551615\n-9223372036854775808\n",
    );
}

#[test]
fn a_user_type_can_implement_display() {
    run(
        "display_user_type",
        r#"module app;
import std::fmt::Display;
import std::string::String;

struct Celsius {
    degrees: i32,
}

extend Celsius with Display {
    fun fmt(&self, out: &mut String) {
        self.degrees.fmt(&mut *out);
        out.push_str("C");
    }
}

fun main() {
    let temperature = Celsius { degrees: 21 };
    println(temperature);
}
"#,
        "21C\n",
    );
}

#[test]
fn display_formats_every_integer_width() {
    run(
        "display_integer_widths",
        r#"module app;
fun main() {
    println(1_i8);
    println(2_i16);
    println(3_i32);
    println(4_i64);
    println(5_u8);
    println(6_u16);
    println(7_u32);
    println(8_u64);
    let width: usize = 9;
    println(width);
}
"#,
        "1\n2\n3\n4\n5\n6\n7\n8\n9\n",
    );
}

#[test]
fn to_string_uses_display() {
    run(
        "to_string_display",
        r#"module app;
import std::string::String;
fun main() {
    let n = 42;
    let text = std::fmt::to_string(&n);
    if text == String::from("42") {
        print("ok");
    }
}
"#,
        "ok",
    );
}

#[test]
fn display_formats_floats() {
    run(
        "display_floats",
        r#"module app;
fun main() {
    println(1.0);
    println(1.5);
    println(1.1);
    println(-2.25);
    println(100.0);
    println(0.001);
    println(0.00001);
    println(100000000000000000000.0);
    println(3.141592653589793);
    println(1.5_f32);
    println(0.1_f32);
    println(1.234567_f32);
    println(12345678.0_f32);
    println(10000000000000000.0);
    println(0.0001);
    println(123456789.123456);
    println(0.00009);
    println(123456.789);
    println(0.000000123456);
    println(1.2345678901234567);
}
"#,
        "1\n1.5\n1.1\n-2.25\n100\n0.001\n1e-5\n1e20\n3.141592653589793\n1.5\n0.1\n1.234567\n1.234568e7\n1e16\n0.0001\n123456789.123456\n9e-5\n123456.789\n1.23456e-7\n1.234567890123457\n",
    );
}

#[test]
fn display_formats_chars() {
    run(
        "display_chars",
        r#"module app;
fun main() {
    println('a');
    println('é');
    println('€');
    println('中');
    println('😀');
}
"#,
        "a\né\n€\n中\n😀\n",
    );
}

#[test]
fn display_formats_special_floats() {
    run(
        "display_special_floats",
        r#"module app;
fun main() {
    let nan = 0.0 / 0.0;
    let inf = 1.0 / 0.0;
    let negative_inf = 0.0 - inf;
    println(nan);
    println(inf);
    println(negative_inf);
}
"#,
        "NaN\ninf\n-inf\n",
    );
}

#[test]
fn display_works_through_a_reference() {
    run(
        "display_reference",
        r#"module app;
import std::string::String;
fun main() {
    let text = String::from("borrowed");
    println(&text);
    println(&text);
}
"#,
        "borrowed\nborrowed\n",
    );
}

#[test]
fn std_io_exposes_qualified_print_and_println() {
    run(
        "std_io_qualified",
        r#"module app;
fun main() {
    std::io::print("a");
    std::io::println("b");
}
"#,
        "ab\n",
    );
}
