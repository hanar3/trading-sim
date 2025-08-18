use config;
use secrecy::{ExposeSecret, SecretBox};
use serde_aux::field_attributes::deserialize_number_from_string;
use sqlx::sqlite::SqliteConnectOptions;

#[derive(serde::Deserialize, Debug)]
pub struct Settings {
    pub amqp: AmqpSettings,
    pub database: DatabaseSettings,
}

#[derive(serde::Deserialize, Debug)]
pub struct AmqpSettings {
    pub host: String,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub port: u16,
    pub username: String,
    pub password: SecretBox<String>,
    pub channel: String,
    pub consumer_tag: String,
}

#[derive(serde::Deserialize, Debug)]
pub struct DatabaseSettings {
    pub file: String,
}

impl DatabaseSettings {
    pub fn get_config(&self) -> SqliteConnectOptions {
        let base_path = std::env::current_dir().expect("Failed to determine the current directory");
        let db_file = base_path.join(&self.file);
        return SqliteConnectOptions::default().filename(db_file);
    }
}

impl AmqpSettings {
    pub fn connection_string(&self) -> String {
        format!(
            "amqp://{}:{}@{}:{}", // ampq://u:p@host:port
            self.username,
            self.password.expose_secret(),
            self.host,
            self.port
        )
    }
}

pub fn get_configuration() -> Result<Settings, config::ConfigError> {
    // Initialise our configuration reader
    let mut settings = config::Config::default();
    let base_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let config_dir = base_path.join("configuration");

    // Read the default config
    settings.merge(config::File::from(config_dir.join("base")).required(true))?;
    let environment: Environment = std::env::var("APP_ENVIRONMENT")
        .unwrap_or_else(|_| "local".into())
        .try_into()
        .expect("Failed to parse APP_ENVIRONMENT");

    settings.merge(config::File::from(config_dir.join(environment.as_str())).required(true))?;

    // Add in settings from environment variables (with a prefix of APP and '__' as separator)
    // E.g. `APP_APPLICATION__PORT=5001 would set `Settings.application.port`
    settings.merge(config::Environment::with_prefix("app").separator("__"))?;
    settings.try_into()
}

pub enum Environment {
    Local,
    Production,
}

impl Environment {
    pub fn as_str(&self) -> &'static str {
        match self {
            Environment::Local => "local",
            Environment::Production => "production",
        }
    }
}

impl TryFrom<String> for Environment {
    type Error = String;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.to_lowercase().as_str() {
            "local" => Ok(Self::Local),
            "production" => Ok(Self::Production),
            other => Err(format!(
                "{} is not a supported environment. Use either `local` or `production`",
                other
            )),
        }
    }
}
