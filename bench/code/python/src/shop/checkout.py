from shop.order import Order, total


def run():
    o = Order()
    o.place()
    total()
