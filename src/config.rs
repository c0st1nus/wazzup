use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub client_database_url_template: String,
    pub public_url: Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self, config::ConfigError> {
        let cfg = config::Config::builder()
            .add_source(config::Environment::default())
            .build()?;
        cfg.try_deserialize()
    }
}