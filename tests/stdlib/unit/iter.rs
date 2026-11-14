use super::*;

#[test]
fn a_custom_iterator_yields_until_none() {
    run(
        "iter_custom",
        r#"module app;
import core::iter::Iterator;
struct Countdown { current: i32 }
extend Countdown with Iterator<i32> {
    fun next(&mut self) -> Option<i32> {
        if self.current <= 0 {
            return .none;
        }
        let value = self.current;
        self.current = self.current - 1;
        return .some(value);
    }
}
fun main() {
    let mut countdown = Countdown { current: 3 };
    println(countdown.next().unwrap());
    println(countdown.next().unwrap());
    println(countdown.next().unwrap());
    let exhausted = countdown.next();
    println(exhausted.is_none());
}
"#,
        "3\n2\n1\ntrue\n",
    );
}

#[test]
fn a_custom_iterator_drives_a_for_loop() {
    run(
        "iter_custom_for",
        r#"module app;
import core::iter::IntoIterator;
import core::iter::Iterator;
import core::option::Option;
struct Counter { current: i32, limit: i32 }
struct CounterIter { current: i32, limit: i32 }
extend CounterIter with Iterator<i32> {
    fun next(&mut self) -> Option<i32> {
        if self.current >= self.limit {
            return .none;
        }
        let value = self.current;
        self.current = self.current + 1;
        return .some(value);
    }
}
extend Counter with IntoIterator {
    type Iter = CounterIter;
    fun into_iter(&self) -> CounterIter {
        return CounterIter { current: self.current, limit: self.limit };
    }
}
fun main() {
    let counter = Counter { current: 1, limit: 4 };
    let mut sum = 0;
    for value in counter {
        sum += value;
    }
    println(sum);
}
"#,
        "6\n",
    );
}
