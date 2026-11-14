use super::*;

#[test]
fn to_string_formats_scalars() {
    run(
        "fmt_to_string_scalars",
        r#"module app;
import std::fmt::to_string;
fun main() {
    println(to_string(&true));
    println(to_string(&false));
    println(to_string(&42));
    println(to_string(&-7));
    println(to_string(&'x'));
    println(to_string(&"text"));
}
"#,
        "true\nfalse\n42\n-7\nx\ntext\n",
    );
}

#[test]
fn to_string_formats_a_string_through_display() {
    run(
        "fmt_to_string_string",
        r#"module app;
import std::fmt::to_string;
import std::string::String;
fun main() {
    let text = String::from("owned");
    println(to_string(&text));
}
"#,
        "owned\n",
    );
}

#[test]
fn to_string_formats_floats() {
    run(
        "fmt_to_string_float",
        r#"module app;
import std::fmt::to_string;
fun main() {
    println(to_string(&1.5));
    println(to_string(&2.25_f32));
}
"#,
        "1.5\n2.25\n",
    );
}

#[test]
fn display_works_through_a_reference() {
    run(
        "fmt_reference",
        r#"module app;
import std::fmt::Display;
fun show<T: Display>(value: &T) {
    println(value);
}
fun main() {
    let value = 9;
    show(&value);
}
"#,
        "9\n",
    );
}

#[test]
fn a_user_type_can_implement_display() {
    run(
        "fmt_user_display",
        r#"module app;
import std::fmt::Display;
import std::string::String;
struct Point { x: i32, y: i32 }
extend Point with Display {
    fun fmt(&self, out: &mut String) {
        out.push_char('(');
        out.push_i64(self.x as i64);
        out.push_str(", ");
        out.push_i64(self.y as i64);
        out.push_char(')');
    }
}
fun main() {
    let point = Point { x: 1, y: 2 };
    println(point);
}
"#,
        "(1, 2)\n",
    );
}
