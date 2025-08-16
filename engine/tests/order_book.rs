use engine::book::OrderBook;
use engine::messages::trading::Side;

fn setup_book() -> OrderBook {
    let mut book = OrderBook::new();
    for i in 0..1000 {
        book.add_limit_order(Side::Buy, 9999 - i, 10);
        book.add_limit_order(Side::Sell, 10001 + i, 10);
    }
    book
}

#[test]
fn add_limit_order_no_match() {
    let mut book = setup_book();
    let (_, trades) = book.add_limit_order(Side::Buy, 9000, 10);
    assert_eq!(trades.len(), 0);
}

#[test]
fn add_limit_order_full_match_one() {
    let mut book = setup_book();
    let (_, trades) = book.add_limit_order(Side::Buy, 10001, 10);
    assert_eq!(trades.len(), 1);
}

#[test]
fn add_limit_order_walk_the_book() {
    let mut book = setup_book();
    let (_, trades) = book.add_limit_order(Side::Buy, 10005, 50);
    assert_eq!(trades.len(), 5);
}

#[test]
fn add_limit_order_two_fills_same_level() {
    let mut book = OrderBook::new();
    book.add_limit_order(Side::Sell, 10000, 5);
    book.add_limit_order(Side::Sell, 10000, 5);
    let (_, trades) = book.add_limit_order(Side::Buy, 10000, 10);
    assert_eq!(trades.len(), 2);
    assert_eq!(book.bids.len(), 0);
    assert_eq!(book.asks.len(), 0);
}

#[test]
fn cancel_limit_order() {
    let mut book = OrderBook::new();
    let (order_id, _) = book.add_limit_order(Side::Sell, 10000, 5);
    book.cancel_order(order_id).unwrap();
    assert_eq!(book.orders.len(), 0);
}
