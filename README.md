# The Phi Programming Language

Phi is a modern programming language written in C++23. To guarantee memory saftey and data-race freedom, Phi uses *mutable value semantics (MVS)*.
What this looks like for the programmer is very similar to existing langauges like Rust; for trivially copyable types such as integers, floating point numbers, booleans, and structs containing only these types, copies happen implicitly. For other types, move is the default, as in Rust. 
However, a key difference is that references are treated as second-class objects and cannot be stored, eliminating the need for lifetime annotations. Phi has a variety of different constructs to making programming with this restriction ergonomic.

## Hello, World!

Every Phi program begins with a `main` function:

```phi
fun main() {
    println("Hello, world!");
}
```

Here `println` is a function that prints its argument to standard output.

---

## Variables

Variables are by-default declared immutable with `let`. To declare a mutable variable, add `mut` after `let`.

```phi
fun main() {
    let foo = 0;            // immutable integer
    let mut bar = 1;        // mutable integer
    let mut phi: f64 = 1.618; // variable with type annotation
}
```

Phi uses **type inference**: the compiler figures out the type of `foo` and `bar` based on their initial values and uses later in the program. Type annotations may be provided as seen with the variable `phi`.

---

## Functions

Functions are declared with the `fun` keyword:

```phi
fun foo() {

}
```

A function can take parameters and return values:

```phi
fun add(x: i32, y: i32) -> i32 {
    return x + y;
}
```
For trivally copyable types, parameters are passed by value. Returns are not guaranteed to be this way due to RVO. Functions are required to have their parameter types and return types annotated.
For non-trivially copyable types, parameters can be passed through a variety of methods:

1. We can pass the parameters through immutable borrow. This works the same as it does in Rust.
```phi
fun add(x: &T, y: &T) {

}
```

2. We can pass the parameters through mutable borrow. This also works the same as it does in Rust.
```phi
fun add(x: &mut T, y: &mut T) {

}
```

3. We can transfer ownership to the function's parameters. 
```phi
fun framed(base: String) -> String {
    base + "!"                       // base may escape into the result — it's ours
}

let title = framed(greeting)         // greeting is gone now (String isn't Trivial)
```


## Projections Projecting Functions
Phi's restriction of references mean that references cannot be returned from functions nor stored into variables. Indeed, functions must own the value which it attempts to return, and variables must own the value they store.
However, it is often use to be able to store and return references and. To accomplish this, Phi has projections and projecting functions.

Projections offer a way to borrow a value or part of a value. While the projection lives, the original variable cannot be mutated or destroyed. If the projection is mutable, we cannot read the source anymore until the projection's lifetime ends. If the projection is immutable, we cannot use it to write to the source variable, but we can read the source variable elsewhere.

```phi
fun main() {
    let x = ...;
    let mut y = &mut x; // as long as y lives, x cannot be read, destroyed, or mutated
                        // if y was instead projected mutably, then it can be read only
}
```

Projecting functions then allow us to use logic to assign projections, basically allowing us to return references. 

```phi
fun min(x: any T, y: any T): &T { // note that we use `:` instead of `->`
    if x < y { 
        yield &x; 
    } else { 
        yield &y; 
    }
}

fun min(x: any T, y: any T): &mut T { // note that we can also use mutable projections
    if x < y { 
        yield &mut x; 
    } else { 
        yield &mut y; 
    }
}

fun min(x: any T, y: any T): T { // and we can return a yielded value, 
    if x < y {                   // although this isn't really the job or projecting functions
        yield x; 
    } else { 
        yield y; 
    }
}
```
As long as the projected value lives (which can be only for a line if used inline), the source variables are subject to the same rules as local projections.
Note that code can also run after the yield, making clean ups easy.

## Control Flow

Phi provides familiar control-flow constructs:

### While loops

```phi
var i = 0;
while i < 5 {
    i = i + 1;
}
```

### For loops

```phi
var sum = 0;
for i in 0..5 {
    sum += i;
}
```
Looping through data structures work through projections:
```phi
for x in &a {
    // immutable projection
}

for x in &mut a {
    // mutable projection
}
```


### If statements

```phi
if x < 5 {
    println("less than 5");
}
```

---

## Structs and Methods

Phi supports user-defined types with `struct`:

```phi
struct Vector2D {
    public x: f64;
    public y: f64;

    public fun dot(&self, other: &Vector2D) -> i32 {
        return self.x * other.x + self.y * other.y;
    }
}
```

Here we define an `Vector2D` struct with two float fields. Methods are declared inside structs, and the first parameter is always `self`, which refers to the instance. As with regular function parameters, `self` can be a mutable reference, immutable reference, or take ownership of the instance. Note that methods can also be projecting.

You can construct a struct with field initializers:

```phi
let force = Vector2D { x = 1.0, y = 1.0 };
```

And access fields or call methods:

```phi
force.x;                 // field access
force.dot(other_force);  // method call
```

## Enums

Phi is not an object oriented programming language. There is no inheritance. Instead, polymorphism can be achieved through sum types. This is done in phi through enums and pattern matching.

Example:

```phi
struct Rectangle {
    public l: f64;
    public w: f64;
}

enum Shape {
    Rectangle: Rectangle;
    Circle: f64;
    Square: { l: f64 };
    Parallelogram: (f64, f64);

    fun perimeter(&self) -> f64 {
        return match &self {
            .Rectangle(rect)     => 2 * (rect.l + rect.w),
            .Circle(r)           => 2 * 3.14 * r,
            .Square(l)           => 4 * l,
            .Parallelogram(b, h) => 2 * (b + h),
        };
    }
}

fun print_name(shape: &Shape) {
    match &shape {
        .Rectangle     => println("rectangle"),
        .Circle        => println("circle"),
        .Square        => println("square"),
        .Parallelogram => println("parallelogram")
    };
}
```

## 9. Traits, generics, and `dyn`

```phi
trait Comparable {
    fun less_than(&self, other: &Self) -> bool
}

impl Comparable for Vector2 {
    fun less_than(&self, other: &Self) -> bool {
        self.length() < other.length()
    }
}

fun largest<T: Comparable>(a: T, b: T) -> T {
    if b.less_than(&a) { b } else { a }
}
```
Generics are checked at the definition and monomorphized by default. For runtime polymorphism, use `dyn Trait`.

## 10. Concurrency

Phi guarantees data-race freedom due to *value independence*: if no two names can reach the same mutable value, neither can two threads.

```phi
fun total(a: &Array<i32>) -> i32 {
    let mid = a.count / 2
    let mut left = 0
    let mut right = 0
    concurrent {                          // both tasks complete before the block exits
        spawn { left = a[..mid].sum() }
        spawn { right = a[mid..].sum() }
    }
    left + right
}
```

## Modules

Phi uses modules for imports and exports. A module is given at the top of a file, and the `public` keyword is used to export a symbol:

```phi
module math;

public struct Vector2D {
    public x: f64;
    public y: f64;
}

```

The contents of the module above can be imported like so:

```phi
import math::Vector2D;

fun main() {
    let v = Vector2D { x: 1.0, y: 1.0 };
}

```

## Operator overloading

Phi allows the overloading of operators through traits:

```phi
trait Add {
    fun add(&self, other: &Self) -> Self {}
}
```
