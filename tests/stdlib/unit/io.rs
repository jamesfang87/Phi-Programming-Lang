use super::*;

#[test]
fn print_omits_a_newline_and_println_adds_one() {
    run(
        "io_print",
        r#"module app;
fun main() {
    print("a");
    print("b");
    println("c");
}
"#,
        "abc\n",
    );
}

#[test]
fn qualified_std_io_exposes_the_same_functions() {
    run(
        "io_qualified",
        r#"module app;
fun main() {
    std::io::print(1);
    std::io::println(2);
}
"#,
        "12\n",
    );
}

#[test]
fn print_formats_every_display_type() {
    run(
        "io_display",
        r#"module app;
fun main() {
    print(true);
    print(' ');
    print(3);
    print(' ');
    print(1.5);
    print(' ');
    print("text");
    println("");
}
"#,
        "true 3 1.5 text\n",
    );
}
