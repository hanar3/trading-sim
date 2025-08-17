#![allow(unused)]

use engine::{
    self,
    book::OrderBook,
    configuration::get_configuration,
    event_queue::queue_loop,
    matching_engine::matching_engine_loop,
    messages::trading::{WireMessage, wire_message::Payload},
};
use futures_lite::stream::StreamExt;

use log;
use prost::Message;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::mpsc::{Receiver, Sender},
};
use tokio::{
    io::{AsyncReadExt, BufReader},
    net::{TcpListener, TcpStream},
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

async fn handle_connection(stream: TcpStream, command_tx: Sender<Payload>) {
    let mut reader = BufReader::new(stream);
    log::info!("new client connected");

    loop {
        match reader.read_u32().await {
            Ok(len) => {
                let mut buf = vec![0; len as usize];
                if let Err(e) = reader.read_exact(&mut buf).await {
                    log::error!("failed to read message payload {:?}", e);
                } else {
                    match WireMessage::decode(buf.as_slice()) {
                        Ok(msg) => {
                            let payload = msg.payload.unwrap();
                            if command_tx.send(payload).is_err() {
                                log::error!("failed to send to engine");
                            }
                        }
                        Err(e) => {
                            log::error!("failed to decode WireMessage, {:?}", e);
                        }
                    }
                }
            }
            Err(e) => {
                log::error!("client disconnected {:?}, closing connection", e);
                break;
            }
        }
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    let config = get_configuration().expect("Failed to read config file");

    let (command_tx, command_rx) = std::sync::mpsc::channel::<Payload>();
    let (event_tx, event_rx) = std::sync::mpsc::channel::<Payload>();
    let (event_queue_tx, event_queue_rx) = std::sync::mpsc::channel::<Payload>();

    let engine_handle = std::thread::spawn(move || {
        log::info!("starting matching engine");
        matching_engine_loop(command_rx, event_tx);
    });

    let distributor_handle = std::thread::spawn(move || {
        event_distributor_loop(event_rx, vec![event_queue_tx]);
    });

    let event_queue_handle = tokio::spawn(async move {
        queue_loop(event_queue_rx, config.amqp).await;
    });

    let listener = TcpListener::bind("127.0.0.1:4000").await?;
    log::info!("started async io listener on main thread");
    loop {
        // Accept a new connection.
        let (socket, _addr) = listener.accept().await?;

        // Clone the sender for the new connection handler.
        let command_tx_clone = command_tx.clone();

        // Spawn a new Tokio task to handle this specific connection.
        // This allows us to handle thousands of connections concurrently.
        tokio::spawn(async move {
            handle_connection(socket, command_tx_clone).await;
        });
    }

    Ok(())
}
