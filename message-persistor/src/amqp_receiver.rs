use std::sync::mpsc::Sender;

use crate::{
    configuration::AmqpSettings,
    messages::trading::{WireMessage, wire_message::Payload},
};
use futures_lite::stream::StreamExt;
use lapin::{
    self, ConnectionProperties,
    options::{BasicAckOptions, BasicConsumeOptions},
    types::FieldTable,
};
use prost::Message;
use sqlx::SqlitePool;

pub async fn amqp_receiver(pool: SqlitePool, config: AmqpSettings) -> lapin::Result<()> {
    let conn = lapin::Connection::connect(
        config.connection_string().as_str(),
        ConnectionProperties::default(),
    )
    .await?;

    log::info!("Conncted to amqp: {}", config.host);

    let channel = conn.create_channel().await?;

    log::info!("Created amqp channel");
    let mut consumer = channel
        .basic_consume(
            &config.channel,
            &config.consumer_tag,
            BasicConsumeOptions {
                no_ack: false,
                ..Default::default()
            },
            FieldTable::default(),
        )
        .await?;
    log::info!(
        "ready to receive messages, will consume. channel = {}, consumer_tag = {}",
        config.channel,
        config.consumer_tag
    );

    while let Some(data) = consumer.next().await {
        log::info!("received message from queue");
        if let Ok(delivery) = data {
            if let Ok(_) = delivery.ack(BasicAckOptions::default()).await {
                let bytes = prost::bytes::Bytes::from(delivery.data);

                if let Ok(wire_message) = WireMessage::decode(bytes) {
                    match wire_message.payload {
                        Some(payload) => match payload {
                            Payload::OrderAccepted(order) => {
                                log::info!("decoded place limit order message: {:?}", order);
                                let new_order = NewOrder {
                                    order_id: order.order_id as i32,
                                    symbol: "BTC/USD".to_string(),
                                    side: order.side,
                                    quantity: order.quantity as i32,
                                    price: order.price as i32,
                                };
                                if let Err(err) = insert_order(&pool, new_order).await {
                                    log::error!("failed to persist order {:?}", err)
                                }
                            }
                            _ => {
                                log::error!(
                                    "impossible to handle payload, not a valid event: {:?}",
                                    payload
                                );
                            }
                        },

                        None => {
                            log::error!("message without a payload received");
                        }
                    }
                } else {
                    log::error!("failed to decode wire_message");
                }
            } else {
                log::error!("didn't ack message from queue, not handling message")
            }
        }
    }

    Ok(())
}

#[derive(Debug, Clone)]
pub struct NewOrder {
    order_id: i32,
    symbol: String,
    side: i32,
    quantity: i32,
    price: i32,
}

pub async fn insert_order(pool: &SqlitePool, new_order: NewOrder) -> Result<(), sqlx::Error> {
    log::info!("inserting order into database {:?}", new_order);
    sqlx::query!(
        r#"INSERT INTO orders (order_id, symbol, side, quantity, price) VALUES ($1, $2, $3, $4, $5)"#,
        new_order.order_id,
        new_order.symbol,
        new_order.side,
        new_order.quantity,
        new_order.price,
    ).execute(pool).await.map_err(|e| {
        log::error!("failed to execute query: {:?}", e);
        e
    })?;

    Ok(())
}
