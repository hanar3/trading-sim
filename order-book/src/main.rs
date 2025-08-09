#![allow(unused)]

use futures_lite::stream::StreamExt;
use lapin::{
    self, ConnectionProperties,
    options::{BasicAckOptions, BasicConsumeOptions},
    types::FieldTable,
};
use log;
use order_book::{
    book::OrderBook,
    configuration::get_configuration,
    defs::items::{Order, Side},
};
use std::collections::{BTreeMap, VecDeque};
use tracing;

#[tokio::main]
async fn main() -> lapin::Result<()> {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    let config = get_configuration().expect("Failed to read config file");
    let mut book = OrderBook::new();

    let conn = lapin::Connection::connect(
        config.amqp.connection_string().as_str(),
        ConnectionProperties::default(),
    )
    .await?;
    log::info!("Conncted to amqp: {}", config.amqp.host);

    let channel = conn.create_channel().await?;

    log::info!("Created amqp channel");
    let mut consumer = channel
        .basic_consume(
            &config.amqp.channel,
            &config.amqp.consumer_tag,
            BasicConsumeOptions {
                no_ack: false,
                ..Default::default()
            },
            FieldTable::default(),
        )
        .await?;
    log::info!(
        "will consume. channel = {}, consumer_tag = {}",
        config.amqp.channel,
        config.amqp.consumer_tag
    );

    while let Some(data) = consumer.next().await {
        if let Ok(delivery) = data {
            delivery.ack(BasicAckOptions::default()).await;
            if let Ok(msg) = prost::Message::decode(&delivery.data[..]) {
                let order = Order::from(msg);
                log::info!("new order received from queue > {:?}", order);
                book.add_limit_order(order.side(), order.price, order.quantity);
            }
        }
    }
    Ok(())
}
