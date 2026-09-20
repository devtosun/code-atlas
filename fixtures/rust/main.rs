mod math;
use math::double as twice;
pub struct Invoice { pub amount: i64 }
impl Invoice {
    pub fn total(&self) -> i64 { twice(self.amount) }
}
pub fn summarize(invoice: &Invoice) -> i64 { invoice.total() }
// phantom_call() must not become a call site.
pub fn label() -> &'static str { "phantom_call()" }
