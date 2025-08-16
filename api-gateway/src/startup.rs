use crate::messages::trading::WireMessage;
use crate::routes::order;
use actix_web::{App, HttpServer, dev::Server, web};
use prost::Message;
use rand::Rng;
use socket2::TcpKeepalive;
use std::net::TcpListener;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tracing_actix_web::TracingLogger;

pub fn run_http(
    listener: TcpListener,
    command_tx: tokio::sync::mpsc::Sender<WireMessage>,
) -> Result<Server, std::io::Error> {
    let sender = web::Data::new(command_tx);
    let server = HttpServer::new(move || {
        App::new()
            .wrap(TracingLogger::default())
            .route("/orders", web::post().to(order::place_limit_order))
            .route("/orders", web::delete().to(order::cancel_order))
            .app_data(sender.clone())
    })
    .listen(listener)?
    .run();

    Ok(server)
}

fn keepalive(stream: TcpStream) -> TcpStream {
    // sigh...tokio removed the set_keepalive so now we have to write 10 more lines of
    // code just to get the socket to die quicker!
    let stream: std::net::TcpStream = stream.into_std().unwrap();
    let socket: socket2::Socket = socket2::Socket::from(stream);
    let keepalive = TcpKeepalive::new()
        .with_time(tokio::time::Duration::from_secs(4)) // send keepalive probes after 4s
        .with_interval(tokio::time::Duration::from_secs(1)) // interval between keepalive probes
        .with_retries(4); // retries until connection drop

    socket.set_tcp_keepalive(&keepalive).unwrap();
    let stream: std::net::TcpStream = socket.into();
    let stream: tokio::net::TcpStream = tokio::net::TcpStream::from_std(stream).unwrap();
    return stream;
}

pub async fn engine_connection_manager(
    mut receiver: tokio::sync::mpsc::Receiver<WireMessage>,
    engine_addr: String,
) {
    let mut backoff = tokio::time::Duration::from_millis(100);
    const MAX_BACKOFF: tokio::time::Duration = tokio::time::Duration::from_secs(30);

    loop {
        log::info!("attempting to connect to {}", &engine_addr);
        match TcpStream::connect(&engine_addr).await {
            Ok(stream) => {
                log::info!("connected to matching engine");
                backoff = tokio::time::Duration::from_millis(100);
                let mut stream = keepalive(stream);

                while let Some(command) = receiver.recv().await {
                    let mut buf = Vec::new();
                    command.encode(&mut buf).unwrap();

                    if let Err(e) = stream.write_u32(buf.len() as u32).await {
                        log::error!(
                            "failed to write length onto stream, connection closed: {}",
                            e
                        );
                        break;
                    }
                    if let Err(e) = stream.write_all(&buf).await {
                        log::error!("Failed to send command to matching engine: {}", e);
                        break;
                    }
                }
            }
            Err(e) => {
                log::error!("Failed to connect: {}. Retrying in {:?}...", e, backoff);
                tokio::time::sleep(backoff).await;

                backoff = (backoff * 2).min(MAX_BACKOFF);
                let jitter = tokio::time::Duration::from_millis(rand::rng().random_range(0..100));
                backoff += jitter;
            }
        }
    }
}
