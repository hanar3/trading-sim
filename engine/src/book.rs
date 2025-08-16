#![allow(unused)]
use crate::{book, messages::trading::Side};
use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap, VecDeque},
    rc::Rc,
    time::Duration,
};

type Price = u64;
type Quantity = u64;
type OrderId = u64;
type OrderHandle = Rc<RefCell<Order>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderStatus {
    Open,
    Filled,
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct Order {
    pub id: OrderId,
    pub side: Side,
    pub price: Price,
    pub quantity: Quantity,
    pub status: OrderStatus,
}

#[derive(Debug, Clone)]
pub struct Trade {
    pub taker_order_id: OrderId,
    pub maker_order_id: OrderId,
    pub quantity: Quantity,
    pub price: Price,
}

#[derive(Debug, Clone)]
pub struct OrderBook {
    pub bids: BTreeMap<Price, VecDeque<OrderHandle>>,
    pub asks: BTreeMap<Price, VecDeque<OrderHandle>>,
    pub orders: HashMap<OrderId, OrderHandle>,
    pub next_order_id: OrderId,
    pub trades_buffer: Vec<Trade>,
}

// BTC-USD
impl OrderBook {
    pub fn new() -> Self {
        OrderBook {
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            trades_buffer: Vec::with_capacity(32),
            orders: HashMap::new(),
            next_order_id: 1,
        }
    }

    fn get_next_order_id(&mut self) -> OrderId {
        let id = self.next_order_id;
        self.next_order_id += 1;
        id
    }

    pub fn add_limit_order(
        &mut self,
        side: Side,
        price: Price,
        quantity: Quantity,
    ) -> (u64, &Vec<Trade>) {
        let order_id = self.get_next_order_id();
        log::debug!("next order id > {}", order_id);
        let mut order_handle = Rc::new(RefCell::new(Order {
            id: order_id,
            side,
            price,
            quantity,
            status: OrderStatus::Open,
        }));

        let mut order = order_handle.borrow_mut();
        self.match_order(&mut order);

        if order.quantity > 0 {
            let book_side = match side {
                Side::Buy => &mut self.bids,
                Side::Sell => &mut self.asks,
                Side::Unspecified => panic!("no side unspecied allowed"),
            };

            // Create a new price level
            book_side
                .entry(price)
                .or_insert_with(VecDeque::new)
                .push_back(Rc::clone(&order_handle));
            self.orders.insert(order_id, Rc::clone(&order_handle));
        }

        (order_id, &self.trades_buffer)
    }

    pub fn cancel_order(&mut self, order_id: OrderId) -> Result<(), &'static str> {
        let order_handle = match self.orders.get(&order_id) {
            Some(handle) => handle.clone(),
            None => return Err("Order not found"),
        };

        let mut order = order_handle.borrow_mut();

        if order.status != OrderStatus::Open {
            return Err("Order is not open");
        }

        order.status = OrderStatus::Cancelled;
        self.orders.remove(&order_id);
        Ok(())
    }

    pub fn match_order(&mut self, taker_order: &mut Order) {
        log::debug!("matching order # = {}", taker_order.id);
        self.trades_buffer.clear();

        let book_to_match = match taker_order.side {
            Side::Unspecified => panic!("no side unspecied allowed"),
            Side::Buy => {
                log::debug!(
                    "side is buy, matching order # {} against asks",
                    taker_order.id
                );
                &mut self.asks
            }
            Side::Sell => {
                log::debug!(
                    "side is sell, matching order # {} against bids",
                    taker_order.id
                );
                &mut self.bids
            }
        };

        loop {
            if taker_order.quantity == 0 {
                break;
            }
            let best_price = match taker_order.side {
                Side::Unspecified => panic!("no side unspecied allowed"),
                Side::Buy => book_to_match.keys().next().cloned(),
                Side::Sell => book_to_match.keys().next_back().cloned(),
            };

            let best_price = match best_price {
                Some(p) => p,
                None => break,
            };

            match taker_order.side {
                Side::Buy if taker_order.price < best_price => break,
                Side::Sell if taker_order.price > best_price => break,
                Side::Unspecified => panic!("no side unspecied allowed"),
                _ => (),
            };

            let mut level_drained = false;
            let mut front_pops_needed = 0;

            if let Some(price_level_queue) = book_to_match.get_mut(&best_price) {
                let is_ghost = if let Some(maker_handle) = price_level_queue.front() {
                    maker_handle.borrow().status != OrderStatus::Open
                } else {
                    book_to_match.remove(&best_price); // level is drained
                    continue;
                };

                if is_ghost {
                    price_level_queue.pop_front();
                    continue;
                }

                while let Some(maker_handle) = price_level_queue.front() {
                    let mut maker_order = maker_handle.borrow_mut();

                    if taker_order.quantity == 0 {
                        break;
                    }

                    let trade_quantity = taker_order.quantity.min(maker_order.quantity);
                    log::debug!(
                        "filled qty {} for taker_order # {} and maker_order {}",
                        trade_quantity,
                        taker_order.id,
                        maker_order.id
                    );

                    self.trades_buffer.push(Trade {
                        taker_order_id: taker_order.id,
                        maker_order_id: maker_order.id,
                        quantity: trade_quantity,
                        price: maker_order.price,
                    });

                    taker_order.quantity -= trade_quantity;
                    maker_order.quantity -= trade_quantity;

                    if maker_order.quantity == 0 {
                        maker_order.status = OrderStatus::Filled;
                        self.orders.remove(&maker_order.id);
                        front_pops_needed += 1;
                        break;
                    }
                }

                if front_pops_needed > 0 {
                    loop {
                        if front_pops_needed == 0 {
                            break;
                        }
                        front_pops_needed -= 1;
                        price_level_queue.pop_front();
                    }
                }

                if price_level_queue.is_empty() {
                    level_drained = true;
                }
            }

            if level_drained {
                book_to_match.remove(&best_price);
            }
        }
        log::debug!(
            "no more price levels to go through for order #{}",
            taker_order.id
        );
    }

    /// Adds a new market order to the book.
    /// Market orders are filled immediately and are not added to the book.
    pub fn add_market_order(&mut self, side: Side, quantity: Quantity) -> &Vec<Trade> {
        let order_id = self.get_next_order_id();
        // A market order doesn't have a price, but we can model it
        // with a dummy price for the struct.
        let mut order = Order {
            id: order_id,
            side,
            price: 0,
            quantity,
            status: OrderStatus::Open,
        };
        self.match_order(&mut order);
        &self.trades_buffer
    }
}
