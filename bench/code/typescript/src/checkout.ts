import { Order, total } from "./order";

export function run(): void {
  const o = new Order();
  o.place();
  total();
}
