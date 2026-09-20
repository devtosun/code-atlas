pub struct First;
pub struct Second;

impl First {
    pub fn render(&self) -> &'static str { "first" }
}

impl Second {
    pub fn render(&self) -> &'static str { "second" }
}

pub fn shadow(value: i32) -> i32 {
    let value = value + 1;
    {
        let value = value + 1;
        value
    }
}
