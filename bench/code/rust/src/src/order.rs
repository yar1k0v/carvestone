pub struct Order;

impl Order {
    pub fn new() -> Order {
        Order
    }
    pub fn place(&self) {}
}

pub fn total() -> u32 {
    0
}

pub const LIMIT: u32 = 3;

pub fn capped() -> u32 {
    LIMIT
}
