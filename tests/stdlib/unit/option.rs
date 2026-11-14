use super::*;

#[test]
fn predicates_report_the_variant() {
    run(
        "option_predicates",
        r#"module app;
fun main() {
    let some: Option<i32> = .some(3);
    let none: Option<i32> = .none;
    println(some.is_some());
    println(some.is_none());
    println(none.is_some());
    println(none.is_none());
}
"#,
        "true\nfalse\nfalse\ntrue\n",
    );
}

#[test]
fn as_ref_borrows_the_payload_without_consuming_the_option() {
    run(
        "option_as_ref",
        r#"module app;
fun main() {
    let some: Option<i32> = .some(4);
    match some.as_ref() {
        .some(value) => { println(*value); },
        .none => { println(0); },
    }
    println(some.unwrap());
    let none: Option<i32> = .none;
    let borrowed = none.as_ref();
    println(borrowed.is_none());
}
"#,
        "4\n4\ntrue\n",
    );
}

#[test]
fn unwrap_and_expect_return_the_payload() {
    run(
        "option_unwrap_expect",
        r#"module app;
fun main() {
    let some: Option<i32> = .some(7);
    println(some.unwrap());
    let again: Option<i32> = .some(8);
    println(again.expect("needed a value"));
}
"#,
        "7\n8\n",
    );
}

#[test]
fn unwrap_or_uses_the_fallback_only_for_none() {
    run(
        "option_unwrap_or",
        r#"module app;
fun main() {
    let some_or: Option<i32> = .some(2);
    println(some_or.unwrap_or(9));
    let none_or: Option<i32> = .none;
    println(none_or.unwrap_or(9));
    let some_lazy: Option<i32> = .some(2);
    println(some_lazy.unwrap_or_else(|| 10));
    let none_lazy: Option<i32> = .none;
    println(none_lazy.unwrap_or_else(|| 10));
}
"#,
        "2\n9\n2\n10\n",
    );
}

#[test]
fn map_transforms_only_the_payload() {
    run(
        "option_map",
        r#"module app;
fun main() {
    let some: Option<i32> = .some(4);
    println(some.map(|value| value * 2).unwrap());
    let none: Option<i32> = .none;
    let mapped = none.map(|value| value * 2);
    println(mapped.is_none());
}
"#,
        "8\ntrue\n",
    );
}

#[test]
fn and_then_flattens_a_fallible_transform() {
    run(
        "option_and_then",
        r#"module app;
fun main() {
    let some: Option<i32> = .some(3);
    println(some.and_then(|value| Option.some(value + 1)).unwrap());
    let none: Option<i32> = .none;
    let chained = none.and_then(|value| Option.some(value + 1));
    println(chained.is_none());
}
"#,
        "4\ntrue\n",
    );
}

#[test]
fn ok_or_turns_an_option_into_a_result() {
    run(
        "option_ok_or",
        r#"module app;
fun main() {
    let some: Option<i32> = .some(4);
    println(some.ok_or(false).unwrap());
    let none: Option<i32> = .none;
    let error: Result<i32, bool> = none.ok_or(true);
    println(error.is_err());
}
"#,
        "4\ntrue\n",
    );
}
