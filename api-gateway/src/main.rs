use std::net::TcpListener;

use api_gateway::messages::trading::WireMessage;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    let configuration =
        api_gateway::configuration::get_configuration().expect("Failed to read config file");
    let (command_tx, command_rx) = tokio::sync::mpsc::channel::<WireMessage>(10_000);
    let http_server_listener = TcpListener::bind(format!(
        "{}:{}",
        configuration.application.host, configuration.application.port
    ))
    .expect("Failed to bind http tcp listener");

    tokio::spawn(api_gateway::startup::engine_connection_manager(
        command_rx,
        format!(
            "{}:{}",
            configuration.engine.host, configuration.engine.port
        ),
    ));

    api_gateway::startup::run_http(http_server_listener, command_tx)?.await
}
