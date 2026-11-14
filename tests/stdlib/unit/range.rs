use super::*;

#[test]
fn exclusive_ranges_contain_their_left_and_exclude_their_right() {
    run(
        "range_exclusive",
        r#"module app;
fun main() {
    let range = 1..4;
    println(range.contains(&1));
    println(range.contains(&3));
    println(range.contains(&4));
    println(range.contains(&0));
}
"#,
        "true\ntrue\nfalse\nfalse\n",
    );
}

#[test]
fn inclusive_ranges_contain_their_right_endpoint() {
    run(
        "range_inclusive",
        r#"module app;
fun main() {
    let range = 1..=4;
    println(range.contains(&4));
    println(range.contains(&5));
}
"#,
        "true\nfalse\n",
    );
}

#[test]
fn open_ranges_contain_every_value_on_their_open_side() {
    run(
        "range_open",
        r#"module app;
fun main() {
    let below = ..3;
    println(below.contains(&0));
    println(below.contains(&3));
    let above = 3..;
    println(above.contains(&3));
    println(above.contains(&2));
}
"#,
        "true\nfalse\ntrue\nfalse\n",
    );
}

#[test]
fn is_empty_reports_a_range_with_no_values() {
    run(
        "range_is_empty",
        r#"module app;
fun main() {
    let empty_exclusive = 4..4;
    println(empty_exclusive.is_empty());
    let one = 4..5;
    println(one.is_empty());
    let single = 4..=4;
    println(single.is_empty());
    let reversed = 5..4;
    println(reversed.is_empty());
}
"#,
        "true\nfalse\nfalse\ntrue\n",
    );
}

#[test]
fn next_walks_an_exclusive_range_to_none() {
    run(
        "range_next",
        r#"module app;
fun main() {
    let mut range = 2..5;
    println(range.next().unwrap());
    println(range.next().unwrap());
    println(range.next().unwrap());
    let exhausted = range.next();
    println(exhausted.is_none());
}
"#,
        "2\n3\n4\ntrue\n",
    );
}

#[test]
fn next_walks_an_inclusive_range_through_its_right_endpoint() {
    run(
        "range_next_inclusive",
        r#"module app;
fun main() {
    let mut range = 2..=3;
    println(range.next().unwrap());
    println(range.next().unwrap());
    let exhausted = range.next();
    println(exhausted.is_none());
}
"#,
        "2\n3\ntrue\n",
    );
}

#[test]
fn for_loops_iterate_ranges() {
    run(
        "range_for",
        r#"module app;
fun main() {
    let mut sum = 0;
    for value in 1..4 {
        sum += value;
    }
    println(sum);
    let mut inclusive_sum = 0;
    for value in 1..=4 {
        inclusive_sum += value;
    }
    println(inclusive_sum);
}
"#,
        "6\n10\n",
    );
}
