# The Phi Programming Language — Language Reference

Phi is a modern programming language written in C++23. To guarantee memory safety and data-race freedom, Phi uses *mutable value semantics (MVS)*.

For trivially copyable types such as integers, floating-point numbers, booleans, and structs containing only these types, copies happen implicitly. For other types, move is the default.

References are treated as **second-class objects** and cannot be stored. Phi has a variety of constructs — projections, functions that return them, and the `any` keyword — that make programming with this restriction ergonomic.

This document is a complete reference to the language: its syntax and its semantics.

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

A single file is compiled and run with:

```
phi run hello.phi
```

A multi-file project is organized around a `phi.toml` manifest at its root, with source files nested beneath a `src/` directory. `phi build` produces a binary; `phi run` builds and executes it in one step.

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
"hello"     // String
```

---

## 3. Variables and Bindings

Variables are declared immutable by default with `let`. To declare a mutable variable, add `mut` after `let`.

```phi
fun main() {
    let foo = 0;              // immutable integer
    let mut bar = 1;           // mutable integer
    let mut phi: f64 = 1.618;  // mutable, with an explicit type annotation
}
```

Phi uses **type inference**: the compiler determines the type of `foo` and `bar` from their initial values and later uses. A type annotation, as seen with `phi`, is optional but may be supplied for clarity.

Rebinding a `let` variable, or reading a variable after it has moved or been dropped, is a compile error.

---

## 4. Basic Types

| Category | Types |
|---|---|
| Signed integers | `i8`, `i16`, `i32`, `i64` |
| Unsigned integers | `u8`, `u16`, `u32`, `u64` |
| Floating point | `f32`, `f64` |
| Boolean | `bool` |
| Character | `char` (a single Unicode scalar value) |
| Text | `String` (owned, growable), `&str` (a projected view of text) |
| Grouping | tuples `(T, U, ...)`, arrays `Array<T>`, fixed-size arrays `[T; N]` |

Integers, floats, `bool`, and `char` are trivially copyable. `String` and `Array<T>` are not — they own a heap allocation and move by default.

Tuples are constructed and destructured positionally:

```phi
let point: (f64, f64) = (1.0, 2.0);
let (x, y) = point;
```

---

## 5. Copies, Moves, and Drops

Trivially copyable types are implicitly copied. Every other type moves by default. A moved-from variable cannot be used again, and a value is destroyed at the end of its lifetime — the last time it is used, not necessarily the end of its enclosing scope.

```phi
let a = String::from("hi");
let b = a;      // moves; `a` is no longer usable
// println(a);  // compile error: use of moved value
println(b);     // fine
```

A type that owns a resource — a file handle, a socket, a heap buffer — implements the `Drop` trait to define what happens when its last use has passed:

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

Parameter types and return types must always be annotated; only local variable types are inferred. For trivially copyable types, parameters are passed by value. Returns are not guaranteed to copy or move in any particular observable way, since return value optimization (RVO) may construct the result directly in the caller's storage.

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

**3. Ownership transfer** — the callee receives the value outright and may move it, mutate it, return it, or let it drop:

```phi
fun framed(base: String) -> String {
    return base + "!";                 // base may escape into the result — it's ours
}

let title = framed(greeting);  // greeting is gone now (String isn't trivial)
```

### Closures

Closures are written the same way as in Rust — parameters between `|...|`, followed by the body:

```phi
let add = |x: i32, y: i32| -> i32 { x + y };
let double = |x| x * 2;      // parameter and return types are inferred when omitted
let answer = || 42;          // no parameters
```

The body is any expression: a `{ ... }` block whose final, semicolon-less expression is the result (exactly like a function body without `return`), or a single bare expression for short closures. Parameter types and the return type are optional and inferred from context; annotate them the same way a function's parameters and return type are annotated when inference isn't enough or clarity is wanted.

Closures are ordinary values and are most often passed directly to a function that takes one:

```phi
let doubled = xs.map(|x| x * 2);
```

---

## 7. Projections

References cannot be returned from ordinary functions or stored into variables — a function must own any value it returns, and a variable must own any value it holds. **Projections** are the construct that makes it possible to still borrow, and to return borrowed data, without lifting that restriction.

### Projections

A projection borrows a value, or part of a value, in place. While a projection lives:

- If it is **mutable**, the source cannot be read, mutated, or destroyed through any other name.
- If it is **immutable**, the source cannot be mutated or destroyed, but it — or other immutable projections of it — may still be read.

A value has either many readers or one writer, never both at once. Projections of *disjoint* parts of a value — two different fields of the same struct, for instance — may coexist independently. By default, a projection's lifetime ends at its last use, exactly like an ordinary variable's:

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

Every binding in a `with` block ends at the closing brace, regardless of where its own last use happened to fall inside the block. This differs from inferred last-use in a way that matters as code changes over time: with last-use, a binding's lock on its source is computed from wherever its final mention is, which means adding a later use during maintenance silently extends how long the source stays locked. A `with` block's boundary can't drift like that — `px` and `py` simply don't exist past the brace, so a later access to `point.x` requires a fresh binding rather than an accidental extension of the old one.

### Functions that return a projection

A function's return type can be `&T`, `&mut T`, or `any T`, exactly as a parameter's can — in which case the function hands back a projection into one of its own parameters, chosen by ordinary control flow, rather than an owned value. There is no separate declaration form for this: it's the same `fun`, `->`, and `return` as any other function, and the return type alone determines the behavior.

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

As long as the returned value lives — which may be a single line, if it is used inline at the call site — the source parameters are subject to the same aliasing rules as any local projection. The function itself ends at `return`, exactly like any other function; any cleanup tied to the projection's source is expressed the ordinary way, through `Drop` on whatever guard or handle is being returned.

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

Looping over a data structure goes through projections, and the loop's binding mode is written explicitly at the `in`:

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

---

## 9. Structs and Methods

Phi supports user-defined types with `struct`. Fields, and any methods that belong inherently to the type, are declared together in the struct body:

```phi
struct Vector2D {
    public x: f64,
    public y: f64,

    public fun dot(&self, other: &Vector2D) -> f64 {
        return self.x * other.x + self.y * other.y;
    }
}
```

For non-static methods, the first parameter is always `self`, which may be an immutable borrow (`&self`), a mutable borrow (`&mut self`), or an owned value (`self`). Static methods omit `self` entirely. As with any function, a method's return type can be `&T`, `&mut T`, or `any T`, in which case it hands back a projection following the same rules described in section 7.

Construct a struct with field initializers:

```phi
let force = Vector2D { x: 1.0, y: 1.0 };
```

Access fields or call methods with `.`:

```phi
force.x;                   // field access
force.dot(other_force);    // method call
```

### `extend`: adding methods after the fact

A struct's methods cannot live in its body. The `extend` construct reopens a type to add more methods:

**Splitting inherent methods across a file or module**, with no trait involved:

```phi
extend Vector2D {
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

A given type may have as many plain `extend Type { }` blocks as convenient. For `extend Type with Trait { }`, there may be exactly one such block for a given `(Type, Trait)` pair anywhere in a program.

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

Phi is not an object-oriented language — there is no inheritance. Polymorphism is achieved through sum types: enums, combined with exhaustive pattern matching.

```phi
struct Rectangle {
    public l: f64,
    public w: f64,
}

enum Shape {
    Rectangle: Rectangle,
    Circle: f64,
    Square: { l: f64 },
    Parallelogram: (f64, f64),
}

extend {
    fun perimeter(&self) -> f64 {
        return match self {
            Rectangle(rect)     => 2 * (rect.l + rect.w),
            Circle(r)           => 2 * 3.14 * r,
            Square(l)           => 4 * l,
            Parallelogram(b, h) => 2 * (b + h),
        };
    }
}

fun print_name(shape: &Shape) {
    match shape {
        Rectangle     => println("rectangle"),
        Circle        => println("circle"),
        Square        => println("square"),
        Parallelogram => println("parallelogram"),
    };
}

fun main() {
    const rect = Shape { Rectangle: { l = 4.0, w = 6.0 } }; // we can elide in cases where it's clear what the type of the ctor is
    const circle = Shape { Circle: 4.0 };
    const square = Shape { Square: .{ l = 4.0 } };

```

A variant is written `Name: Type,` (a payload of an existing type), `Name: { field: Type },` (an inline struct-like payload), `Name: (T, U),` (a tuple payload), or bare `Name,` (no payload).

How you match a variant determines what its arms receive, mirroring the by-value / by-`&`/ by-`&mut` distinction used for function parameters: matching `shape: &Shape` against `Rectangle(rect)` gives `rect: &Rectangle`, while matching an owned `Shape` would give an owned `Rectangle`.

Matches must be **exhaustive** — every variant must be handled, either explicitly or through the wildcard pattern `_`.

`match` is also usable as an expression, as shown by `perimeter` returning the result of its own `match` directly.

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

Generics are checked at their definition site and monomorphized by default: `largest<Vector2D>` and `largest<i32>` are compiled as if they were two separate, ordinary functions.

### Dynamic dispatch with `dyn`

`dyn Trait` is a type that erases the concrete type behind a trait's interface, allowing a single collection to hold several different concrete types at once, resolved at runtime:

```phi
fun render_all(shapes: &Array<dyn Shape>) {
    for shape in shapes {
        println(shape.perimeter());
    }
}
```

---

## 12. Operator Overloading

Operators are overloaded by implementing specific, recognized traits — there is no separate marker on the method itself:

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

Phi has no exceptions. Failure is represented as an ordinary value, using two standard-library enums:

```phi
enum Option<T> {
    Some: T,
    None,
}

enum Result<T, E> {
    Ok: T,
    Err: E,
}
```

A function that can fail returns `Result<T, E>` directly:

```phi
fun read_config(path: &str) -> Result<Config, IoError> {
    let text = fs::read_to_string(path)?;
    return parse_config(&text);
}
```

The `?` operator, applied to a `Result`-valued expression, unwraps the `Ok` payload if present; if the value is `Err(e)`, it immediately returns `Err(e)` from the enclosing function. This requires the enclosing function's own return type to be a compatible `Result`.

At the boundary where an error should actually be handled rather than propagated further, match it explicitly:

```phi
match read_config("phi.toml") {
    Ok(cfg)  => start(cfg),
    Err(err) => println("couldn't start: " + err.message()),
};
```

`Option<T>` is used for absence rather than failure, and is matched the same way, with `Some(value)` and `None` in place of `Ok`/`Err`.

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

By default, every item — a struct, function, trait, or enum — is private to the module it's declared in. The `public` keyword exports an item so that other modules may import it:

```phi
module math;

public struct Vector2D {
    public x: f64,
    public y: f64,
}
```

Visibility is tracked per-field as well as per-item: a `struct` may be `public` while only some of its fields are, restricting outside modules to whichever fields and methods are explicitly marked `public`.

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

## 16. Standard Library Basics

A few types are available without an explicit `import`:

| Type | Purpose |
|---|---|
| `Array<T>` | A growable, owned sequence of `T`. Indexing (`a[i]`), slicing (`a[..n]`, `a[n..]`), and iteration all go through the projection rules from section 7. |
| `String` | An owned, growable, UTF-8 text buffer. `+` concatenates; `&str` is the borrowed view used for parameters that only read text. |
| `Option<T>` | Introduced in section 13: presence (`Some(value)`) or absence (`None`). |
| `Result<T, E>` | Introduced in section 13: success (`Ok(value)`) or failure (`Err(err)`). |
| `HashMap<K, V>` | A hash-based associative map. Reads use `Index`; writes to a possibly-absent key use `IndexSet` (section 12). |

Common trait conformances are provided for these types out of the box — `Array<T>` and `String` both conform to `Drop`, and any `T: Comparable` type can be sorted or compared without additional code.

---
