use std::path::{self, PathBuf};

use message_persistor::queue_loop;
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();
    let configuration = message_persistor::configuration::get_configuration()
        .expect("failed to get configuration from file");

    let base_path = std::env::current_dir().expect("Failed to determine the current directory");
    let db_file = base_path.join(configuration.database.file);
    let opts = SqliteConnectOptions::default().filename(db_file);
    let connection_pool = SqlitePoolOptions::new().connect_lazy_with(opts);
    queue_loop::queue_loop(connection_pool, configuration.amqp)
        .await
        .expect("failed to establish queue loop");

    Ok(())
}
