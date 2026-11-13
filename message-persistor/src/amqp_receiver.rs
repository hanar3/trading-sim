use crate::{
    configuration::AmqpSettings,
    messages::trading::{WireMessage, wire_message::Payload},
};
use futures_lite::stream::StreamExt;
use lapin::{
    self, ConnectionProperties,
    options::{
        BasicAckOptions, BasicConsumeOptions, BasicNackOptions, BasicQosOptions,
        QueueDeclareOptions,
    },
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
    OutOfRange,
    InvalidEventId,
}

impl std::fmt::Display for HandleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Decode(error) => write!(formatter, "protobuf decode: {error}"),
            Self::Database(error) => write!(formatter, "database: {error}"),
            Self::UnexpectedPayload => formatter.write_str("unexpected event type"),
            Self::MissingPayload => formatter.write_str("missing payload"),
            Self::OutOfRange => formatter.write_str("integer exceeds SQLite range"),
            Self::InvalidEventId => {
                formatter.write_str("missing, invalid, or conflicting event ID")
            }
        }
    }
}

async fn handle_payload(
    pool: &SqlitePool,
    event_id: &str,
    bytes: &[u8],
) -> Result<(), HandleError> {
    if event_id.is_empty() || event_id.len() > 128 {
        return Err(HandleError::InvalidEventId);
    }
    let mut transaction = pool.begin().await.map_err(HandleError::Database)?;
    let inserted = sqlx::query("INSERT INTO processed_events (event_id, payload) VALUES (?, ?) ON CONFLICT(event_id) DO NOTHING")
        .bind(event_id).bind(bytes).execute(&mut *transaction).await.map_err(HandleError::Database)?;
    if inserted.rows_affected() == 0 {
        let previous: Vec<u8> =
            sqlx::query_scalar("SELECT payload FROM processed_events WHERE event_id = ?")
                .bind(event_id)
                .fetch_one(&mut *transaction)
                .await
                .map_err(HandleError::Database)?;
        return if previous == bytes {
            Ok(())
        } else {
            Err(HandleError::InvalidEventId)
        };
    }
    let wire_message = WireMessage::decode(bytes).map_err(HandleError::Decode)?;

    match wire_message.payload {
        Some(Payload::OrderAccepted(order)) => {
            let new_order = NewOrder {
                order_id: checked(order.order_id)?,
                client_order_id: order.client_order_id,
                base_currency: order.base_currency,
                quote_currency: order.quote_currency,
                side: order.side,
                quantity: checked(order.quantity)?,
                price: checked(order.price)?,
            };

            insert_order(&mut transaction, new_order)
                .await
                .map_err(HandleError::Database)?;
        }
        Some(Payload::TradeOccurred(trade)) => {
            let new_trade = NewTrade {
                maker_order_id: checked(trade.maker_order_id)?,
                taker_order_id: checked(trade.taker_order_id)?,
                filled_qty: checked(trade.quantity)?,
                price: checked(trade.price)?,
            };

            insert_trade(&mut transaction, new_trade)
                .await
                .map_err(HandleError::Database)?;
        }
        Some(Payload::OrderCancelled(order)) => {
            let result = sqlx::query("UPDATE orders SET cancelled = 1 WHERE order_id = ?")
                .bind(checked(order.order_id)?)
                .execute(&mut *transaction)
                .await
                .map_err(HandleError::Database)?;
            if result.rows_affected() != 1 {
                return Err(HandleError::Database(sqlx::Error::RowNotFound));
            }
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
    transaction.commit().await.map_err(HandleError::Database)?;
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

    channel
        .queue_declare(
            &config.channel,
            QueueDeclareOptions {
                durable: true,
                ..Default::default()
            },
            FieldTable::default(),
        )
        .await?;
    channel.basic_qos(64, BasicQosOptions::default()).await?;
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

        let event_id = delivery
            .properties
            .message_id()
            .as_ref()
            .map(|id| id.as_str())
            .unwrap_or("");
        // Retry transient database failures in place: requeueing at the tail could
        // let a trade overtake the accepted orders its foreign keys depend on.
        let result = loop {
            let result = handle_payload(&pool, event_id, &delivery.data).await;
            let retry = match &result {
                Err(HandleError::Database(sqlx::Error::Database(error))) => matches!(
                    error.code().as_deref(),
                    Some("5" | "6" | "SQLITE_BUSY" | "SQLITE_LOCKED")
                ),
                Err(HandleError::Database(sqlx::Error::Io(_) | sqlx::Error::PoolTimedOut)) => true,
                _ => false,
            };
            if !retry {
                break result;
            }
            log::warn!("transient database failure; retrying current event in order");
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        };
        match result {
            Ok(_) => {
                log::info!("message processed and persisted");
                if let Err(e) = delivery.ack(BasicAckOptions::default()).await {
                    log::error!("failed to ack message: {}", e);
                }
            }
            Err(err) => {
                log::error!("failed to handle payload: {}, nacking", err);

                if let Err(e) = delivery
                    .nack(BasicNackOptions {
                        requeue: false,
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

fn checked(value: u64) -> Result<i64, HandleError> {
    i64::try_from(value).map_err(|_| HandleError::OutOfRange)
}

#[derive(Debug, Clone)]
pub struct NewTrade {
    maker_order_id: i64,
    taker_order_id: i64,
    filled_qty: i64,
    price: i64,
}

async fn insert_trade(
    connection: &mut sqlx::SqliteConnection,
    trade: NewTrade,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO trades (maker_order_id, taker_order_id, filled_qty, price) VALUES (?, ?, ?, ?)")
        .bind(trade.maker_order_id)
        .bind(trade.taker_order_id)
        .bind(trade.filled_qty)
        .bind(trade.price)
        .execute(connection).await?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct NewOrder {
    client_order_id: String,
    order_id: i64,
    base_currency: String,
    quote_currency: String,
    side: i32,
    quantity: i64,
    price: i64,
}

async fn insert_order(
    connection: &mut sqlx::SqliteConnection,
    order: NewOrder,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO orders (order_id, base_currency, quote_currency, side, quantity, price, client_order_id) VALUES (?, ?, ?, ?, ?, ?, ?)")
        .bind(order.order_id)
        .bind(order.base_currency)
        .bind(order.quote_currency)
        .bind(order.side)
        .bind(order.quantity)
        .bind(order.price)
        .bind(order.client_order_id)
        .execute(connection).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::trading::{OrderAccepted, OrderCancelled, TradeOccurred};
    use sqlx::{Row, sqlite::SqlitePoolOptions};

    async fn database() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        crate::migrate(&pool).await.unwrap();
        pool
    }

    async fn deliver(pool: &SqlitePool, payload: Payload) -> Result<(), HandleError> {
        let bytes = WireMessage {
            payload: Some(payload),
        }
        .encode_to_vec();
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut hasher);
        let id = format!("test-{}", hasher.finish());
        handle_payload(pool, &id, &bytes).await
    }

    fn order(id: u64, side: i32) -> Payload {
        Payload::OrderAccepted(OrderAccepted {
            client_order_id: String::new(),
            order_id: id,
            user_id: id,
            side,
            price: 5_000_000_000,
            quantity: 4_000_000_000,
            base_currency: "BTC".into(),
            quote_currency: "USD".into(),
        })
    }

    #[tokio::test]
    async fn persists_large_values_trade_identity_price_and_cancellation() {
        let pool = database().await;
        deliver(&pool, order(3_000_000_000, 2)).await.unwrap();
        deliver(&pool, order(3_000_000_001, 1)).await.unwrap();
        deliver(
            &pool,
            Payload::TradeOccurred(TradeOccurred {
                maker_order_id: 3_000_000_000,
                taker_order_id: 3_000_000_001,
                quantity: 4_000_000_000,
                price: 5_000_000_000,
            }),
        )
        .await
        .unwrap();
        let row =
            sqlx::query("SELECT maker_order_id, taker_order_id, filled_qty, price FROM trades")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(row.get::<i64, _>("maker_order_id"), 3_000_000_000);
        assert_eq!(row.get::<i64, _>("taker_order_id"), 3_000_000_001);
        assert_eq!(row.get::<i64, _>("filled_qty"), 4_000_000_000);
        assert_eq!(row.get::<i64, _>("price"), 5_000_000_000);
        deliver(
            &pool,
            Payload::OrderCancelled(OrderCancelled {
                order_id: 3_000_000_000,
            }),
        )
        .await
        .unwrap();
        let row = sqlx::query(
            "SELECT quantity, price, cancelled FROM orders WHERE order_id = 3000000000",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.get::<i64, _>("quantity"), 4_000_000_000);
        assert_eq!(row.get::<i64, _>("price"), 5_000_000_000);
        assert_eq!(row.get::<i64, _>("cancelled"), 1);
    }

    #[tokio::test]
    async fn out_of_range_values_and_malformed_messages_do_not_write() {
        let pool = database().await;
        assert!(matches!(
            deliver(&pool, order(u64::MAX, 1)).await,
            Err(HandleError::OutOfRange)
        ));
        assert!(matches!(
            handle_payload(&pool, "malformed", &[255]).await,
            Err(HandleError::Decode(_))
        ));
        assert!(matches!(
            handle_payload(&pool, "missing", &[]).await,
            Err(HandleError::MissingPayload)
        ));
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM orders")
                .fetch_one(&pool)
                .await
                .unwrap(),
            0
        );
    }
    #[tokio::test]
    async fn duplicate_delivery_is_idempotent_and_failed_events_roll_back_receipts() {
        let pool = database().await;
        for _ in 0..2 {
            deliver(&pool, order(1, 2)).await.unwrap();
            deliver(&pool, order(2, 1)).await.unwrap();
            deliver(
                &pool,
                Payload::TradeOccurred(TradeOccurred {
                    maker_order_id: 1,
                    taker_order_id: 2,
                    quantity: 2,
                    price: 100,
                }),
            )
            .await
            .unwrap();
            deliver(
                &pool,
                Payload::OrderCancelled(OrderCancelled { order_id: 1 }),
            )
            .await
            .unwrap();
        }
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM trades")
                .fetch_one(&pool)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM processed_events")
                .fetch_one(&pool)
                .await
                .unwrap(),
            4
        );
        let bytes = WireMessage {
            payload: Some(order(3, 1)),
        }
        .encode_to_vec();
        let conflicting = WireMessage {
            payload: Some(order(4, 1)),
        }
        .encode_to_vec();
        handle_payload(&pool, "stable-id", &bytes).await.unwrap();
        assert!(matches!(
            handle_payload(&pool, "stable-id", &conflicting).await,
            Err(HandleError::InvalidEventId)
        ));
        let missing = Payload::OrderCancelled(OrderCancelled { order_id: 9 });
        assert!(deliver(&pool, missing.clone()).await.is_err());
        deliver(&pool, order(9, 1)).await.unwrap();
        deliver(&pool, missing).await.unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT cancelled FROM orders WHERE order_id = 9")
                .fetch_one(&pool)
                .await
                .unwrap(),
            1
        );
    }
    #[tokio::test]
    async fn upgrades_a_versioned_database_without_fabricating_historical_prices() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let mut original = sqlx::migrate!("../migrations");
        original.migrations = std::borrow::Cow::Owned(
            original
                .iter()
                .filter(|migration| migration.version < 20261113000000)
                .cloned()
                .collect(),
        );
        original.run(&pool).await.unwrap();
        sqlx::query("INSERT INTO orders (order_id, base_currency, quote_currency, side, quantity, price) VALUES (1, 'BTC', 'USD', 2, 5, 100), (2, 'BTC', 'USD', 1, 5, 100)").execute(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO trades (maker_order_id, taker_order_id, filled_qty) VALUES (1, 2, 5)",
        )
        .execute(&pool)
        .await
        .unwrap();
        crate::migrate(&pool).await.unwrap();
        crate::migrate(&pool).await.unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, Option<i64>>("SELECT price FROM trades")
                .fetch_one(&pool)
                .await
                .unwrap(),
            None
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT sum(cancelled) FROM orders")
                .fetch_one(&pool)
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM trades")
                .fetch_one(&pool)
                .await
                .unwrap(),
            1
        );
    }
}
