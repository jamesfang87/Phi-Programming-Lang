use super::*;

#[test]
fn an_empty_vector_has_zero_length() {
    run(
        "vector_empty",
        r#"module app;
import std::vector::Vector;
fun main() {
    let values: Vector<i32> = Vector::empty();
    println(values.len());
    println(values.is_empty());
}
"#,
        "0\ntrue\n",
    );
}

#[test]
fn push_and_pop_behave_like_a_stack() {
    run(
        "vector_push_pop",
        r#"module app;
import std::vector::Vector;
fun main() {
    let mut values: Vector<i32> = Vector::empty();
    values.push(1);
    values.push(2);
    println(values.len());
    println(values.pop().unwrap());
    println(values.pop().unwrap());
    let empty = values.pop();
    println(empty.is_none());
    println(values.is_empty());
}
"#,
        "2\n2\n1\ntrue\ntrue\n",
    );
}

#[test]
fn get_and_index_project_an_element() {
    run(
        "vector_get",
        r#"module app;
import std::vector::Vector;
fun main() {
    let mut values: Vector<i32> = Vector::empty();
    values.push(4);
    values.push(5);
    println(*values.get(1).unwrap());
    let missing = values.get(2);
    println(missing.is_none());
    println(values[0]);
}
"#,
        "5\ntrue\n4\n",
    );
}

#[test]
fn index_set_replaces_an_element() {
    run(
        "vector_index_set",
        r#"module app;
import std::vector::Vector;
fun main() {
    let mut values: Vector<i32> = Vector::empty();
    values.push(1);
    values.push(2);
    values[0] = 9;
    println(values[0]);
    println(values[1]);
}
"#,
        "9\n2\n",
    );
}

#[test]
fn clear_leaves_an_empty_vector() {
    run(
        "vector_clear",
        r#"module app;
import std::vector::Vector;
fun main() {
    let mut values: Vector<i32> = Vector::empty();
    values.push(1);
    values.push(2);
    values.clear();
    println(values.len());
    println(values.is_empty());
}
"#,
        "0\ntrue\n",
    );
}

#[test]
fn growth_keeps_every_element_in_order() {
    run(
        "vector_grow",
        r#"module app;
import std::vector::Vector;
fun main() {
    let mut values: Vector<i32> = Vector::empty();
    let mut i = 0;
    while i < 100 {
        values.push(i);
        i += 1;
    }
    println(values.len());
    println(values[0]);
    println(values[99]);
}
"#,
        "100\n0\n99\n",
    );
}

#[test]
fn a_vector_iterator_yields_each_element_and_then_none() {
    run(
        "vector_iter",
        r#"module app;
import std::vector::Vector;
fun main() {
    let mut values: Vector<i32> = Vector::empty();
    values.push(6);
    values.push(7);
    let mut iter = values.into_iter();
    println(*iter.next().unwrap());
    println(*iter.next().unwrap());
    let exhausted = iter.next();
    println(exhausted.is_none());
}
"#,
        "6\n7\ntrue\n",
    );
}
