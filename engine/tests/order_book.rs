use engine::book::OrderBook;
use engine::messages::trading::Side;

fn setup_book() -> OrderBook {
    let mut book = OrderBook::new();
    for i in 0..1000 {
        book.add_limit_order(Side::Buy, 9999 - i, 10).unwrap();
        book.add_limit_order(Side::Sell, 10001 + i, 10).unwrap();
    }
    book
}

#[test]
fn add_limit_order_no_match() {
    let mut book = setup_book();
    let (_, trades) = book.add_limit_order(Side::Buy, 9000, 10).unwrap();
    assert_eq!(trades.len(), 0);
}

#[test]
fn add_limit_order_full_match_one() {
    let mut book = setup_book();
    let (_, trades) = book.add_limit_order(Side::Buy, 10001, 10).unwrap();
    assert_eq!(trades.len(), 1);
}

#[test]
fn add_limit_order_walk_the_book() {
    let mut book = setup_book();
    let (_, trades) = book.add_limit_order(Side::Buy, 10005, 50).unwrap();
    assert_eq!(trades.len(), 5);
}

#[test]
fn add_limit_order_two_fills_same_level() {
    let mut book = OrderBook::new();
    book.add_limit_order(Side::Sell, 10000, 5).unwrap();
    book.add_limit_order(Side::Sell, 10000, 5).unwrap();
    let (_, trades) = book.add_limit_order(Side::Buy, 10000, 10).unwrap();
    assert_eq!(trades.len(), 2);
    assert_eq!(book.bids.len(), 0);
    assert_eq!(book.asks.len(), 0);
}

#[test]
fn cancel_limit_order() {
    let mut book = OrderBook::new();
    let (order_id, _) = book.add_limit_order(Side::Sell, 10000, 5).unwrap();
    book.cancel_order(order_id).unwrap();
    assert_eq!(book.orders.len(), 0);
}

#[test]
fn price_priority_fifo_and_maker_execution_price_on_both_sides() {
    for (maker_side, taker_side, worse, better, limit) in [
        (Side::Sell, Side::Buy, 102, 101, 103),
        (Side::Buy, Side::Sell, 101, 102, 100),
    ] {
        let mut book = OrderBook::new();
        let (first, _) = book.add_limit_order(maker_side, worse, 3).unwrap();
        let (second, _) = book.add_limit_order(maker_side, better, 4).unwrap();
        let (third, _) = book.add_limit_order(maker_side, better, 5).unwrap();
        let (taker, trades) = book.add_limit_order(taker_side, limit, 12).unwrap();
        let actual: Vec<_> = trades
            .iter()
            .map(|t| (t.maker_order_id, t.taker_order_id, t.quantity, t.price))
            .collect();
        assert_eq!(
            actual,
            vec![
                (second, taker, 4, better),
                (third, taker, 5, better),
                (first, taker, 3, worse)
            ]
        );
        assert!(book.orders.is_empty());
        assert!(book.bids.is_empty());
        assert!(book.asks.is_empty());
    }
}

#[test]
fn cancelled_orders_at_every_queue_position_never_trade() {
    for side in [Side::Buy, Side::Sell] {
        for cancelled_position in 0..3 {
            let mut book = OrderBook::new();
            let ids: Vec<_> = (0..3)
                .map(|_| book.add_limit_order(side, 100, 5).unwrap().0)
                .collect();
            let cancelled = ids[cancelled_position];
            book.cancel_order(cancelled).unwrap();
            assert!(book.cancel_order(cancelled).is_err());
            let opposite = if side == Side::Buy {
                Side::Sell
            } else {
                Side::Buy
            };
            let (taker, trades) = book.add_limit_order(opposite, 100, 15).unwrap();
            assert_eq!(trades.iter().map(|t| t.quantity).sum::<u64>(), 10);
            assert_eq!(
                trades.iter().map(|t| t.maker_order_id).collect::<Vec<_>>(),
                ids.into_iter()
                    .filter(|id| *id != cancelled)
                    .collect::<Vec<_>>()
            );
            assert_eq!(book.orders.len(), 1);
            assert_eq!(book.orders[&taker].borrow().quantity, 5);
        }
    }
}

#[test]
fn partial_fills_conserve_quantity_and_rest_at_the_limit() {
    let mut book = OrderBook::new();
    let (maker, _) = book.add_limit_order(Side::Sell, 100, 10).unwrap();
    let (_, trades) = book.add_limit_order(Side::Buy, 101, 4).unwrap();
    assert_eq!(trades[0].quantity, 4);
    assert_eq!(book.orders[&maker].borrow().quantity, 6);
    let (taker, trades) = book.add_limit_order(Side::Buy, 101, 9).unwrap();
    assert_eq!(trades[0].quantity, 6);
    assert!(!book.orders.contains_key(&maker));
    let remaining = book.orders[&taker].borrow();
    assert_eq!((remaining.quantity, remaining.price), (3, 101));
    assert_eq!(4 + 6 + remaining.quantity, 4 + 9);
}

#[test]
fn non_crossing_orders_and_cancelled_levels_do_not_create_fills() {
    let mut book = OrderBook::new();
    let (id, _) = book.add_limit_order(Side::Sell, 99, 5).unwrap();
    book.cancel_order(id).unwrap();
    book.add_limit_order(Side::Sell, 102, 5).unwrap();
    let (_, trades) = book.add_limit_order(Side::Buy, 100, 5).unwrap();
    assert!(trades.is_empty());
    assert_eq!(book.orders.len(), 2);
    assert!(!book.asks.contains_key(&99));
}

#[test]
fn invalid_orders_do_not_mutate_book_or_consume_ids() {
    let mut book = OrderBook::new();
    for (side, price, quantity) in [
        (Side::Unspecified, 100, 1),
        (Side::Buy, 0, 1),
        (Side::Sell, 100, 0),
        (Side::Buy, u64::MAX, 1),
        (Side::Sell, 100, u64::MAX),
    ] {
        assert!(book.add_limit_order(side, price, quantity).is_err());
        assert!(book.orders.is_empty());
        assert!(book.bids.is_empty());
        assert!(book.asks.is_empty());
        assert_eq!(book.next_order_id, 1);
    }
    assert!(book.cancel_order(123).is_err());
    book.next_order_id = i64::MAX as u64;
    assert_eq!(
        book.add_limit_order(Side::Buy, 100, 1).unwrap().0,
        i64::MAX as u64
    );
    assert!(book.add_limit_order(Side::Buy, 100, 1).is_err());
}

#[test]
fn deterministic_mixed_workloads_match_a_simple_reference_model() {
    // A deliberately simple vector model provides an independent oracle for the indexed book.
    #[derive(Clone)]
    struct Resting {
        id: u64,
        side: Side,
        price: u64,
        quantity: u64,
    }
    for seed in 1u64..=8 {
        let mut random = seed;
        let mut next = || {
            random = random.wrapping_mul(6364136223846793005u64).wrapping_add(1);
            random >> 32
        };
        let mut book = OrderBook::new();
        let mut reference: Vec<Resting> = Vec::new();
        let mut next_id = 1;
        for _ in 0..500 {
            if next() % 4 == 0 && !reference.is_empty() {
                let index = next() as usize % reference.len();
                let removed = reference.remove(index);
                book.cancel_order(removed.id).unwrap();
            } else {
                let side = if next() % 2 == 0 {
                    Side::Buy
                } else {
                    Side::Sell
                };
                let price = 90 + next() % 21;
                let quantity = 1 + next() % 20;
                let mut remaining = quantity;
                let mut expected = Vec::new();
                while remaining > 0 {
                    let best = reference
                        .iter()
                        .enumerate()
                        .filter(|(_, order)| {
                            order.side != side
                                && if side == Side::Buy {
                                    order.price <= price
                                } else {
                                    order.price >= price
                                }
                        })
                        .min_by_key(|(_, order)| {
                            (
                                if side == Side::Buy {
                                    order.price
                                } else {
                                    u64::MAX - order.price
                                },
                                order.id,
                            )
                        })
                        .map(|(index, _)| index);
                    let Some(index) = best else { break };
                    let maker = &mut reference[index];
                    let filled = remaining.min(maker.quantity);
                    expected.push((maker.id, next_id, maker.price, filled));
                    remaining -= filled;
                    maker.quantity -= filled;
                    if maker.quantity == 0 {
                        reference.remove(index);
                    }
                }
                let (id, trades) = book.add_limit_order(side, price, quantity).unwrap();
                assert_eq!(id, next_id);
                assert_eq!(
                    trades
                        .iter()
                        .map(|t| (t.maker_order_id, t.taker_order_id, t.price, t.quantity))
                        .collect::<Vec<_>>(),
                    expected
                );
                if remaining > 0 {
                    reference.push(Resting {
                        id,
                        side,
                        price,
                        quantity: remaining,
                    });
                }
                next_id += 1;
            }
            let mut actual: Vec<_> = book
                .orders
                .values()
                .map(|handle| {
                    let order = handle.borrow();
                    assert_eq!(order.status, engine::book::OrderStatus::Open);
                    (order.id, order.side as i32, order.price, order.quantity)
                })
                .collect();
            actual.sort_unstable();
            let mut expected: Vec<_> = reference
                .iter()
                .map(|o| (o.id, o.side as i32, o.price, o.quantity))
                .collect();
            expected.sort_unstable();
            assert_eq!(actual, expected);
        }
    }
}
