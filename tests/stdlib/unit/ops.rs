use super::*;

#[test]
fn primitive_copy_returns_an_equal_value() {
    run(
        "ops_copy",
        r#"module app;
fun main() {
    let value = 5;
    println(value.copy() == 5);
    let flag = true;
    println(flag.copy());
}
"#,
        "true\ntrue\n",
    );
}

#[test]
fn add_and_eq_can_be_implemented_for_a_struct() {
    run(
        "ops_add_eq",
        r#"module app;
import core::ops::Add;
import core::ops::Eq;
struct Meters { value: i32 }
extend Meters with Add<Meters> {
    fun add(&self, other: &Meters) -> Meters {
        return Meters { value: self.value + other.value };
    }
}
extend Meters with Eq {
    fun eq(&self, other: &Meters) -> bool {
        return self.value == other.value;
    }
}
fun main() {
    let a = Meters { value: 1 };
    let b = Meters { value: 2 };
    let sum = a + b;
    println(sum.value);
    println(sum == Meters { value: 3 });
}
"#,
        "3\ntrue\n",
    );
}

#[test]
fn index_and_index_set_operators_project_a_custom_type() {
    run(
        "ops_index",
        r#"module app;
import core::ops::Index;
import core::ops::IndexSet;
struct Pair { left: i32, right: i32 }
extend Pair with Index<bool, i32> {
    fun index(&self, key: bool) -> &i32 {
        if key {
            return &self.right;
        }
        return &self.left;
    }
}
extend Pair with IndexSet<bool, i32> {
    fun index_set(&mut self, key: bool, value: i32) {
        if key {
            self.right = value;
        } else {
            self.left = value;
        }
    }
}
fun main() {
    let mut pair = Pair { left: 1, right: 2 };
    println(pair[false]);
    println(pair[true]);
    pair[false] = 9;
    println(pair[false]);
}
"#,
        "1\n2\n9\n",
    );
}

#[test]
fn a_drop_impl_runs_when_the_value_leaves_scope() {
    run(
        "ops_drop",
        r#"module app;
import core::ops::Drop;
import std::string::String;
struct Noisy { label: char }
extend Noisy with Drop {
    fun drop(self) {
        let mut text = String::empty();
        text.push_char(self.label);
        println(text.as_str());
    }
}
fun main() {
    let first = Noisy { label: 'a' };
    let second = Noisy { label: 'b' };
    println("body");
}
"#,
        "body\nb\na\n",
    );
}
