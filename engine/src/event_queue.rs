use crate::{
    configuration::AmqpSettings,
    messages::trading::{WireMessage, wire_message::Payload},
};
use lapin::{
    self, BasicProperties, ConnectionProperties,
    options::{BasicPublishOptions, ConfirmSelectOptions, QueueDeclareOptions},
};
use prost::Message;
use tokio::sync::mpsc::Receiver;

pub async fn queue_loop(
    mut event_rx: Receiver<Payload>,
    config: AmqpSettings,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
            Default::default(),
        )
        .await?;
    channel
        .confirm_select(ConfirmSelectOptions::default())
        .await?;
    log::info!("Created amqp channel");

    log::info!(
        "channel: {}, consumer-tag: {}",
        config.channel,
        config.consumer_tag
    );

    while let Some(event) = event_rx.recv().await {
        let wire_message = WireMessage {
            payload: Some(event),
        };
        let bytes = wire_message.encode_to_vec();
        let confirmation = channel
            .basic_publish(
                "",
                &config.channel,
                BasicPublishOptions {
                    mandatory: true,
                    ..Default::default()
                },
                &bytes,
                BasicProperties::default()
                    .with_delivery_mode(2)
                    .with_message_id(uuid::Uuid::new_v4().to_string().into()),
            )
            .await?
            .await?;
        if !matches!(
            confirmation,
            lapin::publisher_confirm::Confirmation::Ack(None)
        ) {
            log::error!("broker rejected or returned event: {confirmation:?}");
            // Do not silently continue after losing an event.
            return Err(std::io::Error::other("event was not confirmed and routed").into());
        }
    }
    Ok(())
}
