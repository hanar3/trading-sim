use crate::{
    book::OrderBook,
    configuration::ApplicationSettings,
    messages::trading::{OrderAccepted, OrderCancelled, TradeOccurred, wire_message::Payload},
};

pub fn matching_engine_loop(
    mut command_rx: tokio::sync::mpsc::Receiver<Payload>,
    event_tx: tokio::sync::mpsc::Sender<Payload>,
    config: ApplicationSettings,
) {
    let mut book = OrderBook::new();
    log::info!("matching engine started, ready to receive commands");
    while let Some(command) = command_rx.blocking_recv() {
        log::info!("Matching engine received event {:?}", command);

        match command {
            Payload::PlaceLimitOrder(order) => {
                if order.base_currency != config.base_currency.name
                    || order.quote_currency != config.quote_currency.name
                {
                    log::warn!("rejected unsupported currency pair");
                    continue;
                }
                let Ok(side) = crate::messages::trading::Side::try_from(order.side) else {
                    log::warn!("rejected invalid side");
                    continue;
                };
                let (order_id, trades) =
                    match book.add_limit_order(side, order.price, order.quantity) {
                        Ok(result) => result,
                        Err(reason) => {
                            log::warn!("rejected limit order: {reason}");
                            continue;
                        }
                    };

                if event_tx
                    .blocking_send(Payload::OrderAccepted(OrderAccepted {
                        order_id,
                        client_order_id: order.client_order_id,
                        user_id: order.user_id,
                        side: order.side,
                        price: order.price,
                        quantity: order.quantity,
                        base_currency: config.base_currency.name.clone(),
                        quote_currency: config.quote_currency.name.clone(),
                    }))
                    .is_err()
                {
                    return;
                }

                for trade in trades {
                    if event_tx
                        .blocking_send(Payload::TradeOccurred(TradeOccurred {
                            taker_order_id: trade.taker_order_id,
                            maker_order_id: trade.maker_order_id,
                            price: trade.price,
                            quantity: trade.quantity,
                        }))
                        .is_err()
                    {
                        return;
                    }
                }
            }
            Payload::CancelOrder(request) => match book.cancel_order(request.order_id) {
                Ok(_) => {
                    if event_tx
                        .blocking_send(Payload::OrderCancelled(OrderCancelled {
                            order_id: request.order_id,
                        }))
                        .is_err()
                    {
                        return;
                    }
                }
                Err(err) => {
                    log::error!("failed to cancel order {}: {}", request.order_id, err);
                    // send an error event upstream?
                }
            },

            _ => {
                // This will only handle input messages
            }
        };
    }
}
