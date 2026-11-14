mod support;

fn run(name: &str, source: &str, expected: &str) {
    let output = support::run_src("known_gaps", name, source);
    support::expect_stdout(&output, expected, name);
}

#[test]
#[ignore = "known gap: for loops over references need IntoIterator"]
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
#[ignore = "known gap: for loops over references need IntoIterator"]
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
