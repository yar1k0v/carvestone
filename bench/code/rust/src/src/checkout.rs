use crate::order::{Order, total, LIMIT};

pub fn run() {
    let o = Order::new();
    o.place();
    total();
}

pub fn over(n: u32) -> bool {
    n > LIMIT
}
