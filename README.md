# The Phi Programming Language

Phi is a modern programming language inspired by Hylo and Rust guaranteeing memory safety and data-race freedom through *mutable value semantics (MVS)*. In MVS, references are treated as **second-class objects** and cannot be stored. While this may sound overly-restrictive, this restriction allows us to completely elide lifetime annotations while preserving non-lexical lifetimes. In addition, it promotes the use of Data-Oriented Design, which greatly improves cache-locality and performance. Furthermore, Phi as a variety of modern language ergonomics, such as projections and the `any` keyword, wrapped in familiar syntax which makes programming in Phi incredibly similar to programming in C++, Rust, or any other languages.

---

## 1. Getting Started

Every Phi program begins with a `main` function:

```phi
fun main() {
    println("Hello, world!");
}
```

`println` is a function that prints its argument, followed by a newline, to standard output. `print` behaves identically but omits the trailing newline.

### Running a program

You can run and compile a single Phi file as so:

```
phi run hello.phi
```

A multi-file project is organized around a `Phi.toml` manifest at the project root root, with source files inside a `src/` directory. To create a new project, you can use the following two commands:

```
phi new [project_name] // creates a new project with the given name under the current directory
phi init               // creates a new project in the current directory with its name
```

To compile and run the project, Phi provides three commands:
```
phi run   // compiles and runs the executable
phi build // compiles and generates an executable w/o running
phi check // just checks source code w/o generating an executable

---

## 2. Lexical Structure

### Comments

```phi
// a line comment, runs to the end of the line

/*
   a block comment,
   which may span multiple lines
   /* and may nest */
*/
```

### Identifiers

Identifiers begin with a letter or underscore and continue with letters, digits, or underscores. Types and traits use `PascalCase`; functions, methods, and variables use `snake_case`.

### Literals

```phi
42          // i32 by default
42_i64      // explicit suffix
3.14        // f64 by default
3.14_f32    // explicit suffix
1_000_000   // underscores may separate digits for readability
true, false // bool
'a'         // char
"hello"     // &String
```
---

## 3. Variables and Bindings

Variables are declared immutable by default with `let`. To declare a mutable variable, add `mut` after `let`.

```phi
fun main() {
    let foo = 0;               // immutable integer
    let mut bar = 1;           // mutable integer
    let mut phi: f64 = 1.618;  // mutable, with an explicit type annotation
}
```

Notice how there are no type annotations provided for the variables `foo` and `bar`. Phi uses **type inference**: the compiler determines the type of `foo` and `bar` from their initial values and later uses. A type annotation, as seen with `phi`, is optional but may be supplied for clarity.

---

## 4. Basic Types

| Category | Types |
|---|---|
| Signed integers | `i8`, `i16`, `i32`, `i64` |
| Unsigned integers | `u8`, `u16`, `u32`, `u64` |
| Floating point | `f32`, `f64` |
| Boolean | `bool` |
| Character | `char` (a single Unicode scalar value) |
| Text | `String` (owned, growable), `&String` (a projected view of text) |
| Grouping | tuples `(T, U, ...)`, arrays `Array<T>`, fixed-size arrays `[T; N]` |

---

## 5. Copies, Moves, and Drops

In Phi, trivially copyable types (such as integers, floats, bools, chars, etc.) are implicitly copied. Every other type moves by default. A moved-from variable cannot be used again, and a value is destroyed at the end of its lifetime. Note that the end of a value's lifetime is the last time it is used, not necessarily the end of its enclosing scope.

```phi
let a = String::from("hi");
let b = a;      // moves; `a` is no longer usable
// println(a);  // compile error: use of moved value
println(b);     // fine
```

A type that owns a resource, such as memory or a file handle, implements the `Drop` trait to define what happens when its last use has passed:

```phi
extend TempFile with Drop {
    fun drop(self) {
        fs::remove(self.path);
    }
}
```

---

## 6. Functions

Functions are declared with `fun`:

```phi
fun foo() {

}
```

A function can take parameters and return a value:

```phi
fun add(x: i32, y: i32) -> i32 {
    return x + y;
}
```

Parameter types and return types must always be annotated; only local variable types are inferred. For trivially copyable types, parameters are passed by value. 
For non-trivially-copyable types, a parameter can be passed in one of three ways:

**1. Immutable borrow** — the callee may read but not mutate or move the argument:

```phi
fun print_both(x: &String, y: &String) {
    println(x);
    println(y);
}
```

**2. Mutable borrow** — the callee may read and mutate the argument, but not move it out:

```phi
fun append_bang(x: &mut String) {
    x.push('!');
}
```

**3. Ownership transfer** — the callee receives the value and now owns it. it may further move it, mutate it, return it, or let it drop:

```phi
fun framed(base: String) -> String {
    base + "!"                 // base may escape into the result — it's ours
                               // note that we do not have to use return,
                               // we can just write the return value without a semicolon on the last line
}

let title = framed(greeting);  // greeting is gone now (String isn't trivial)
```

### Closures

Closures are written the same way as in Rust:

```phi
let add = |x: i32, y: i32| -> i32 { x + y };
let double = |x| x * 2;      // parameter and return types are inferred when omitted
let answer = || 42;          // no parameters
```

Closures are ordinary values and are most often passed directly to a function that takes one:

```phi
let doubled = xs.map(|x| x * 2);
```

### Function types

The type of a function or closure is written `fun(ParamType, ...) -> ReturnType`, and is used anywhere a closure or function is passed or stored as a value — most commonly a higher-order function's parameter type:

```phi
fun apply_twice(f: fun(i32) -> i32, x: i32) -> i32 {
    return f(f(x));
}

apply_twice(|x| x + 1, 5);
```

A function type with no `->` — `fun(String)` — describes a function that doesn't return a value. Function types nest like any other type, so `fun(fun(i32) -> i32) -> fun() -> bool` is the type of a function that takes an `i32 -> i32` function and returns a `() -> bool` function.

---

## 7. Projections

To make programming with MVS ergonomic, Phi has **Projections**, like the ones in the Hylo Programming Language. Projections make it possible to still create and use references without lifting MVS restrictions.

### Projections

Like in Hylo, a projection is essentially a reference to a value, or part of a value. While a projection lives:
- If it is **mutable**, the source cannot be read, mutated, moved, or destroyed through any other name.
- If it is **immutable**, the source cannot be mutated, moved or destroyed, but it, and other immutable projections of it, may still be read.

A general rule for allowed actions after a value has been projected is that a value has either *many readers* or *one writer* (but never both at once). However, note that projections of *disjoint* parts of a value, such as two different fields of the same struct, may coexist independently. By default, a projection's lifetime ends at its last use, exactly like an ordinary variable's:

```phi
fun main() {
    let x = ...;
    let mut y = &mut x;  // as long as y lives, x cannot be read, destroyed, or mutated
                         // if y had instead been an immutable projection (&x),
                         // x could still be read elsewhere, just not mutated
}
```

### Bounding a projection explicitly with `with`

A `with` block gives one or more projections a fixed, declared lifetime — the block itself — rather than leaving the end point to be inferred from last use:

```phi
let mut point = Point { x: 1, y: 2 };

with px = &mut point.x, py = &mut point.y {
    px += 1;
    py += 1;
}
// px and py go out of scope here, unconditionally
```

As every binding in a `with` block ends at the closing brace, regardless of where its own last use happened to fall inside the block, `with` blocks help reduce bugs which may occur when further changes to the code extend or shorten how long a source stays locked due to a projection. They also document exactly how long a projection should live.

### Functions that return a projection

Unlike Hylo, which requires ordinary functions to own the values they return, ordinary functions in Phi *can* return projections. To reduce code duplication, Phi has the `any` keyword which works like methond bundles in Hylo. `any` lets the function's result mode vary based on how the function is used at each call site.

```phi
// `any` lets the function's result mode — &T, &mut T, or a move —
// be decided by how the function is used at the call site
fun min(x: any i32, y: any i32) -> any i32 {
    return if x < y { x } else { y };
}

// parameters and their projection mode can also be pinned down explicitly
fun min(x: &mut i32, y: &mut i32) -> &mut i32 {
    return if x < y { &mut x } else { &mut y };
}
```
The use of any does not have any impact on the performance of the program. The compiler generates a separate function underneath the hood for each mode.

As long as the returned projection lives — which may be a single line if it is used inline at the call site — the source parameters are subject to the same aliasing rules as any local projection. For example, if a function returns a mutable projection which is stored into a variable, the source parameters cannot be read, mutated, moved, or destroyed while the returned projection lives.

### Rules

1. **A projection cannot exceed the access of its source parameter.** A function may not `return &mut` from a parameter that was only borrowed immutably. The reverse is fine.
2. **`any` should only be used for a parameter that is only read.** This is not a hard compiler error, but a convention: `any` will always bind as though `&mut` were possible, so a parameter that is genuinely written to should be declared `&mut` explicitly.
3. **`any` is only meaningful in a function whose return type is `&T`, `&mut T`, or `any T`.** It has no effect on a function returning an owned type.
4. **A function returning `&T`, `&mut T`, or `any T` cannot be overloaded.** There is exactly one declaration with a given name and parameter list in a given scope for such a return type.

---

## 8. Control Flow

### While loops

```phi
let mut i = 0;
while i < 5 {
    i = i + 1;
}
```

### For loops

```phi
let mut sum = 0;
for i in 0..5 {
    sum += i;
}
```

Looping over a data structure uses projections, and the loop's binding mode is written explicitly after the `in`:

```phi
for x in &a {
    // x is an immutable projection of each element; a is untouched afterward
}

for x in &mut a {
    // x is a mutable projection of each element
}

for x in a {
    // the loop consumes a; each x is owned
}
```

### If statements

```phi
if x < 5 {
    println("less than 5");
}
```

`if` may also be used as an expression, evaluating to the value of whichever branch runs:

```phi
let label = if x < 5 { "small" } else { "large" };
```

`if`, `match`,`spawn`, and `concurrent` are the only constructs usable as expressions. `while` and `for` are always statements and never produce a value.

### Pattern matching with statements

`if let` runs its branch only when a value matches a pattern, binding whatever the pattern names:

```phi
if let .some(n) = lookup(key) {
    println(n);
} else {
    println("missing");
}
```
It chains with `else if let`, and like `if`, is usable as an expression when every branch produces a value.

`while let` loops for as long as the value keeps matching, which is the usual way to drain something that reports its own end:

```phi
while let .some(line) = reader.next_line() {
    process(line);
}
```

`let else` declarations also allow you to destructure some pattern:
```phi
let .some(n) = lookup(key) else {
    // handle the error case
}
foo(n) // we have extracted n and can use it after
```

---

## 9. Structs and Methods

Phi supports user-defined types with `struct`.

```phi
struct Vector2D {
    public x: f64,
    public y: f64,
}
```

You can construct a struct with a struct literal. However, this cannot be done if a struct has at least 1 private field. In that case, you must write a function inside an `extend` block (see the next section) which returns an instance of that struct.

```phi
let force = Vector2D { x: 1.0, y: 1.0 };
```

Access fields or call methods with `.`:

```phi
force.x;                   // field access
force.dot(other_force);    // method call
```

### `extend`: adding methods

Unlike in C++ or Java, a struct's methods cannot live in its body. The `extend` construct extends structs with methods. For non-static methods, the first parameter is always `self`, which may be an immutable borrow (`&self`), a mutable borrow (`&mut self`), or an owned value (`self`). Static methods omit `self` entirely.

```phi
extend Vector2D {
    public fun new(x: f64, y: f64) -> Vector2D {
        // Vector2D::new(x, y) must be used instead of a struct literal to initialize
        // a Vector2D outside of the struct if either x or y were private.
        return Vector2D { x: x, y: y };
    }

    public fun normalized(&self) -> Vector2D {
        let m = self.length();
        return Vector2D { x: self.x / m, y: self.y / m };
    }
}
```

**Declaring conformance to a trait**, using `with`:

```phi
extend Vector2D with Comparable {
    fun less_than(&self, other: &Self) -> bool {
        return self.length() < other.length();
    }
}
```

A given type may have as many plain `extend Type { }` blocks as convenient. For `extend Type with Trait { }`, there may be as many as long as they don't conflict. For example, `extend<T> Foo<T> with Bar` conflicts with `extend<i32> Foo<i32> with Bar` but `extend<String> Foo<String> with Bar` does not.

### Methods that return a projection

```phi
struct Pair {
    public first: String,
    public second: String,
}

extend Pair {
    public fun longer(&self) -> &String {
        return if self.first.len() > self.second.len() { &self.first } else { &self.second };
    }
}
```

`any` on `self` follows the same rules described in section 7.

---

## 10. Enums and Pattern Matching

Phi is not an object-oriented language — there is no inheritance. Polymorphism is achieved through sum types called enums.

```phi
struct Rectangle {
    public l: f64,
    public w: f64,
}

enum Shape {
    rectangle: Rectangle,
    circle: f64,
    square: { l: f64 },
    parallelogram: (f64, f64),
}

extend Shape {
    fun perimeter(&self) -> f64 {
        return match self {
            .rectangle(rect)       => 2.0 * (rect.l + rect.w),
            .circle(r)             => 2.0 * 3.14 * r,
            .square { l }          => 4.0 * l,
            .parallelogram((b, h)) => 2.0 * (b + h),
        };
    }
}

fun print_name(shape: &Shape) {
    match shape {
        .rectangle     => println("rectangle"),
        .circle        => println("circle"),
        .square        => println("square"),
        .parallelogram => println("parallelogram"),
    };
}

fun main() {
    let rect          = .rectangle(.{ l: 4.0, w: 6.0 });
    let circle        = .circle(1.24);
    let square        = .square { l: 4.0 };
    let parallelogram = .parallelogram((1.0, 1.0));
}
```

An enum works like a struct that can only have one field set at a time. Variants of an enum can have no type (where only the name of the variant is give), a single type (as seen with `.circle`) or an anonymous struct declared inline (such as `.square`).

### Building a variant

A variant is built by naming it after a `.`, and the payload is written the way its *type* is written:

| Declaration | Construction |
|---|---|
| `circle: f64` | `.circle(1.24)` |
| `rectangle: Rectangle` | `.rectangle(.{ l: 4.0, w: 6.0 })` |
| `parallelogram: (f64, f64)` | `.parallelogram((1.0, 1.0))` |
| `square: { l: f64 }` | `.square { l: 4.0 }` |
| `nothing,` | `.nothing` |

Parentheses hold exactly one value, whatever its type. Braces are only for a payload declared inline as an anonymous struct, because only then do the field names belong to the variant's own declaration. `.square { l }` is shorthand for `.square { l: l }`.

The leading `.` allows you to elide the enum type and have the compiler infer it from the context of the program. You may also name it explicitly when it is ambiguous what enum you are referring to or when it is clearer to be explicit.

```phi
let s = Shape.circle(1.24);
let s = math::Shape.circle(1.24);   // `::` walks modules, `.` reaches a member
```

Note that `.` can also allow you to elide in struct's name in a struct literal (`.{ l: 4.0, w: 6.0 }`) when it is clear in the context what struct you are referring to.

Enums with no payloads anywhere are written and used the same way, which allows you to use enums as C-style enums.
```phi
enum Color { red, green, blue }

let c = Color.red;
```

### Matching a variant

A pattern mirrors construction exactly, with bindings in place of values:

```phi
.circle(r)                 // r: f64
.rectangle(rect)           // rect: Rectangle
.parallelogram((b, h))     // a tuple pattern inside the single payload slot
.square { l }              // `{ l: inner }` to destructure further
.nothing                   // no payload
```

How you match a variant determines what its arms receive, mirroring the by-value / by-`&` / by-`&mut` distinction used for function parameters. For example, matching `shape: &Shape` against `.rectangle(rect)` gives `rect: &Rectangle`, while matching an owned `Shape` would give an owned `Rectangle`.

Matches must be **exhaustive** — every variant must be handled, either explicitly or through the wildcard pattern `_`.

`match` is also usable as an expression, as shown by `perimeter` returning the result of its own `match` directly. For the common case of caring about one variant only, use [`if let`](#if-let-and-while-let).

---

## 11. Traits and Generics

A trait declares behavior a type can provide:

```phi
trait Comparable {
    fun less_than(&self, other: &Self) -> bool;
}
```

A type conforms to a trait with `extend Type with Trait { }`:

```phi
extend Vector2D with Comparable {
    fun less_than(&self, other: &Self) -> bool {
        return self.length() < other.length();
    }
}
```

### Generic functions and bounds

```phi
fun largest<T: Comparable>(a: T, b: T) -> T {
    return if b.less_than(&a) { b } else { a };
}
```

Multiple bounds on the same type parameter are joined with `+` inside the same angle brackets:

```phi
fun describe<T: Comparable + Display>(value: T) { ... }
```

The use of generics also does not effect program performance as the compiler monomorphizes them by default. For example, `largest<Vector2D>` and `largest<i32>` would be generated into and compiled as if they were two separate, ordinary functions.

### Dynamic dispatch with `dyn`

`dyn Trait` is a type that erases the concrete type behind a trait's interface, allowing a single collection to hold several different concrete types at once, which are resolved at runtime:

```phi
fun render_all(shapes: &Array<dyn Shape>) {
    for shape in shapes {
        println(shape.perimeter());
    }
}
```

---

## 12. Operator Overloading

Operators are overloaded by implementing specific traits:

```phi
trait Add {
    fun add(&self, other: &Self) -> Self;
}

extend Vector2D with Add {
    fun add(&self, other: &Self) -> Self {
        return Vector2D { x: self.x + other.x, y: self.y + other.y }
    }
}
```

Indexing works the same way, but is split across two traits: reading from an existing element uses `Index`, while assigning into a key that may not yet exist — such as inserting into a hash map — uses `IndexSet`.

```phi
trait Index<K, V> {
    fun index(&self, key: K) -> &V;
}

trait IndexSet<K, V> {
    fun index_set(&mut self, key: K, value: V);
}
```

---

## 13. Error Handling

Phi has no exceptions. Errors are instead represented as values, using two standard-library enums:

```phi
enum Option<T> {
    some: T,
    none,
}

enum Result<T, E> {
    ok: T,
    err: E,
}
```

A function that can fail returns `Result<T, E>` directly:

```phi
fun read_config(path: &str) -> Result<Config, IoError> {
    let text = fs::read_to_string(path)?;
    return parse_config(&text);
}
```

The `?` operator, applied to a `Result`-valued expression, unwraps the `ok` payload if present; if the value is `.err(e)`, it immediately returns `.err(e)` from the enclosing function. This requires the enclosing function's own return type to be a compatible `Result`.

At the boundary where an error should actually be handled rather than propagated further, match it explicitly:

```phi
match read_config("phi.toml") {
    .ok(cfg)  => start(cfg),
    .err(err) => println("couldn't start: " + err.message()),
};
```

Returning one is the ordinary elided form, since the function's return type supplies the enum:

```phi
fun parse_port(text: &str) -> Result<u16, ParseError> {
    if text.is_empty() {
        return .err(.empty);
    }
    return .ok(80);
}
```

`Option<T>` is used for absence rather than failure, and is matched the same way, with `.some(value)` and `.none` in place of `.ok`/`.err`. When only one variant matters, [`if let`](#if-let-and-while-let) reads better than a full `match`.

---

## 14. Concurrency

Phi guarantees data-race freedom through *value independence*: if no two names can reach the same mutable value at once, then no two threads can either.

```phi
fun total(a: &Array<i32>) -> i32 {
    let mid = a.count / 2;

    // both tasks complete before the block exits
    return concurrent {                          
        let left = spawn { a[..mid].sum() } // you actually can't split an array but it's whatever
        let right = spawn { a[mid..].sum() } // Task<i32>
        
        left.join() + right.join() // this is the return value of the concurrent block
    }
}
```

A `concurrent` block does not exit until every `spawn`ed task inside it has completed, so values assigned inside a task are safe to read immediately after the block.

---

## 15. Modules and Visibility

A module is declared at the top of a file:

```phi
module math;
```

Nested modules are expressed with `::` in the module path, both at the declaration site and on import:

```phi
module math::vector;
```

By default, every item — a struct, function, trait, or enum — is private to the module it's declared in. The `public` keyword exports an item so that other modules may import it. For fields, it allows the field to be accessed outside of the struct.

```phi
module math;

public struct Vector2D {
    public x: f64,
    public y: f64,
}
```

### Importing

```phi
import math::Vector2D;      // a single item
import math::*;              // everything public in the module
import math::vector::Line;    // a nested module path

fun main() {
    let v = Vector2D { x: 1.0, y: 1.0 };
}
```

---
