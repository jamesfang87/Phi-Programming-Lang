use super::*;

#[test]
fn len_and_is_empty_report_the_slice_extent() {
    run(
        "slice_len",
        r#"module app;
fun main() {
    let mut values = new [0; 3];
    values[0] = 1;
    values[1] = 2;
    values[2] = 3;
    let slice: &[i32] = &*values;
    println(slice.len());
    println(slice.is_empty());
    let empty = new [0; 0];
    let empty_slice: &[i32] = &*empty;
    println(empty_slice.len());
    println(empty_slice.is_empty());
}
"#,
        "3\nfalse\n0\ntrue\n",
    );
}

#[test]
fn get_returns_some_for_an_in_bounds_key_and_none_otherwise() {
    run(
        "slice_get",
        r#"module app;
fun main() {
    let mut values = new [0; 3];
    values[0] = 1;
    values[1] = 2;
    values[2] = 3;
    let slice: &[i32] = &*values;
    println(*slice.get(1).unwrap());
    let missing = slice.get(3);
    println(missing.is_none());
}
"#,
        "2\ntrue\n",
    );
}

#[test]
fn index_and_index_set_project_a_slice_element() {
    run(
        "slice_index",
        r#"module app;
fun main() {
    let mut values = new [0; 3];
    values[0] = 7;
    values[1] = 8;
    values[2] = 9;
    let slice: &mut [i32] = &mut *values;
    slice[1] = 5;
    println(slice[0]);
    println(slice[1]);
    println(slice[2]);
}
"#,
        "7\n5\n9\n",
    );
}

#[test]
fn a_slice_iterator_yields_each_element_and_then_none() {
    run(
        "slice_next",
        r#"module app;
fun main() {
    let mut values = new [0; 2];
    values[0] = 4;
    values[1] = 5;
    let slice: &[i32] = &*values;
    let mut iter = slice.into_iter();
    println(*iter.next().unwrap());
    println(*iter.next().unwrap());
    let exhausted = iter.next();
    println(exhausted.is_none());
}
"#,
        "4\n5\ntrue\n",
    );
}

#[test]
fn a_slice_can_be_walked_by_a_for_loop() {
    run(
        "slice_for",
        r#"module app;
fun main() {
    let mut values = new [0; 3];
    values[0] = 1;
    values[1] = 2;
    values[2] = 3;
    let slice: &[i32] = &*values;
    let mut sum = 0;
    for value in slice {
        sum += *value;
    }
    println(sum);
}
"#,
        "6\n",
    );
}
