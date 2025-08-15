use crate::messages::trading::WireMessage;
use crate::routes::order;
use actix_web::middleware::Logger;
use actix_web::{App, HttpServer, dev::Server, web};
use prost::Message;
use std::net::TcpListener;
use std::sync::mpsc::{self, Sender};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tracing_actix_web::TracingLogger;

pub fn run_http(
    listener: TcpListener,
    command_tx: Sender<WireMessage>,
) -> Result<Server, std::io::Error> {
    let sender = web::Data::new(command_tx);
    let server = HttpServer::new(move || {
        App::new()
            .wrap(TracingLogger::default())
            .route("/orders", web::post().to(order::place_limit_order))
            .app_data(sender.clone())
    })
    .listen(listener)?
    .run();

    Ok(server)
}

pub async fn engine_connection_manager(
    receiver: mpsc::Receiver<WireMessage>,
    mut stream: TcpStream,
) {
    for command in receiver {
        let mut buf = Vec::new();
        command.encode(&mut buf).unwrap();

        stream.write_u32(buf.len() as u32).await.unwrap();
        if let Err(e) = stream.write_all(&buf).await {
            eprintln!("Failed to send command to matching engine: {}", e);
            break;
        }
    }
}
