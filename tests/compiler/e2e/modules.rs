use super::*;

#[test]
fn multi_file_function_import() {
    run_project(
        "multi_file_function",
        &[
            (
                "src/main.phi",
                r#"module app;
import math::triple;
fun main() {
    if triple(4) == 12 { core::io::write_bytes(1, "ok" as &[u8]); }
}
"#,
            ),
            (
                "src/math.phi",
                r#"module math;
public fun triple(x: i32) -> i32 { return x * 3; }
"#,
            ),
        ],
        "ok",
    );
}

#[test]
fn private_function_cannot_be_imported() {
    rejects_project(
        "private_function",
        &[
            (
                "src/main.phi",
                r#"module app;
import lib::hidden;
fun main() { hidden(); }
"#,
            ),
            (
                "src/lib.phi",
                r#"module lib;
fun hidden() -> i32 { return 1; }
"#,
            ),
        ],
        "private",
    );
}

#[test]
fn private_field_cannot_be_constructed_or_read() {
    rejects_project(
        "private_field",
        &[
            (
                "src/main.phi",
                r#"module app;
import lib::Secret;
fun main() { let secret = Secret { hidden: 1 }; let value = secret.hidden; }
"#,
            ),
            (
                "src/lib.phi",
                r#"module lib;
public struct Secret { hidden: i32 }
"#,
            ),
        ],
        "private",
    );
}

#[test]
fn private_method_cannot_be_called_across_modules() {
    rejects_project(
        "private_method",
        &[
            (
                "src/main.phi",
                r#"module app;
import lib::Secret;
fun main() {
    let secret = Secret { value: 1 };
    println(secret.hidden());
}
"#,
            ),
            (
                "src/lib.phi",
                r#"module lib;
public struct Secret {
    public value: i32,
}
extend Secret {
    fun hidden(&self) -> i32 {
        return self.value;
    }
}
"#,
            ),
        ],
        "private",
    );
}

#[test]
fn wildcard_import_does_not_import_private_items() {
    rejects_project(
        "private_wildcard",
        &[
            (
                "src/main.phi",
                r#"module app;
import lib::*;
fun main() { hidden(); }
"#,
            ),
            (
                "src/lib.phi",
                r#"module lib;
public fun visible() -> i32 { return 1; }
fun hidden() -> i32 { return 2; }
"#,
            ),
        ],
        "cannot find `hidden`",
    );
}

#[test]
fn multi_file_struct_import() {
    run_project(
        "multi_file_struct",
        &[
            (
                "src/main.phi",
                r#"module app;
import shapes::Point;
fun main() {
    let p = Point { x: 3, y: 4 };
    if p.x + p.y == 7 { core::io::write_bytes(1, "ok" as &[u8]); }
}
"#,
            ),
            (
                "src/shapes.phi",
                r#"module shapes;
public struct Point { public x: i32, public y: i32 }
"#,
            ),
        ],
        "ok",
    );
}

#[test]
fn multi_file_trait_and_impl() {
    run_project(
        "multi_file_trait",
        &[
            (
                "src/main.phi",
                r#"module app;
import geometry::Shape;
import geometry::Circle;
fun main() {
    let c = Circle { radius: 3 };
    if c.area() == 9 { core::io::write_bytes(1, "ok" as &[u8]); }
}
"#,
            ),
            (
                "src/geometry.phi",
                r#"module geometry;
public trait Shape { fun area(&self) -> i32; }
public struct Circle { public radius: i32 }
extend Circle with Shape { fun area(&self) -> i32 { return self.radius * self.radius; } }
"#,
            ),
        ],
        "ok",
    );
}
