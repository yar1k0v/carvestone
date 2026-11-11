namespace Shop {
 public class Checkout {
  public void Run() {
   var s = new OrderService();
   s.Place();
  }
 }
}
