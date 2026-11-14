mod support;

fn run(name: &str, source: &str, expected: &str) {
    let output = support::run_src("examples", name, source);
    support::expect_stdout(&output, expected, name);
}

fn run_project(name: &str, files: &[(&str, &str)], expected: &str) {
    let output = support::run_files("examples", name, files);
    support::expect_stdout(&output, expected, name);
}

#[test]
fn fizzbuzz_prints_the_classic_sequence() {
    run(
        "fizzbuzz",
        r#"module app;
fun main() {
    for i in 1..16 {
        if i % 15 == 0 { println("FizzBuzz"); }
        else if i % 3 == 0 { println("Fizz"); }
        else if i % 5 == 0 { println("Buzz"); }
        else { println(i); }
    }
}
"#,
        "1\n2\nFizz\n4\nBuzz\nFizz\n7\n8\nFizz\nBuzz\n11\nFizz\n13\n14\nFizzBuzz\n",
    );
}

#[test]
fn fibonacci_prints_intermediate_values() {
    run(
        "fibonacci",
        r#"module app;
fun fib(n: i32) -> i32 {
    if n < 2 { return n; }
    return fib(n - 1) + fib(n - 2);
}
fun main() {
    for i in 0..10 {
        print("fib(");
        print(i);
        print(") = ");
        println(fib(i));
    }
}
"#,
        "fib(0) = 0\nfib(1) = 1\nfib(2) = 1\nfib(3) = 2\nfib(4) = 3\nfib(5) = 5\nfib(6) = 8\nfib(7) = 13\nfib(8) = 21\nfib(9) = 34\n",
    );
}

#[test]
fn prime_sieve_prints_primes_below_thirty() {
    run(
        "prime_sieve",
        r#"module app;
fun main() {
    let mut sieve = new [true; 30];
    let mut i = 2;
    while i < 30 {
        if (*sieve)[i] {
            print(i);
            print(" ");
            let mut j = i * i;
            while j < 30 {
                (*sieve)[j] = false;
                j += i;
            }
        }
        i += 1;
    }
    println("");
}
"#,
        "2 3 5 7 11 13 17 19 23 29 \n",
    );
}

#[test]
fn string_builder_combines_chars_bytes_and_numbers() {
    run(
        "string_builder",
        r#"module app;
import std::string::String;
fun main() {
    let mut text = String::from("hello");
    text.push_char(' ');
    text.push_str("world");
    text.push_str(" #");
    text.push_u64(42);
    println(&text);
    println(text.len());
    let other = String::from("hello world #42");
    println(text == other);
    println(text < other);
    let joined = String::from("left") + String::from("right");
    println(joined);
}
"#,
        "hello world #42\n15\ntrue\nfalse\nleftright\n",
    );
}

#[test]
fn vector_stack_pops_in_reverse_order() {
    run(
        "vector_stack",
        r#"module app;
import std::vector::Vector;
fun main() {
    let mut stack: Vector<i32> = Vector::empty();
    for i in 1..6 { stack.push(i * i); }
    println(stack.len());
    println(stack[3]);
    while !stack.is_empty() {
        let top = stack.pop().unwrap();
        print(top);
        if !stack.is_empty() { print(","); }
    }
    println("");
}
"#,
        "5\n16\n25,16,9,4,1\n",
    );
}

#[test]
fn option_and_result_pipelines_compose() {
    run(
        "option_result_pipeline",
        r#"module app;
import core::option::Option;
import core::result::Result;

fun half(n: i32) -> Option<i32> {
    if n % 2 == 0 { return .some(n / 2); }
    return .none;
}

fun add_halves(a: i32, b: i32) -> Option<i32> {
    let sum = half(a)? + half(b)?;
    return .some(sum);
}

fun main() {
    println(half(20).and_then(half).map(|x| x * 3).unwrap_or(-1));
    println(half(7).unwrap_or(-1));
    println(add_halves(20, 12).unwrap_or(-1));
    println(add_halves(20, 13).unwrap_or(-1));
    let result: Result<i32, i32> = .ok(7);
    println(result.map(|x| x + 1).unwrap_or(0));
    let failed: Result<i32, i32> = .err(5);
    println(failed.map(|x| x + 1).unwrap_or(0));
}
"#,
        "15\n-1\n16\n-1\n8\n0\n",
    );
}

#[test]
fn matrix_uses_nested_vectors() {
    run(
        "matrix",
        r#"module app;
import std::vector::Vector;

fun main() {
    let mut table: Vector<Vector<i32>> = Vector::empty();
    for row in 0..3 {
        let mut line: Vector<i32> = Vector::empty();
        for col in 0..3 {
            line.push((row + 1) * (col + 1));
        }
        table.push(line);
    }

    for row in 0..3 {
        for col in 0..3 {
            print(table[row][col]);
            print(" ");
        }
        println("");
    }
}
"#,
        "1 2 3 \n2 4 6 \n3 6 9 \n",
    );
}

#[test]
fn generic_display_showcase_prints_every_builtin() {
    run(
        "generic_display",
        r#"module app;
import std::fmt::Display;

fun show<T: Display>(label: str, value: T) {
    print(label);
    print(": ");
    println(value);
}

fun main() {
    show("int", 42);
    show("bool", true);
    show("float", 2.5);
    show("char", 'φ');
    show("text", "hello");
}
"#,
        "int: 42\nbool: true\nfloat: 2.5\nchar: φ\ntext: hello\n",
    );
}

#[test]
fn temperature_table_formats_floats() {
    run(
        "temperature_table",
        r#"module app;
fun main() {
    for c in 0..5 {
        let celsius: i32 = c;
        let f = (celsius as f64) * 9.0 / 5.0 + 32.0;
        print(celsius);
        print("C = ");
        print(f);
        println("F");
    }
}
"#,
        "0C = 32F\n1C = 33.8F\n2C = 35.6F\n3C = 37.4F\n4C = 39.2F\n",
    );
}

#[test]
fn collatz_prints_step_counts() {
    run(
        "collatz",
        r#"module app;
fun collatz_length(start: i64) -> i32 {
    let mut n = start;
    let mut steps = 0;
    while n != 1 {
        if n % 2 == 0 { n = n / 2; }
        else { n = 3 * n + 1; }
        steps += 1;
    }
    return steps;
}

fun main() {
    for i in 1..11 {
        print(i);
        print(" -> ");
        println(collatz_length(i));
    }
    println(collatz_length(27));
}
"#,
        "1 -> 0\n2 -> 1\n3 -> 7\n4 -> 2\n5 -> 5\n6 -> 8\n7 -> 16\n8 -> 3\n9 -> 19\n10 -> 6\n111\n",
    );
}

#[test]
fn vowel_count_scans_string_bytes() {
    run(
        "vowel_count",
        r#"module app;
fun main() {
    let text = "the quick brown fox jumps over the lazy dog";
    let bytes = text as &[u8];
    let mut vowels = 0;
    for i in 0..text.len() {
        let b = bytes[i];
        if b == 97_u8 || b == 101_u8 || b == 105_u8 || b == 111_u8 || b == 117_u8 {
            vowels += 1;
        }
    }
    println(vowels);
}
"#,
        "11\n",
    );
}

#[test]
fn multi_file_stats_project_uses_std_types() {
    run_project(
        "multi_file_stats",
        &[
            (
                "src/main.phi",
                r#"module app;
import stats::maximum;
import stats::sum;
import std::vector::Vector;
import text::exclaim;

fun main() {
    let mut values: Vector<i32> = Vector::empty();
    values.push(4);
    values.push(8);
    values.push(15);
    values.push(16);
    values.push(23);
    values.push(42);
    println(sum(&values));
    println(maximum(&values));
    println(exclaim("total"));
}
"#,
            ),
            (
                "src/stats.phi",
                r#"module stats;
import std::vector::Vector;

public fun sum(values: &Vector<i32>) -> i32 {
    let mut total = 0;
    for i in 0..values.len() {
        total += *values[i];
    }
    return total;
}

public fun maximum(values: &Vector<i32>) -> i32 {
    let mut best = *values[0];
    for i in 1..values.len() {
        if *values[i] > best {
            best = *values[i];
        }
    }
    return best;
}
"#,
            ),
            (
                "src/text.phi",
                r#"module text;
import std::string::String;

public fun exclaim(text: str) -> String {
    let mut out = String::from(text);
    out.push_char('!');
    return out;
}
"#,
            ),
        ],
        "108\n42\ntotal!\n",
    );
}

#[test]
fn custom_display_formats_a_temperature_struct() {
    run(
        "custom_display_temperature",
        r#"module app;
import std::fmt::Display;
import std::string::String;

struct Temperature {
    public celsius: f64,
}

extend Temperature with Display {
    fun fmt(&self, out: &mut String) {
        out.push_f64(self.celsius);
        out.push_str("C");
    }
}

fun main() {
    println(Temperature { celsius: 21.5 });
    println(Temperature { celsius: -3.0 });
    println(Temperature { celsius: 0.0 });
}
"#,
        "21.5C\n-3C\n0C\n",
    );
}

#[test]
fn bank_account_uses_methods_and_result() {
    run(
        "bank_account",
        r#"module app;
import core::result::Result;
import std::fmt::Display;
import std::string::String;

struct Account {
    public balance: i32,
}

extend Account {
    public fun deposit(&mut self, amount: i32) {
        self.balance += amount;
    }

    public fun withdraw(&mut self, amount: i32) -> Result<i32, str> {
        if amount > self.balance {
            return .err("insufficient funds");
        }
        self.balance -= amount;
        return .ok(amount);
    }
}

extend Account with Display {
    fun fmt(&self, out: &mut String) {
        out.push_i64(self.balance as i64);
    }
}

fun main() {
    let mut account = Account { balance: 100 };
    account.deposit(50);
    println(&account);
    match account.withdraw(30) {
        .ok(amount) => println(amount),
        .err(message) => println(message),
    };
    println(&account);
    match account.withdraw(500) {
        .ok(amount) => println(amount),
        .err(message) => println(message),
    };
    println(&account);
}
"#,
        "150\n30\n120\ninsufficient funds\n120\n",
    );
}

#[test]
fn dyn_dispatch_prints_areas_through_a_trait_object() {
    run(
        "dyn_shapes",
        r#"module app;

trait Shape {
    fun area(&self) -> f64;
}

struct Circle {
    public radius: f64,
}

struct Rect {
    public width: f64,
    public height: f64,
}

extend Circle with Shape {
    fun area(&self) -> f64 {
        return 3.14159 * self.radius * self.radius;
    }
}

extend Rect with Shape {
    fun area(&self) -> f64 {
        return self.width * self.height;
    }
}

fun describe(label: str, shape: &dyn Shape) {
    print(label);
    print(": ");
    println(shape.area());
}

fun main() {
    let circle = Circle { radius: 1.0 };
    let rect = Rect { width: 3.0, height: 2.0 };
    describe("circle", &circle);
    describe("rect", &rect);
}
"#,
        "circle: 3.14159\nrect: 6\n",
    );
}

#[test]
fn caesar_cipher_round_trips_ascii_text() {
    run(
        "caesar_cipher",
        r#"module app;
import std::string::String;

fun shift_text(text: str, shift: u8) -> String {
    let bytes = text as &[u8];
    let mut out = String::empty();
    for i in 0..text.len() {
        let b = bytes[i];
        if b >= 97_u8 && b <= 122_u8 {
            let offset = (b - 97_u8 + shift) % 26_u8;
            out.push_char((offset + 97_u8) as char);
        } else if b >= 65_u8 && b <= 90_u8 {
            let offset = (b - 65_u8 + shift) % 26_u8;
            out.push_char((offset + 65_u8) as char);
        } else {
            out.push_char(b as char);
        }
    }
    return out;
}

fun main() {
    let encoded = shift_text("Hello, World!", 3);
    println(&encoded);
    let decoded = shift_text(encoded.as_str(), 23);
    println(&decoded);
}
"#,
        "Khoor, Zruog!\nHello, World!\n",
    );
}

#[test]
fn gcd_and_lcm_use_recursion() {
    run(
        "gcd_lcm",
        r#"module app;

fun gcd(a: i32, b: i32) -> i32 {
    if b == 0 {
        return a;
    }
    return gcd(b, a % b);
}

fun lcm(a: i32, b: i32) -> i32 {
    return a / gcd(a, b) * b;
}

fun main() {
    println(gcd(48, 18));
    println(lcm(4, 6));
    println(gcd(17, 13));
    println(lcm(21, 6));
}
"#,
        "6\n12\n1\n42\n",
    );
}

#[test]
fn bubble_sort_orders_a_fixed_array() {
    run(
        "bubble_sort",
        r#"module app;

fun main() {
    let mut values = new [0; 6];
    (*values)[0] = 5;
    (*values)[1] = 3;
    (*values)[2] = 6;
    (*values)[3] = 1;
    (*values)[4] = 4;
    (*values)[5] = 2;

    let mut i: usize = 0;
    while i < 6 {
        let mut j: usize = 0;
        while j < 5 - i {
            if (*values)[j] > (*values)[j + 1] {
                let tmp = (*values)[j];
                (*values)[j] = (*values)[j + 1];
                (*values)[j + 1] = tmp;
            }
            j += 1;
        }
        i += 1;
    }

    for k in 0..6 {
        print((*values)[k]);
        print(" ");
    }
    println("");
}
"#,
        "1 2 3 4 5 6 \n",
    );
}

#[test]
fn binary_search_returns_the_matching_index_or_none() {
    run(
        "binary_search_option",
        r#"module app;
import core::option::Option;
import std::vector::Vector;

fun search(values: &Vector<i32>, key: i32) -> Option<usize> {
    let mut lo: usize = 0;
    let mut hi = values.len();
    while lo < hi {
        let mid = (lo + hi) / 2;
        if *values[mid] == key {
            return .some(mid);
        } else if *values[mid] < key {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    return .none;
}

fun main() {
    let mut values: Vector<i32> = Vector::empty();
    values.push(2);
    values.push(4);
    values.push(6);
    values.push(8);
    values.push(10);
    values.push(12);
    values.push(14);

    println(search(&values, 10).unwrap_or(999));
    println(search(&values, 2).unwrap_or(999));
    println(search(&values, 14).unwrap_or(999));
    println(search(&values, 5).unwrap_or(999));
}
"#,
        "4\n0\n6\n999\n",
    );
}

#[test]
fn pascals_triangle_uses_nested_vectors() {
    run(
        "pascals_triangle",
        r#"module app;
import std::vector::Vector;

fun main() {
    let mut rows: Vector<Vector<i32>> = Vector::empty();
    for n in 0..6 {
        let mut row: Vector<i32> = Vector::empty();
        for k in 0..(n + 1) {
            let value = if k > 0 && k < n {
                *rows[n - 1][k - 1] + *rows[n - 1][k]
            } else {
                1
            };
            row.push(value);
        }
        rows.push(row);
    }

    for n in 0..6 {
        for k in 0..rows[n].len() {
            print(rows[n][k]);
            print(" ");
        }
        println("");
    }
}
"#,
        "1 \n1 1 \n1 2 1 \n1 3 3 1 \n1 4 6 4 1 \n1 5 10 10 5 1 \n",
    );
}

#[test]
fn palindrome_check_compares_bytes_from_both_ends() {
    run(
        "palindrome",
        r#"module app;

fun is_palindrome(text: str) -> bool {
    let bytes = text as &[u8];
    let mut left: usize = 0;
    let mut right = text.len();
    while left < right {
        right -= 1;
        if bytes[left] != bytes[right] {
            return false;
        }
        left += 1;
    }
    return true;
}

fun main() {
    println(is_palindrome("racecar"));
    println(is_palindrome("level"));
    println(is_palindrome("hello"));
    println(is_palindrome(""));
}
"#,
        "true\ntrue\nfalse\ntrue\n",
    );
}

#[test]
fn vector_of_strings_owns_and_pops_text() {
    run(
        "vector_of_strings",
        r#"module app;
import std::string::String;
import std::vector::Vector;

fun main() {
    let mut names: Vector<String> = Vector::empty();
    names.push(String::from("alpha"));
    names.push(String::from("beta"));
    names.push(String::from("gamma"));
    println(names.len());
    while !names.is_empty() {
        let name = names.pop().unwrap();
        println(&name);
    }
}
"#,
        "3\ngamma\nbeta\nalpha\n",
    );
}

#[test]
fn higher_order_map_and_fold_use_closures() {
    run(
        "higher_order_map_fold",
        r#"module app;
import std::vector::Vector;

fun map(values: &Vector<i32>, f: fun(i32) -> i32) -> Vector<i32> {
    let mut out: Vector<i32> = Vector::empty();
    for i in 0..values.len() {
        out.push(f(*values[i]));
    }
    return out;
}

fun fold(values: &Vector<i32>, init: i32, f: fun(i32, i32) -> i32) -> i32 {
    let mut acc = init;
    for i in 0..values.len() {
        acc = f(acc, *values[i]);
    }
    return acc;
}

fun main() {
    let mut values: Vector<i32> = Vector::empty();
    for i in 1..6 {
        values.push(i);
    }
    let doubled = map(&values, |x| x * 2);
    println(fold(&doubled, 0, |a, b| a + b));
    let squares = map(&values, |x| x * x);
    for i in 0..squares.len() {
        print(squares[i]);
        print(" ");
    }
    println("");
    println(fold(&values, 1, |a, b| a * b));
}
"#,
        "30\n1 4 9 16 25 \n120\n",
    );
}

#[test]
fn insertion_sort_mutates_a_vector_in_place() {
    run(
        "insertion_sort",
        r#"module app;
import std::vector::Vector;

fun main() {
    let mut values: Vector<i32> = Vector::empty();
    values.push(5);
    values.push(3);
    values.push(6);
    values.push(1);
    values.push(4);
    values.push(2);

    for i in 1..values.len() {
        let key = *values[i];
        let mut j = i;
        while j > 0 && *values[j - 1] > key {
            let previous = *values[j - 1];
            values[j] = previous;
            j -= 1;
        }
        values[j] = key;
    }

    for i in 0..values.len() {
        print(values[i]);
        print(" ");
    }
    println("");
}
"#,
        "1 2 3 4 5 6 \n",
    );
}

#[test]
fn to_string_formats_several_types_into_one_line() {
    run(
        "to_string_format",
        r#"module app;
import std::fmt::to_string;
import std::string::String;

fun main() {
    let number = 42;
    let flag = true;
    let ratio = 1.5;
    let letter = 'x';
    let line = String::from("n=") + to_string(&number)
        + String::from(", b=") + to_string(&flag)
        + String::from(", f=") + to_string(&ratio)
        + String::from(", c=") + to_string(&letter);
    println(line);
}
"#,
        "n=42, b=true, f=1.5, c=x\n",
    );
}

#[test]
fn collatz_sequence_prints_every_step() {
    run(
        "collatz_sequence",
        r#"module app;

fun main() {
    let mut n: i64 = 6;
    print(n);
    let mut steps = 0;
    while n != 1 {
        if n % 2 == 0 {
            n = n / 2;
        } else {
            n = 3 * n + 1;
        }
        print(" -> ");
        print(n);
        steps += 1;
    }
    println("");
    println(steps);
}
"#,
        "6 -> 3 -> 10 -> 5 -> 16 -> 8 -> 4 -> 2 -> 1\n8\n",
    );
}

#[test]
fn multi_file_shapes_project_uses_dyn_dispatch() {
    run_project(
        "multi_file_shapes",
        &[
            (
                "src/main.phi",
                r#"module app;
import shapes::Circle;
import shapes::Rect;
import shapes::Shape;

fun describe(label: str, shape: &dyn Shape) {
    print(label);
    print(": ");
    println(shape.area());
}

fun main() {
    let circle = Circle { radius: 1.0 };
    let rect = Rect { width: 3.0, height: 2.0 };
    describe("circle", &circle);
    describe("rect", &rect);
}
"#,
            ),
            (
                "src/shapes.phi",
                r#"module shapes;

public trait Shape {
    fun area(&self) -> f64;
}

public struct Circle {
    public radius: f64,
}

public struct Rect {
    public width: f64,
    public height: f64,
}

extend Circle with Shape {
    fun area(&self) -> f64 {
        return 3.14159 * self.radius * self.radius;
    }
}

extend Rect with Shape {
    fun area(&self) -> f64 {
        return self.width * self.height;
    }
}
"#,
            ),
        ],
        "circle: 3.14159\nrect: 6\n",
    );
}

#[test]
fn enum_payload_shapes_print_perimeters() {
    run(
        "enum_payload_shapes",
        r#"module app;

struct Rectangle {
    public l: f64,
    public w: f64,
}

enum Shape {
    rectangle: Rectangle,
    circle: f64,
    square: { l: f64 },
    parallelogram: (f64, f64),
    nothing,
}

extend Shape {
    fun perimeter(&self) -> f64 {
        return match self {
            .rectangle(rect) => 2.0 * (rect.l + rect.w),
            .circle(r) => 2.0 * 3.14 * (*r),
            .square { l } => 4.0 * (*l),
            .parallelogram((b, h)) => 2.0 * ((*b) + (*h)),
            .nothing => 0.0,
        };
    }
}

fun main() {
    let rect: Shape = .rectangle(Rectangle { l: 4.0, w: 6.0 });
    let circle: Shape = .circle(1.0);
    let square: Shape = .square { l: 4.0 };
    let parallelogram: Shape = .parallelogram((1.0, 1.0));
    let nothing: Shape = .nothing;
    println(rect.perimeter());
    println(circle.perimeter());
    println(square.perimeter());
    println(parallelogram.perimeter());
    println(nothing.perimeter());
}
"#,
        "20\n6.28\n16\n4\n0\n",
    );
}

#[test]
fn match_supports_literal_patterns_and_guards() {
    run(
        "match_literals_guards",
        r#"module app;

fun classify(n: i32) -> str {
    return match n {
        0 => "zero",
        1 => "one",
        value if value < 0 => "negative",
        value if value % 2 == 0 => "even",
        _ => "odd",
    };
}

fun describe(c: char) -> str {
    return match c {
        'a' => "A",
        'b' => "B",
        _ => "other",
    };
}

fun flip(value: bool) -> bool {
    return match value {
        true => false,
        false => true,
    };
}

fun main() {
    for i in -2..6 {
        print(i);
        print(" is ");
        println(classify(i));
    }
    println(describe('a'));
    println(describe('z'));
    println(flip(true));
    println(flip(false));
}
"#,
        "-2 is negative\n-1 is negative\n0 is zero\n1 is one\n2 is even\n3 is odd\n4 is even\n5 is odd\nA\nother\nfalse\ntrue\n",
    );
}

#[test]
fn defer_statements_run_in_reverse_order_at_scope_exit() {
    run(
        "defer_order",
        r#"module app;

fun main() {
    defer println("first");
    defer println("second");
    println("body");
    {
        defer println("inner");
        println("inner body");
    }
    println("after");
}
"#,
        "body\ninner body\ninner\nafter\nsecond\nfirst\n",
    );
}

#[test]
fn copy_structs_are_duplicated_on_assignment() {
    run(
        "copy_struct",
        r#"module app;
import core::ops::Copy;

struct Point {
    public x: i32,
    public y: i32,
}

extend Point with Copy {
    fun copy(&self) -> Self {
        return Point { x: self.x, y: self.y };
    }
}

fun main() {
    let a = Point { x: 1, y: 2 };
    let b = a;
    println(a.x + a.y);
    println(b.x + b.y);
    let c = b;
    println(c.x);
}
"#,
        "3\n3\n1\n",
    );
}

#[test]
fn with_block_bounds_two_mutable_projections() {
    run(
        "with_block",
        r#"module app;

struct Point {
    public x: i32,
    public y: i32,
}

fun main() {
    let mut point = Point { x: 1, y: 2 };
    with px = &mut point.x, py = &mut point.y {
        *px += 10;
        *py += 20;
        let total = *px + *py;
        println(total);
    }
    println(point.x);
    println(point.y);
}
"#,
        "33\n11\n22\n",
    );
}

#[test]
fn any_parameters_support_owned_shared_and_mutable_modes() {
    run(
        "any_parameters",
        r#"module app;

struct Foo {
    public x: i32,
}

extend Foo {
    fun show(&self) -> i32 {
        return self.x;
    }
}

fun relay(value: any Foo) -> i32 {
    return value.show();
}

fun main() {
    let owned = Foo { x: 5 };
    println(relay(owned));
    let shared = Foo { x: 7 };
    println(relay(&shared));
    let mut mutable = Foo { x: 9 };
    println(relay(&mut mutable));
}
"#,
        "5\n7\n9\n",
    );
}

#[test]
fn closures_capture_copied_state_by_value() {
    run(
        "closure_capture_by_value",
        r#"module app;

fun main() {
    let mut total = 0;
    let add = |x| {
        total += x;
    };
    add(2);
    add(3);
    add(5);
    println(total);
}
"#,
        "0\n",
    );
}

#[test]
fn function_values_and_void_function_types_work() {
    run(
        "function_values",
        r#"module app;

fun double(x: i32) -> i32 {
    return x * 2;
}

fun apply(f: fun(i32) -> i32, x: i32) -> i32 {
    return f(x);
}

fun show(x: i32) {
    println(x);
}

fun run(f: fun(i32), x: i32) {
    f(x);
}

fun main() {
    println(apply(double, 21));
    let add_one = |x| x + 1;
    println(apply(add_one, 41));
    run(show, 99);
    run(|value| println(value + 1), 9);
}
"#,
        "42\n42\n99\n10\n",
    );
}

#[test]
fn comments_and_string_char_escapes_are_lexed() {
    run(
        "escapes_comments",
        r#"module app;
/* outer comment
   /* nested comment */
   still in the outer comment
*/
fun main() {
    // line comment
    println("line1\nline2");
    println("tab:\tend");
    println("quote: \" and backslash: \\");
    print('A');
    println('B');
}
"#,
        "line1\nline2\ntab:\tend\nquote: \" and backslash: \\\nAB\n",
    );
}

#[test]
fn float_comparisons_handle_nan_and_infinity() {
    run(
        "float_comparisons",
        r#"module app;
fun main() {
    let nan = 0.0 / 0.0;
    let inf = 1.0 / 0.0;
    println(nan == nan);
    println(nan != nan);
    println(inf > 0.0);
    println(0.0 - inf < 0.0);
    println(1.0 / 3.0);
}
"#,
        "false\ntrue\ntrue\ntrue\n0.3333333333333333\n",
    );
}

#[test]
fn every_compound_assignment_operator_runs() {
    run(
        "compound_assign",
        r#"module app;
fun main() {
    let mut value = 20;
    value += 5;
    println(value);
    value -= 3;
    println(value);
    value *= 2;
    println(value);
    value /= 4;
    println(value);
    value %= 5;
    println(value);
}
"#,
        "25\n22\n44\n11\n1\n",
    );
}

#[test]
fn else_if_let_chains_through_borrowed_options() {
    run(
        "else_if_let",
        r#"module app;
import core::option::Option;

fun describe(value: &Option<i32>) -> str {
    return if let .some(n) = value {
        if *n > 0 { "positive" } else { "non-positive" }
    } else if let .none = value {
        "none"
    } else {
        "unknown"
    };
}

fun main() {
    let some: Option<i32> = .some(5);
    let negative: Option<i32> = .some(-5);
    let none: Option<i32> = .none;
    println(describe(&some));
    println(describe(&negative));
    println(describe(&none));
}
"#,
        "positive\nnon-positive\nnone\n",
    );
}

#[test]
fn associated_functions_construct_and_mutate_a_counter() {
    run(
        "associated_functions",
        r#"module app;

struct Counter {
    public value: i32,
}

extend Counter {
    public fun empty() -> Counter {
        return Counter { value: 0 };
    }

    public fun bump(&mut self) {
        self.value += 1;
    }
}

fun main() {
    let mut counter = Counter::empty();
    counter.bump();
    counter.bump();
    counter.bump();
    println(counter.value);
}
"#,
        "3\n",
    );
}

#[test]
fn nested_modules_re_export_public_items_through_wildcards() {
    run_project(
        "nested_modules",
        &[
            (
                "src/main.phi",
                r#"module app;
import math::vector::*;

fun main() {
    let point = Point { x: 3, y: 4 };
    println(point.x + point.y);
    println(length(&point));
}
"#,
            ),
            (
                "src/vector.phi",
                r#"module math::vector;

public struct Point {
    public x: i32,
    public y: i32,
}

public fun length(point: &Point) -> i32 {
    return point.x * point.x + point.y * point.y;
}
"#,
            ),
        ],
        "7\n25\n",
    );
}

#[test]
fn iso_struct_fields_are_auto_dereferenced() {
    run(
        "iso_struct",
        r#"module app;

struct Point {
    public x: i32,
    public y: i32,
}

struct Box<T> {
    public inner: iso T,
}

fun main() {
    let boxed: Box<Point> = Box { inner: new Point { x: 3, y: 4 } };
    println(boxed.inner.x);
    println(boxed.inner.y);
}
"#,
        "3\n4\n",
    );
}

#[test]
fn functions_and_methods_can_return_projections() {
    run(
        "returned_projections",
        r#"module app;
import std::string::String;

struct Pair {
    public first: String,
    public second: String,
}

extend Pair {
    public fun longer(&self) -> &String {
        return if self.first.len() > self.second.len() {
            &self.first
        } else {
            &self.second
        };
    }
}

fun first_word(text: &String) -> &String {
    return text;
}

fun main() {
    let pair = Pair {
        first: String::from("short"),
        second: String::from("much longer"),
    };
    println(pair.longer());
    let word = String::from("hello");
    println(first_word(&word));
}
"#,
        "much longer\nhello\n",
    );
}

#[test]
fn generic_enums_support_payloads_and_fallbacks() {
    run(
        "generic_enum",
        r#"module app;

enum Maybe<T> {
    value: T,
    absent,
}

fun unwrap_or<T>(value: Maybe<T>, fallback: T) -> T {
    return match value {
        .value(inner) => inner,
        .absent => fallback,
    };
}

fun main() {
    let some: Maybe<i32> = .value(7);
    let none: Maybe<i32> = .absent;
    println(unwrap_or(some, 0));
    println(unwrap_or(none, 42));
    let text: Maybe<str> = .value("hello");
    println(unwrap_or(text, "missing"));
}
"#,
        "7\n42\nhello\n",
    );
}

#[test]
fn iso_values_cross_function_boundaries() {
    run(
        "iso_return",
        r#"module app;

struct Point {
    public x: i32,
    public y: i32,
}

fun boxed_point(x: i32, y: i32) -> iso Point {
    return new Point { x: x, y: y };
}

fun boxed_number(value: i32) -> iso i32 {
    return new value;
}

fun main() {
    let point = boxed_point(3, 4);
    println(point.x);
    println(point.y);
    let number = boxed_number(42);
    println(*number);
}
"#,
        "3\n4\n42\n",
    );
}

#[test]
fn moving_a_field_leaves_the_rest_of_the_struct_usable() {
    run(
        "partial_move",
        r#"module app;
import std::string::String;

struct Pair {
    public first: String,
    public second: String,
}

fun main() {
    let pair = Pair {
        first: String::from("first"),
        second: String::from("second"),
    };
    let first = pair.first;
    println(&first);
    println(&pair.second);
}
"#,
        "first\nsecond\n",
    );
}

#[test]
fn lossless_casts_cover_integer_float_bool_and_char() {
    run(
        "lossless_casts",
        r#"module app;
fun main() {
    let small: i8 = 12;
    let wide: i64 = small as i64;
    println(wide);
    let byte: u8 = 200;
    let double: f64 = byte as f64;
    println(double);
    let half_word: i16 = 42;
    let single: f32 = half_word as f32;
    println(single);
    let narrow_float: f32 = 1.5;
    let wide_float: f64 = narrow_float as f64;
    println(wide_float);
    let yes = true as i32;
    println(yes);
    let letter = 'A' as u32;
    println(letter);
}
"#,
        "12\n200\n42\n1.5\n1\n65\n",
    );
}

#[test]
fn drop_impls_run_in_reverse_declaration_order() {
    run(
        "drop_order",
        r#"module app;
import core::ops::Drop;

struct Loud {
    public id: i32,
}

extend Loud with Drop {
    fun drop(self) {
        println(self.id);
    }
}

fun main() {
    let first = Loud { id: 1 };
    let second = Loud { id: 2 };
    println(first.id + second.id);
}
"#,
        "3\n2\n1\n",
    );
}

#[test]
fn tuple_patterns_work_in_matches_and_bindings() {
    run(
        "tuple_patterns",
        r#"module app;

fun sum(pair: (i32, i32)) -> i32 {
    return match pair {
        (left, right) => left + right,
        _ => 0,
    };
}

fun main() {
    println(sum((3, 4)));
    let (x, y) = (10, 20);
    println(x + y);
    let nested = ((1, 2), 3);
    match nested {
        ((a, b), c) => println(a + b + c),
        _ => println(0),
    };
}
"#,
        "7\n30\n6\n",
    );
}

#[test]
fn operator_overloading_implements_add_eq_and_comparable() {
    run(
        "operator_overload",
        r#"module app;
import core::ops::Add;
import core::ops::Eq;
import core::ops::Comparable;

struct Vec2 {
    public x: i32,
    public y: i32,
}

extend Vec2 with Add<Vec2> {
    fun add(&self, other: &Self) -> Self {
        return Vec2 { x: self.x + other.x, y: self.y + other.y };
    }
}

extend Vec2 with Eq {
    fun eq(&self, other: &Self) -> bool {
        return self.x == other.x && self.y == other.y;
    }
}

extend Vec2 with Comparable {
    fun less_than(&self, other: &Self) -> bool {
        return self.x * self.x + self.y * self.y < other.x * other.x + other.y * other.y;
    }
}

fun main() {
    let a = Vec2 { x: 1, y: 2 };
    let b = Vec2 { x: 3, y: 4 };
    let c = a + b;
    println(c.x);
    println(c.y);
    println(a == Vec2 { x: 1, y: 2 });
    println(a < b);
}
"#,
        "4\n6\ntrue\ntrue\n",
    );
}

#[test]
fn custom_index_and_index_set_operators_run() {
    run(
        "custom_index",
        r#"module app;
import core::ops::Index;
import core::ops::IndexSet;

struct Pair {
    public first: i32,
    public second: i32,
}

extend Pair with Index<usize, i32> {
    fun index(&self, key: usize) -> &i32 {
        if key == 0 {
            return &self.first;
        }
        return &self.second;
    }
}

extend Pair with IndexSet<usize, i32> {
    fun index_set(&mut self, key: usize, value: i32) {
        if key == 0 {
            self.first = value;
        } else {
            self.second = value;
        }
    }
}

fun main() {
    let mut pair = Pair { first: 1, second: 2 };
    pair[0] = 10;
    pair[1] = 20;
    println(pair[0]);
    println(pair[1]);
}
"#,
        "10\n20\n",
    );
}

#[test]
fn tail_expressions_are_function_return_values() {
    run(
        "tail_expressions",
        r#"module app;

fun add(a: i32, b: i32) -> i32 {
    a + b
}

fun describe(n: i32) -> str {
    if n < 0 {
        "negative"
    } else if n == 0 {
        "zero"
    } else {
        "positive"
    }
}

fun main() {
    println(add(2, 3));
    println(describe(-1));
    println(describe(0));
    println(describe(1));
}
"#,
        "5\nnegative\nzero\npositive\n",
    );
}

#[test]
fn open_and_inclusive_ranges_support_contains() {
    run(
        "open_ranges",
        r#"module app;

fun main() {
    let from = 5..;
    let to = ..10;
    let all = ..;
    let inclusive = 1..=5;
    println(from.contains(&7));
    println(from.contains(&3));
    println(to.contains(&9));
    println(to.contains(&10));
    println(all.contains(&0));
    println(inclusive.contains(&5));
    println(inclusive.contains(&6));
}
"#,
        "true\nfalse\ntrue\nfalse\ntrue\ntrue\nfalse\n",
    );
}

#[test]
fn defer_runs_when_a_function_returns() {
    run(
        "defer_return",
        r#"module app;

fun compute() -> i32 {
    defer println("defer ran");
    return 7;
}

fun main() {
    println(compute());
}
"#,
        "defer ran\n7\n",
    );
}

#[test]
fn unary_negation_and_not_can_be_overloaded() {
    run(
        "neg_not",
        r#"module app;
import core::ops::Neg;
import core::ops::Not;

struct Vec2 {
    public x: i32,
    public y: i32,
}

struct Flag {
    public value: bool,
}

extend Vec2 with Neg {
    fun neg(&self) -> Self {
        return Vec2 { x: 0 - self.x, y: 0 - self.y };
    }
}

extend Flag with Not {
    fun not(&self) -> Self {
        return Flag { value: !self.value };
    }
}

fun main() {
    let a = Vec2 { x: 1, y: 2 };
    let b = -a;
    println(b.x);
    println(b.y);
    let flag = Flag { value: true };
    let flipped = !flag;
    println(flipped.value);
}
"#,
        "-1\n-2\nfalse\n",
    );
}

#[test]
fn dyn_trait_objects_can_expose_multiple_methods() {
    run(
        "dyn_multi",
        r#"module app;

trait Shape {
    fun area(&self) -> f64;
    fun sides(&self) -> i32;
}

struct Circle {
    public radius: f64,
}

struct Rect {
    public width: f64,
    public height: f64,
}

extend Circle with Shape {
    fun area(&self) -> f64 { return 3.14159 * self.radius * self.radius; }
    fun sides(&self) -> i32 { return 0; }
}

extend Rect with Shape {
    fun area(&self) -> f64 { return self.width * self.height; }
    fun sides(&self) -> i32 { return 4; }
}

fun describe(label: str, shape: &dyn Shape) {
    print(label);
    print(" area=");
    print(shape.area());
    print(" sides=");
    println(shape.sides());
}

fun main() {
    let circle = Circle { radius: 1.0 };
    let rect = Rect { width: 3.0, height: 2.0 };
    describe("circle", &circle);
    describe("rect", &rect);
}
"#,
        "circle area=3.14159 sides=0\nrect area=6 sides=4\n",
    );
}

#[test]
fn result_try_propagation_returns_the_error() {
    run(
        "result_try",
        r#"module app;
import core::result::Result;

fun half(n: i32) -> Result<i32, str> {
    if n % 2 == 0 {
        return .ok(n / 2);
    }
    return .err("odd");
}

fun combine(a: i32, b: i32) -> Result<i32, str> {
    let sum = half(a)? + half(b)?;
    return .ok(sum);
}

fun main() {
    println(combine(20, 12).unwrap_or(-1));
    println(combine(20, 13).err().unwrap());
    let odd = half(9);
    println(odd.is_err());
}
"#,
        "16\nodd\ntrue\n",
    );
}

#[test]
fn for_loops_support_break_and_continue() {
    run(
        "for_break_continue",
        r#"module app;

fun main() {
    for i in 0..10 {
        if i == 3 {
            continue;
        }
        if i == 7 {
            break;
        }
        print(i);
    }
    println("");
}
"#,
        "012456\n",
    );
}

#[test]
fn defer_runs_on_loop_continue_and_break() {
    run(
        "defer_loop",
        r#"module app;

fun main() {
    for i in 0..3 {
        defer println(i);
        if i == 1 {
            continue;
        }
        if i == 2 {
            break;
        }
        println("body");
    }
    println("done");
}
"#,
        "body\n0\n1\n2\ndone\n",
    );
}

#[test]
fn match_supports_string_literal_patterns() {
    run(
        "string_patterns",
        r#"module app;

fun answer(text: str) -> str {
    return match text {
        "yes" => "affirmative",
        _ => "unknown",
    };
}

fun main() {
    println(answer("yes"));
    println(answer("maybe"));
}
"#,
        "affirmative\nunknown\n",
    );
}

#[test]
fn match_supports_or_patterns_for_literals_bools_and_variants() {
    run(
        "or_patterns",
        r#"module app;

enum Color {
    red,
    green,
    blue,
}

fun classify(n: i32) -> str {
    return match n {
        1 | 2 => "small",
        3 | 4 => "medium",
        _ => "large",
    };
}

fun warm(color: Color) -> bool {
    return match color {
        .red | .green => true,
        .blue => false,
    };
}

fun bool_name(value: bool) -> str {
    return match value {
        true | false => "bool",
    };
}

fun main() {
    println(classify(2));
    println(classify(3));
    println(classify(9));
    println(warm(Color.red));
    println(warm(Color.blue));
    println(bool_name(false));
}
"#,
        "small\nmedium\nlarge\ntrue\nfalse\nbool\n",
    );
}

#[test]
fn any_return_types_adapt_to_the_call_site() {
    run(
        "any_return",
        r#"module app;

fun min(x: any i32, y: any i32) -> any i32 {
    return if x < y { x } else { y };
}

fun main() {
    let owned = min(7, 3);
    println(owned);
    let a = 5;
    let b = 9;
    let shared = min(&a, &b);
    println(*shared);
    let mut c = 11;
    let mut d = 4;
    let unique = min(&mut c, &mut d);
    println(*unique);
}
"#,
        "3\n5\n4\n",
    );
}

#[test]
fn string_plus_str_concatenates() {
    run(
        "string_plus_str",
        r#"module app;
import std::string::String;

fun main() {
    let greeting = String::from("hello") + "!";
    println(greeting);
    let mirrored = "hello" + String::from("!");
    println(mirrored);
    let chained = "a" + String::from("b") + "c";
    println(chained);
}
"#,
        "hello!\nhello!\nabc\n",
    );
}

#[test]
fn associated_types_project_from_an_implementation() {
    run(
        "associated_types",
        r#"module app;

public trait Container {
    type Item;
    fun get(&self) -> Self::Item;
}

struct Number { value: i32 }

extend Number with Container {
    type Item = i32;
    fun get(&self) -> i32 { return self.value; }
}

public trait Doubler {
    type Output = Self;
    fun double(&self) -> Self::Output;
}

extend i32 with Doubler {
    fun double(&self) -> i32 { return *self * 2; }
}

fun main() {
    let number = Number { value: 42 };
    println(number.get());
    let five = 5;
    println(five.double());
}
"#,
        "42\n10\n",
    );
}
