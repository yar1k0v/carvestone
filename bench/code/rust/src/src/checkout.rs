use crate::order::{Order, total};

pub fn run() {
    let o = Order::new();
    o.place();
    total();
}
