use super::*;

#[test]
fn predicates_report_the_variant() {
    run(
        "result_predicates",
        r#"module app;
fun main() {
    let ok: Result<i32, bool> = .ok(3);
    let err: Result<i32, bool> = .err(true);
    println(ok.is_ok());
    println(ok.is_err());
    println(err.is_ok());
    println(err.is_err());
}
"#,
        "true\nfalse\nfalse\ntrue\n",
    );
}

#[test]
fn as_ref_borrows_both_sides() {
    run(
        "result_as_ref",
        r#"module app;
fun main() {
    let ok: Result<i32, bool> = .ok(4);
    match ok.as_ref() {
        .ok(value) => { println(*value); },
        .err(value) => { println(*value); },
    }
    let err: Result<i32, bool> = .err(false);
    match err.as_ref() {
        .ok(value) => { println(*value); },
        .err(value) => { println(*value); },
    }
}
"#,
        "4\nfalse\n",
    );
}

#[test]
fn unwrap_and_expect_return_the_ok_payload() {
    run(
        "result_unwrap_expect",
        r#"module app;
fun main() {
    let ok: Result<i32, bool> = .ok(7);
    println(ok.unwrap());
    let again: Result<i32, bool> = .ok(8);
    println(again.expect("needed a value"));
}
"#,
        "7\n8\n",
    );
}

#[test]
fn unwrap_or_uses_the_fallback_only_for_err() {
    run(
        "result_unwrap_or",
        r#"module app;
fun main() {
    let ok_or: Result<i32, bool> = .ok(2);
    println(ok_or.unwrap_or(9));
    let err_or: Result<i32, bool> = .err(true);
    println(err_or.unwrap_or(9));
    let ok_lazy: Result<i32, bool> = .ok(2);
    println(ok_lazy.unwrap_or_else(|error| if error { 10 } else { 11 }));
    let err_lazy: Result<i32, bool> = .err(true);
    println(err_lazy.unwrap_or_else(|error| if error { 10 } else { 11 }));
}
"#,
        "2\n9\n2\n10\n",
    );
}

#[test]
fn map_and_map_err_transform_one_side() {
    run(
        "result_map",
        r#"module app;
fun main() {
    let ok: Result<i32, bool> = .ok(4);
    println(ok.map(|value| value * 2).unwrap());
    let err: Result<i32, bool> = .err(false);
    let mapped: Result<i32, i32> = err.map_err(|error| if error { 1 } else { 2 });
    match mapped {
        .ok(value) => { println(value); },
        .err(value) => { println(value); },
    }
}
"#,
        "8\n2\n",
    );
}

#[test]
fn and_then_flattens_a_fallible_transform() {
    run(
        "result_and_then",
        r#"module app;
fun main() {
    let ok: Result<i32, bool> = .ok(3);
    println(ok.and_then(|value| Result.ok(value + 1)).unwrap());
    let err: Result<i32, bool> = .err(true);
    let chained = err.and_then(|value| Result.ok(value + 1));
    println(chained.is_err());
}
"#,
        "4\ntrue\n",
    );
}

#[test]
fn ok_and_err_extract_the_matching_side() {
    run(
        "result_accessors",
        r#"module app;
fun main() {
    let ok: Result<i32, bool> = .ok(1);
    println(ok.ok().unwrap());
    let err: Result<i32, bool> = .err(false);
    println(err.err().unwrap());
    let also_err: Result<i32, bool> = .err(false);
    let maybe = also_err.ok();
    println(maybe.is_none());
}
"#,
        "1\nfalse\ntrue\n",
    );
}
