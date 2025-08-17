use std::path::{self, PathBuf};

use sqlx::{
    Executor, SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let configuration = message_persistor::configuration::get_configuration()
        .expect("failed to get configuration from file");
    let opts = SqliteConnectOptions::default().filename(PathBuf::from(configuration.database.file));
    let connection_pool = SqlitePoolOptions::new().connect_lazy_with(opts);
    Ok(())
}
