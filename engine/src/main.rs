use engine::{
    configuration::get_configuration, event_queue::queue_loop,
    matching_engine::matching_engine_loop, messages::trading::wire_message::Payload,
};
use tokio::{
    io::BufReader,
    net::{TcpListener, TcpStream},
};

async fn handle_connection(stream: TcpStream, command_tx: tokio::sync::mpsc::Sender<Payload>) {
    let mut reader = BufReader::new(stream);
    loop {
        match engine::transport::read_command(&mut reader).await {
            Ok(payload) => {
                if command_tx.send(payload).await.is_err() {
                    break;
                }
            }
            Err(error) => {
                log::debug!("closing command connection: {error}");
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

    let (command_tx, command_rx) = tokio::sync::mpsc::channel::<Payload>(1024);
    let (event_tx, event_rx) = tokio::sync::mpsc::channel::<Payload>(1024);
    let listener =
        TcpListener::bind((config.application.host.as_str(), config.application.port)).await?;

    let _engine_handle = std::thread::spawn(move || {
        log::info!("starting matching engine");
        matching_engine_loop(command_rx, event_tx, config.application);
    });

    let _event_queue_handle = tokio::spawn(async move {
        if let Err(error) = queue_loop(event_rx, config.amqp).await {
            log::error!("event publisher failed: {error}; stopping engine");
            std::process::exit(1);
        }
    });

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
}
