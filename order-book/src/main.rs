#![allow(unused)]

use futures_lite::stream::StreamExt;
use lapin::{
    self, ConnectionProperties,
    options::{BasicAckOptions, BasicConsumeOptions},
    types::FieldTable,
};
use log;
use order_book::{
    self, book::OrderBook, command_queue_loop, configuration::get_configuration,
    matching_engine::matching_engine_loop, messages::trading::wire_message::Payload,
};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::mpsc::{Receiver, Sender},
};

use tracing;

fn event_distributor_loop(event_rx: Receiver<Payload>, consumers: Vec<Sender<Payload>>) {
    for event in event_rx {
        log::info!(
            "broadcasting event: {:?} to {} consumers",
            event,
            consumers.len()
        );
        for consumer_tx in &consumers {
            consumer_tx.send(event.clone()).unwrap();
        }
    }
}

#[tokio::main]
async fn main() -> lapin::Result<()> {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    let config = get_configuration().expect("Failed to read config file");

    let (command_tx, command_rx) = std::sync::mpsc::channel::<Payload>();
    let (event_tx, event_rx) = std::sync::mpsc::channel::<Payload>();

    let command_queue_handle = tokio::spawn(async move {
        log::info!("input queue loop initialized");
        command_queue_loop::queue_loop(command_tx, config.amqp).await;
    });

    let engine_handle = std::thread::spawn(move || {
        log::info!("starting matching engine");
        matching_engine_loop(command_rx, event_tx);
    });

    let distributor_handle = std::thread::spawn(move || {
        event_distributor_loop(event_rx, vec![]);
    });

    // loop {}

    Ok(())
}
