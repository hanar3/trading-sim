use std::sync::mpsc::{Receiver, Sender};

use crate::{
    book::OrderBook,
    messages::trading::{OrderAccepted, TradeOccurred, WireMessage, wire_message::Payload},
};

pub fn matching_engine_loop(command_rx: Receiver<Payload>, event_tx: Sender<Payload>) {
    let mut book = OrderBook::new();
    log::info!("matching engine started, ready to receive commands");
    for command in command_rx {
        log::info!("Matching engine received event {:?}", command);

        match command {
            Payload::PlaceLimitOrder(order) => {
                let (order_id, trades) =
                    book.add_limit_order(order.side(), order.price, order.quantity);

                event_tx
                    .send(Payload::OrderAccepted(OrderAccepted {
                        order_id,
                        user_id: order.user_id,
                        side: order.side,
                        price: order.price,
                        quantity: order.quantity,
                    }))
                    .unwrap(); // TODO: handle the error

                for trade in trades {
                    event_tx
                        .send(Payload::TradeOccurred(TradeOccurred {
                            taker_order_id: trade.taker_order_id,
                            maker_order_id: trade.maker_order_id,
                            price: trade.price,
                            quantity: trade.quantity,
                        }))
                        .unwrap(); // TODO: handle the error
                }
            }
            _ => {
                // This will only handle input messages
            }
        };
    }
}
