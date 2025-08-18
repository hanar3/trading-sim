use crate::{
    configuration::AmqpSettings,
    messages::trading::{WireMessage, wire_message::Payload},
};
use lapin::{self, BasicProperties, ConnectionProperties, options::BasicPublishOptions};
use prost::Message;
use std::sync::mpsc::Receiver;

pub async fn queue_loop(event_rx: Receiver<Payload>, config: AmqpSettings) -> lapin::Result<()> {
    let conn = lapin::Connection::connect(
        config.connection_string().as_str(),
        ConnectionProperties::default(),
    )
    .await?;

    log::info!("Conncted to amqp: {}", config.host);

    let channel = conn.create_channel().await?;
    log::info!("Created amqp channel");

    log::info!(
        "channel: {}, consumer-tag: {}",
        config.channel,
        config.consumer_tag
    );

    let mut buf = Vec::new();
    let mut wire_message = WireMessage::default();

    for event in event_rx {
        log::info!("received event from engine: {:?}", event);
        buf.clear();
        wire_message.payload = Some(event);
        if let Ok(_) = wire_message.encode(&mut buf) {
            if let Ok(_) = channel
                .basic_publish(
                    "",
                    &config.channel,
                    BasicPublishOptions::default(),
                    &buf,
                    BasicProperties::default(),
                )
                .await
            {
                log::info!("published {:?} to queue", wire_message);
            } else {
                log::error!("failed to publish {:?} to queue", wire_message);
            }
        }
    }

    Ok(())
}
