use std::net::TcpListener;
use std::sync::mpsc;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

use api_gateway::messages::trading::WireMessage;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    let configuration =
        api_gateway::configuration::get_configuration().expect("Failed to read config file");
    let (command_tx, command_rx) = std::sync::mpsc::channel::<WireMessage>();
    let http_server_listener = TcpListener::bind(format!(
        "{}:{}",
        configuration.application.host, configuration.application.port
    ))
    .expect("Failed to bind http tcp listener");

    let matching_engine_listener = TcpStream::connect(format!(
        "{}:{}",
        configuration.engine.host, configuration.engine.port
    ))
    .await
    .expect("Failed to engine tcp listener");

    tokio::spawn(api_gateway::startup::engine_connection_manager(
        command_rx,
        matching_engine_listener,
    ));

    api_gateway::startup::run_http(http_server_listener, command_tx)?.await
}
