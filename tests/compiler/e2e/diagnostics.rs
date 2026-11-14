use super::*;

#[test]
fn reject_type_mismatch_in_return() {
    rejects(
        "reject_return_type",
        "module app;\nfun broken() -> bool { return 1; }\nfun main() {}\n",
        "E0302",
    );
}

#[test]
fn reject_unknown_name() {
    rejects(
        "reject_unknown_name",
        "module app;\nfun main() { let x = nope; }\n",
        "E0201",
    );
}

#[test]
fn reject_unknown_name_suggests_a_nearby_one() {
    rejects(
        "reject_unknown_name_hint",
        "module app;\nfun main() { let counter = 1; let y = countr; }\n",
        "did you mean `counter`?",
    );
}

#[test]
fn reject_println_without_display() {
    rejects(
        "reject_println_without_display",
        "module app;\nstruct Plain { public value: i32 }\nfun main() { println(Plain { value: 5 }); }\n",
        "E0428",
    );
}

#[test]
fn reject_wrong_arity() {
    rejects(
        "reject_wrong_arity",
        "module app;\nfun f(x: i32) -> i32 { return x; }\nfun main() { let y = f(1, 2); }\n",
        "E0402",
    );
}

#[test]
fn reject_unknown_field() {
    rejects(
        "reject_unknown_field",
        "module app;\nstruct S { public x: i32 }\nfun main() { let s = S { x: 1 }; let y = s.z; }\n",
        "E0306",
    );
}

#[test]
fn reject_non_exhaustive_match() {
    rejects(
        "reject_nonexhaustive",
        r#"module app;
enum E { a, b }
fun f(e: E) -> i32 { return match e { .a => 1, }; }
fun main() {}
"#,
        "E0310",
    );
}

#[test]
fn reject_use_after_move() {
    rejects(
        "reject_use_after_move",
        r#"module app;
struct S { public x: i32 }
fun main() { let a = S { x: 1 }; let b = a; let c = a; }
"#,
        "E0501",
    );
}

#[test]
fn reject_assignment_to_immutable() {
    rejects(
        "reject_immutable_assign",
        "module app;\nfun main() { let x = 1; x = 2; }\n",
        "not declared `mut`",
    );
}

#[test]
fn reject_conflicting_trait_impls() {
    rejects(
        "reject_conflicting_impls",
        r#"module app;
trait T { fun t(&self) -> i32; }
struct S { public x: i32 }
extend S with T { fun t(&self) -> i32 { return 1; } }
extend S with T { fun t(&self) -> i32 { return 2; } }
fun main() {}
"#,
        "conflicting implementations",
    );
}

#[test]
fn reject_missing_return_on_a_path() {
    rejects(
        "reject_missing_return",
        "module app;\nfun f() -> i32 { let x = 1; }\nfun main() {}\n",
        "does not return its declared return type",
    );
}

#[test]
fn reject_unknown_method() {
    rejects(
        "reject_unknown_method",
        "module app;\nstruct S { public x: i32 }\nfun main() { let s = S { x: 1 }; let y = s.missing(); }\n",
        "E0401",
    );
}

#[test]
fn reject_missing_trait_member() {
    rejects(
        "reject_missing_trait_member",
        r#"module app;
trait T { fun t(&self) -> i32; }
struct S { public x: i32 }
extend S with T {}
fun main() {}
"#,
        "t",
    );
}

#[test]
fn reject_operator_on_wrong_type() {
    rejects(
        "reject_bad_operator",
        "module app;\nstruct S { public x: i32 }\nfun main() { let a = S { x: 1 }; let b = S { x: 2 }; let c = a + b; }\n",
        "Add",
    );
}

#[test]
fn reject_negative_literal_for_unsigned() {
    rejects(
        "reject_unsigned_negative",
        "module app;\nfun main() { let x: u8 = -1; }\n",
        "",
    );
}

#[test]
fn duplicate_struct_fields_are_rejected() {
    rejects(
        "reject_duplicate_field",
        "module app;\nstruct S { public x: i32, public x: i32 }\nfun main() { let s = S { x: 1, x: 2 }; let y = s.x; }\n",
        "x",
    );
}

#[test]
fn value_recursive_enum_is_rejected() {
    rejects(
        "recursive_value_type",
        r#"module app;
enum List { cons: { head: i32, tail: List }, nil }
fun main() {}
"#,
        "recursive",
    );
}
