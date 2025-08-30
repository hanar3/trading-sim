use crate::{
    configuration::AmqpSettings,
    messages::trading::{WireMessage, wire_message::Payload},
};
use futures_lite::stream::StreamExt;
use lapin::{
    self, ConnectionProperties,
    options::{BasicAckOptions, BasicConsumeOptions, BasicNackOptions},
    types::FieldTable,
};
use prost::Message;
use sqlx::SqlitePool;

#[derive(Debug)]
enum HandleError {
    Decode(prost::DecodeError),
    Database(sqlx::Error),
    UnexpectedPayload,
    MissingPayload,
}

async fn handle_payload(pool: &SqlitePool, bytes: &[u8]) -> Result<(), HandleError> {
    let wire_message = WireMessage::decode(bytes).map_err(HandleError::Decode)?;

    match wire_message.payload {
        Some(Payload::OrderAccepted(order)) => {
            let new_order = NewOrder {
                order_id: order.order_id as i32,
                base_currency: order.base_currency,
                quote_currency: order.quote_currency,
                side: order.side,
                quantity: order.quantity as i32,
                price: order.price as i32,
            };

            insert_order(&pool, new_order)
                .await
                .map_err(HandleError::Database)?;
        }
        Some(Payload::TradeOccurred(trade)) => {
            let new_trade = NewTrade {
                maker_order_id: trade.taker_order_id as i32,
                taker_order_id: trade.maker_order_id as i32,
                filled_qty: trade.quantity as i32,
            };

            insert_trade(&pool, new_trade)
                .await
                .map_err(HandleError::Database)?;
        }
        Some(_) => {
            log::error!("Received a valid payload, but unexpected payload type");
            return Err(HandleError::UnexpectedPayload);
        }
        None => {
            log::error!("Received a message with no payload");
            return Err(HandleError::MissingPayload);
        }
    }
    Ok(())
}

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

    while let Some(delivery_result) = consumer.next().await {
        let delivery = match delivery_result {
            Ok(d) => d,
            Err(e) => {
                log::error!("failed to receive message from amqp {}", e);
                continue;
            }
        };

        match handle_payload(&pool, &delivery.data).await {
            Ok(_) => {
                log::info!("message processed and persisted");
                if let Err(e) = delivery.ack(BasicAckOptions::default()).await {
                    log::error!("failed to ack message: {}", e);
                }
            }
            Err(err) => {
                log::error!("failed to handle payload: {:?}, nacking", err);
                let requeue = matches!(err, HandleError::Database(_));

                if let Err(e) = delivery
                    .nack(BasicNackOptions {
                        requeue,
                        ..Default::default()
                    })
                    .await
                {
                    log::error!("failed to NACK message {}", e);
                }
            }
        }
    }

    Ok(())
}

#[derive(Debug, Clone)]
pub struct NewTrade {
    maker_order_id: i32,
    taker_order_id: i32,
    filled_qty: i32,
}

pub async fn insert_trade(pool: &SqlitePool, new_trade: NewTrade) -> Result<(), sqlx::Error> {
    log::info!("inserting trade into database {:?}", new_trade);
    sqlx::query!(
        r#"INSERT INTO trades (maker_order_id, taker_order_id, filled_qty) VALUES ($1, $2, $3)"#,
        new_trade.maker_order_id,
        new_trade.taker_order_id,
        new_trade.filled_qty
    )
    .execute(pool)
    .await
    .map_err(|e| {
        log::error!("failed to execute query: {:?}", e);
        e
    })?;

    Ok(())
}

#[derive(Debug, Clone)]
pub struct NewOrder {
    order_id: i32,
    base_currency: String,
    quote_currency: String,
    side: i32,
    quantity: i32,
    price: i32,
}

pub async fn insert_order(pool: &SqlitePool, new_order: NewOrder) -> Result<(), sqlx::Error> {
    log::info!("inserting order into database {:?}", new_order);
    sqlx::query!(
        r#"INSERT INTO orders (order_id, base_currency, quote_currency, side, quantity, price) VALUES ($1, $2, $3, $4, $5, $6)"#,
        new_order.order_id,
        new_order.base_currency,
        new_order.quote_currency,
        new_order.side,
        new_order.quantity,
        new_order.price,
    ).execute(pool).await.map_err(|e| {
        log::error!("failed to execute query: {:?}", e);
        e
    })?;

    Ok(())
}
