pub fn double(value: i64) -> i64 { value * 2 }
pub fn shadow() -> i64 {
    let double = |n: i64| n + 10;
    double(2) // Local closure, not this module's double function.
}
