mod support;

fn run(name: &str, source: &str, expected: &str) {
    let output = support::run_src("iteration", name, source);
    support::expect_stdout(&output, expected, name);
}

#[test]
fn for_loops_iterate_over_slice_projections() {
    run(
        "for_slice_projection",
        r#"module app;

fun main() {
    let mut values = new [0; 3];
    (*values)[0] = 1;
    (*values)[1] = 2;
    (*values)[2] = 3;
    let mut sum = 0;
    for value in &*values {
        sum += *value;
    }
    println(sum);
}
"#,
        "6\n",
    );
}

#[test]
fn for_loops_iterate_over_vector_references() {
    run(
        "for_vector_projection",
        r#"module app;
import std::vector::Vector;

fun main() {
    let mut values: Vector<i32> = Vector::empty();
    values.push(1);
    values.push(2);
    values.push(3);
    let mut sum = 0;
    for value in &values {
        sum += *value;
    }
    println(sum);
}
"#,
        "6\n",
    );
}

#[test]
fn vector_iterator_yields_each_element_once() {
    run(
        "vector_iterator_protocol",
        r#"module app;
import std::vector::Vector;

fun main() {
    let mut values: Vector<i32> = Vector::empty();
    values.push(10);
    values.push(20);
    values.push(30);
    let mut iter = values.into_iter();
    match iter.next() { .some(value) => println(*value), .none => println("none") };
    match iter.next() { .some(value) => println(*value), .none => println("none") };
    match iter.next() { .some(value) => println(*value), .none => println("none") };
    match iter.next() { .some(value) => println(*value), .none => println("none") };
}
"#,
        "10\n20\n30\nnone\n",
    );
}

#[test]
fn slice_iterator_yields_each_element_once() {
    run(
        "slice_iterator_protocol",
        r#"module app;

fun main() {
    let mut values = new [0; 3];
    (*values)[0] = 4;
    (*values)[1] = 5;
    (*values)[2] = 6;
    let mut iter = (&*values).into_iter();
    match iter.next() { .some(value) => println(*value), .none => println("none") };
    match iter.next() { .some(value) => println(*value), .none => println("none") };
    match iter.next() { .some(value) => println(*value), .none => println("none") };
    match iter.next() { .some(value) => println(*value), .none => println("none") };
}
"#,
        "4\n5\n6\nnone\n",
    );
}

#[test]
fn vector_iteration_reads_every_element() {
    run(
        "vector_iteration_sum",
        r#"module app;
import std::vector::Vector;

fun main() {
    let mut values: Vector<i32> = Vector::empty();
    values.push(3);
    values.push(1);
    values.push(4);
    values.push(1);
    values.push(5);
    let mut sum = 0;
    for value in &values {
        sum += *value;
    }
    println(sum);
    println(values.len());
}
"#,
        "14\n5\n",
    );
}

#[test]
fn slice_iteration_reads_every_element() {
    run(
        "slice_iteration_product",
        r#"module app;

fun main() {
    let mut values = new [0; 4];
    (*values)[0] = 3;
    (*values)[1] = 1;
    (*values)[2] = 4;
    (*values)[3] = 1;
    let mut product = 1;
    for value in &values {
        product *= *value;
    }
    println(product);
}
"#,
        "12\n",
    );
}

#[test]
fn iteration_over_empty_collections_runs_zero_times() {
    run(
        "empty_iteration",
        r#"module app;
import std::vector::Vector;

fun main() {
    let values: Vector<i32> = Vector::empty();
    let mut count = 0;
    for _ in &values {
        count += 1;
    }
    let mut empty = new [0; 0];
    for _ in &*empty {
        count += 10;
    }
    println(count);
}
"#,
        "0\n",
    );
}

#[test]
fn break_and_continue_control_iteration() {
    run(
        "iteration_break_continue",
        r#"module app;
import std::vector::Vector;

fun main() {
    let mut values: Vector<i32> = Vector::empty();
    for i in 0..10 {
        values.push(i);
    }
    let mut sum = 0;
    for value in &values {
        if *value == 3 {
            continue;
        }
        if *value == 7 {
            break;
        }
        sum += *value;
    }
    println(sum);
}
"#,
        "18\n",
    );
}

#[test]
fn nested_iteration_visits_every_pair() {
    run(
        "nested_iteration",
        r#"module app;
import std::vector::Vector;

fun main() {
    let mut left: Vector<i32> = Vector::empty();
    left.push(1);
    left.push(2);
    let mut right: Vector<i32> = Vector::empty();
    right.push(10);
    right.push(20);
    right.push(30);
    let mut total = 0;
    for a in &left {
        for b in &right {
            total += *a * *b;
        }
    }
    println(total);
}
"#,
        "180\n",
    );
}

#[test]
fn range_iteration_handles_inclusive_empty_and_negative_bounds() {
    run(
        "iteration_range_shapes",
        r#"module app;

fun main() {
    let mut inclusive = 0;
    for i in 0..=4 {
        inclusive += i;
    }
    println(inclusive);
    let mut empty = 0;
    for i in 4..4 {
        empty += i;
    }
    println(empty);
    let mut negative = 0;
    for i in -2..2 {
        negative += i;
    }
    println(negative);
}
"#,
        "10\n0\n-2\n",
    );
}

#[test]
fn a_vector_can_be_iterated_more_than_once() {
    run(
        "vector_repeat_iteration",
        r#"module app;
import std::vector::Vector;

fun main() {
    let mut values: Vector<i32> = Vector::empty();
    values.push(1);
    values.push(2);
    values.push(3);
    let mut first = 0;
    for value in &values {
        first += *value;
    }
    let mut second = 0;
    for value in &values {
        second += *value;
    }
    println(first);
    println(second);
    println(values.len());
}
"#,
        "6\n6\n3\n",
    );
}

#[test]
fn vector_of_strings_iterates_by_reference() {
    run(
        "vector_string_iteration",
        r#"module app;
import std::string::String;
import std::vector::Vector;

fun main() {
    let mut words: Vector<String> = Vector::empty();
    words.push(String::from("a"));
    words.push(String::from("bb"));
    words.push(String::from("ccc"));
    let mut total = 0;
    for word in &words {
        total += word.len();
    }
    println(total);
}
"#,
        "6\n",
    );
}

#[test]
fn a_slice_can_be_iterated_more_than_once() {
    run(
        "slice_repeat_iteration",
        r#"module app;

fun main() {
    let mut values = new [0; 4];
    (*values)[0] = 1;
    (*values)[1] = 2;
    (*values)[2] = 3;
    (*values)[3] = 4;
    let mut first = 0;
    for value in &*values {
        first += *value;
    }
    let mut second = 0;
    for value in &*values {
        second += *value;
    }
    println(first);
    println(second);
}
"#,
        "10\n10\n",
    );
}

#[test]
fn iteration_over_tuple_elements_reads_every_field() {
    run(
        "vector_tuple_iteration",
        r#"module app;
import std::vector::Vector;

fun main() {
    let mut pairs: Vector<(i32, i32)> = Vector::empty();
    pairs.push((1, 2));
    pairs.push((3, 4));
    let mut total = 0;
    for pair in &pairs {
        total += pair.0 + pair.1;
    }
    println(total);
}
"#,
        "10\n",
    );
}

#[test]
fn iterating_through_a_slice_parameter() {
    run(
        "slice_parameter_iteration",
        r#"module app;

fun sum(values: &[i32]) -> i32 {
    let mut total = 0;
    for value in values {
        total += *value;
    }
    return total;
}

fun main() {
    let mut values = new [0; 3];
    (*values)[0] = 2;
    (*values)[1] = 3;
    (*values)[2] = 5;
    println(sum(&*values));
}
"#,
        "10\n",
    );
}

#[test]
fn iterating_through_a_vector_parameter() {
    run(
        "vector_parameter_iteration",
        r#"module app;
import std::vector::Vector;

fun sum(values: &Vector<i32>) -> i32 {
    let mut total = 0;
    for value in values {
        total += *value;
    }
    return total;
}

fun main() {
    let mut values: Vector<i32> = Vector::empty();
    values.push(2);
    values.push(3);
    values.push(5);
    println(sum(&values));
}
"#,
        "10\n",
    );
}

#[test]
fn two_vector_iterators_have_independent_cursors() {
    run(
        "independent_vector_iterators",
        r#"module app;
import std::vector::Vector;

fun main() {
    let mut values: Vector<i32> = Vector::empty();
    values.push(1);
    values.push(2);
    let mut first = values.into_iter();
    let mut second = values.into_iter();
    match first.next() { .some(value) => println(*value), .none => println("none") };
    match second.next() { .some(value) => println(*value), .none => println("none") };
    match first.next() { .some(value) => println(*value), .none => println("none") };
    match second.next() { .some(value) => println(*value), .none => println("none") };
}
"#,
        "1\n1\n2\n2\n",
    );
}

#[test]
fn iteration_ends_its_borrow_at_the_loop_boundary() {
    run(
        "iteration_borrow_ends",
        r#"module app;
import std::vector::Vector;

fun main() {
    let mut values = new [0; 3];
    (*values)[0] = 1;
    (*values)[1] = 2;
    (*values)[2] = 3;
    let mut slice_total = 0;
    for value in &*values {
        slice_total += *value;
    }
    (*values)[1] = 9;
    println(slice_total);
    println((*values)[1]);

    let mut vector: Vector<i32> = Vector::empty();
    vector.push(1);
    vector.push(2);
    vector.push(3);
    let mut vector_total = 0;
    for value in &vector {
        vector_total += *value;
    }
    vector.push(4);
    let mut second_total = 0;
    for value in &vector {
        second_total += *value;
    }
    println(vector_total);
    println(second_total);
}
"#,
        "6\n9\n6\n10\n",
    );
}
