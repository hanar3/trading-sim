use message_persistor::amqp_receiver;
use sqlx::sqlite::SqlitePoolOptions;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();
    let configuration = message_persistor::configuration::get_configuration()
        .expect("failed to get configuration from file");

    let connection_pool =
        SqlitePoolOptions::new().connect_lazy_with(configuration.database.get_config());
    amqp_receiver::amqp_receiver(connection_pool, configuration.amqp)
        .await
        .expect("failed to establish queue loop");

    Ok(())
}
