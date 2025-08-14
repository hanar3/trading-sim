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

pub async fn queue_loop(command_tx: Sender<Payload>, config: AmqpSettings) -> lapin::Result<()> {
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
        "will consume. channel = {}, consumer_tag = {}",
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
                            Payload::PlaceLimitOrder(order) => {
                                log::info!("decoded place limit order message: {:?}", order);
                                let res = command_tx.send(payload).unwrap();
                                log::info!("command_tx send result {:?}", res);
                            }
                            _ => {
                                log::error!(
                                    "impossible to handle payload, not a valid command: {:?}",
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
