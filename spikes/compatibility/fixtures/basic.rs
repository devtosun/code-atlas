struct RustBasic {
    value: i32,
}

impl RustBasic {
    fn doubled(&self) -> i32 {
        self.value * 2
    }
}

fn rust_basic() -> i32 {
    RustBasic { value: 21 }.doubled()
}
