#![allow(unused)]
use crate::defs::items::Side;
use std::{
    collections::{BTreeMap, VecDeque},
    time::Duration,
};

type Price = u64;
type Quantity = u64;
type OrderId = u64;

#[derive(Debug, Clone)]
pub struct Order {
    id: OrderId,
    side: Side,
    price: Price,
    quantity: Quantity,
}

#[derive(Debug, Clone)]
pub struct Trade {
    taker_order_id: OrderId,
    maker_order_id: OrderId,
    quantity: Quantity,
    price: Price,
}

#[derive(Debug, Clone)]
pub struct OrderBook {
    bids: BTreeMap<Price, VecDeque<Order>>,
    asks: BTreeMap<Price, VecDeque<Order>>,
    next_order_id: OrderId,
}

// BTC-USD
impl OrderBook {
    pub fn new() -> Self {
        OrderBook {
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            next_order_id: 1,
        }
    }

    fn get_next_order_id(&mut self) -> OrderId {
        let id = self.next_order_id;
        self.next_order_id += 1;
        id
    }

    pub fn add_limit_order(&mut self, side: Side, price: Price, quantity: Quantity) -> Vec<Trade> {
        let order_id = self.get_next_order_id();
        log::info!("next order id > {}", order_id);
        let mut order = Order {
            id: order_id,
            side,
            price,
            quantity,
        };

        let trades = self.match_order(&mut order);

        if order.quantity > 0 {
            let book_side = match order.side {
                Side::Buy => &mut self.bids,
                Side::Sell => &mut self.asks,
            };

            // Create a new price level
            book_side
                .entry(order.price)
                .or_insert_with(VecDeque::new)
                .push_back(order);
        }

        trades
    }

    pub fn match_order(&mut self, taker_order: &mut Order) -> Vec<Trade> {
        log::info!("matching order # = {}", taker_order.id);
        let mut trades = Vec::new();
        let mut empty_price_levels = Vec::new();

        let (book_to_match, is_bid_match) = match taker_order.side {
            Side::Buy => {
                log::info!(
                    "side is buy, matching order # {} against asks",
                    taker_order.id
                );
                (&mut self.asks, false)
            }
            Side::Sell => {
                log::info!(
                    "side is sell, matching order # {} against bids",
                    taker_order.id
                );
                (&mut self.bids, true)
            }
        };

        println!("{:?}", book_to_match.keys());
        let price_levels: Vec<Price> = if is_bid_match {
            book_to_match.keys().rev().cloned().collect()
        } else {
            book_to_match.keys().cloned().collect()
        };

        for price in price_levels {
            log::info!(
                "attempting to match against price level: [{}] for order # {}, side: {}",
                price,
                taker_order.id,
                taker_order.side.as_str_name()
            );

            if taker_order.quantity == 0 {
                log::info!("order filled completely: {}", taker_order.id);
                break; // Order fully filled
            }

            log::info!("checking if it's impossible to fill order given current book");
            match taker_order.side {
                Side::Buy if taker_order.price < price => break,
                Side::Sell if taker_order.price > price => break,
                _ => (),
            }
            log::info!(
                "possible to fill order # {} at price level {}",
                taker_order.id,
                taker_order.price
            );

            let price_level_queue = book_to_match.get_mut(&price).unwrap();
            while let Some(maker_order) = price_level_queue.front_mut() {
                if taker_order.quantity == 0 {
                    break;
                }

                let trade_quantity = taker_order.quantity.min(maker_order.quantity);
                log::info!(
                    "filled qty {} for taker_order # {} and maker_order {}",
                    trade_quantity,
                    taker_order.id,
                    maker_order.id
                );

                trades.push(Trade {
                    taker_order_id: taker_order.id,
                    maker_order_id: maker_order.id,
                    quantity: trade_quantity,
                    price: maker_order.price,
                });

                taker_order.quantity -= trade_quantity;
                maker_order.quantity -= trade_quantity;

                if maker_order.quantity == 0 {
                    price_level_queue.pop_front();
                }
            }

            if price_level_queue.is_empty() {
                empty_price_levels.push(price);
            }
        }
        log::info!(
            "no more price levels to go through for order #{}",
            taker_order.id
        );

        for price in empty_price_levels {
            book_to_match.remove(&price);
        }

        trades
    }

    /// Adds a new market order to the book.
    /// Market orders are filled immediately and are not added to the book.
    pub fn add_market_order(&mut self, side: Side, quantity: Quantity) -> Vec<Trade> {
        let order_id = self.get_next_order_id();
        // A market order doesn't have a price, but we can model it
        // with a dummy price for the struct.
        let mut order = Order {
            id: order_id,
            side,
            price: 0,
            quantity,
        };
        self.match_order(&mut order)
    }
    /// Displays the current state of the order book.
    pub fn display(&self) {
        println!("\n--- ORDER BOOK ---");
        println!("      ASKS");
        println!("Price    | Quantity");
        println!("-------------------");
        // Display top 5 asks (lowest price first)
        for (&price, queue) in self.asks.iter().take(5) {
            let total_quantity: Quantity = queue.iter().map(|o| o.quantity).sum();
            println!("{:<8} | {}", price, total_quantity);
        }

        println!("\n      BIDS");
        println!("Price    | Quantity");
        println!("-------------------");
        // Display top 5 bids (highest price first)
        for (&price, queue) in self.bids.iter().rev().take(5) {
            let total_quantity: Quantity = queue.iter().map(|o| o.quantity).sum();
            println!("{:<8} | {}", price, total_quantity);
        }
        println!("-------------------\n");
    }
}
