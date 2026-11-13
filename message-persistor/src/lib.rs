pub mod amqp_receiver;
pub mod configuration;
pub mod messages;

/// Apply the same versioned schema in development, tests, and production.
pub async fn migrate(pool: &sqlx::SqlitePool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("../migrations").run(pool).await
}
