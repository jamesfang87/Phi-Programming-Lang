# The Phi Programming Language

Phi is a modern programming language written in C++23.

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

Variables are declared with `var` (mutable) or `const` (immutable):

```phi
fun main() {
    var phi  = 0;       // mutable integer
    const foo = 10;    // immutable integer
    const var: i64 = 11;

}
```

Phi uses **type inference**: the compiler figures out the type of `phi` and `lambda` based on their initial values. Type annotations may be provided as seen with the variable `bar`.

---

## Functions

Functions are declared with the `fun` keyword:

```phi
fun foo() {
    var i = 0;
    while i < 10 {
        if i == 5 {
            break;
        }
        i = i + 1;
    }
}
```

A function can take parameters and return values:

```phi
fun add(x: i32, y: i32) -> i32 {
    return x + y;
}
```

Functions are required to have their parameter types and and return types annotated.

## Control Flow

Phi provides familiar control-flow constructs:

### While loops

```phi
var i = 0;
while (i < 5) {
    println(i);
    i = i + 1;
}
```

### For loops

```phi
for i in 0..5 {
    println(i);
}
```

### If statements

```phi
if x < 5 {
    println("less than");
}
```

---

## Structs and Methods

Phi supports user-defined types with `struct`:

```phi
struct RGB {
    r: i32;
    g: i32;
    b: i32;

    public fun compareRed(const this, const other: i32) -> i32 {
        return this.r - other;
    }
}
```

Here we define an `RGB` struct with three integer fields. Methods are declared inside structs, and the first parameter is always `this`, which refers to the instance. `this` can be prefixed with const or var, based on whether the method will mutate the internal contents of the struct or not.

You can construct a struct with field initializers:

```phi
const color = RGB { r = 255, g = 255, b = 255 };
```

And access fields or call methods:

```phi
println(color.r);              // field access
println(color.compareRed(7));  // method call
```
