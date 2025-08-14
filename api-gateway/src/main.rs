use std::net::TcpListener;

use api_gateway::configuration::get_configuration;

#[tokio::main]
async fn main() {
    let configuration = get_configuration();
}
