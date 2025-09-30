use chrono_tz::Tz;
use serde::Deserialize;
use std::env;
use std::str::FromStr;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub webhook_port: Option<u16>,
    pub public_url: Option<String>,
    pub timezone: Option<String>,
    pub max_body_bytes: Option<usize>,
}

impl Config {
    pub fn from_env() -> Result<Self, config::ConfigError> {
        dotenvy::dotenv().ok();

        let cfg = config::Config::builder()
            .add_source(config::Environment::default())
            .build()?;

        let mut config: Config = cfg.try_deserialize()?;

        // Устанавливаем значение по умолчанию для timezone если не указано
        if config.timezone.is_none() {
            config.timezone = Some("UTC".to_string());
        }

        // Валидация конфигурации
        config.validate()?;

        Ok(config)
    }

    /// Получает временную зону из конфигурации
    pub fn get_timezone(&self) -> Result<Tz, chrono_tz::ParseError> {
        let tz_str = self.timezone.as_deref().unwrap_or("UTC");
        tz_str.parse::<Tz>()
    }

    /// Валидирует конфигурацию на наличие потенциальных проблем безопасности
    fn validate(&self) -> Result<(), config::ConfigError> {
        // Проверяем, что host не содержит подозрительных символов
        if !self
            .host
            .chars()
            .all(|c| c.is_alphanumeric() || ".:-_".contains(c))
        {
            return Err(config::ConfigError::Message(
                "Invalid host format".to_string(),
            ));
        }

        // Проверяем разумные ограничения для порта (u16 максимум 65535)
        if self.port < 1024 {
            return Err(config::ConfigError::Message(
                "Port must be 1024 or higher for security reasons".to_string(),
            ));
        }

        if let Some(webhook_port) = self.webhook_port {
            if webhook_port < 1024 {
                return Err(config::ConfigError::Message(
                    "Webhook port must be 1024 or higher for security reasons".to_string(),
                ));
            }

            if webhook_port == self.port {
                return Err(config::ConfigError::Message(
                    "Webhook port must differ from main port".to_string(),
                ));
            }
        }

        // Валидируем временную зону
        if let Some(tz_str) = &self.timezone {
            if tz_str.parse::<Tz>().is_err() {
                return Err(config::ConfigError::Message(format!(
                    "Invalid timezone: {}",
                    tz_str
                )));
            }
        }

        // Валидируем лимит тела (если указан): 1MB..500MB
        if let Some(limit) = self.max_body_bytes {
            let min = 1 * 1024 * 1024; // 1MB
            let max = 500 * 1024 * 1024; // 500MB
            if limit < min || limit > max {
                return Err(config::ConfigError::Message(format!(
                    "max_body_bytes must be between {} and {} bytes",
                    min, max
                )));
            }
        }

        Ok(())
    }
}

impl Config {
    pub fn effective_max_body_bytes(&self) -> usize {
        self.max_body_bytes.unwrap_or(100 * 1024 * 1024)
    }

    pub fn effective_webhook_port(&self) -> u16 {
        self.webhook_port.unwrap_or(3245)
    }
}

#[derive(Debug, Clone)]
pub struct DatabaseSettings {
    pub url: String,
    pub max_connections: Option<u32>,
    pub min_connections: Option<u32>,
    pub connect_timeout_secs: Option<u64>,
    pub acquire_timeout_secs: Option<u64>,
    pub idle_timeout_secs: Option<u64>,
    pub sql_log: Option<bool>,
}

impl DatabaseSettings {
    pub fn default_from_url(url: String) -> Self {
        Self {
            url,
            max_connections: parse_env_var("DATABASE_MAX_CONNECTIONS"),
            min_connections: parse_env_var("DATABASE_MIN_CONNECTIONS"),
            connect_timeout_secs: parse_env_var("DATABASE_CONNECT_TIMEOUT_SECS"),
            acquire_timeout_secs: parse_env_var("DATABASE_ACQUIRE_TIMEOUT_SECS"),
            idle_timeout_secs: parse_env_var("DATABASE_IDLE_TIMEOUT_SECS"),
            sql_log: parse_env_var("DATABASE_SQL_LOG"),
        }
    }
}

fn parse_env_var<T>(key: &str) -> Option<T>
where
    T: FromStr,
{
    env::var(key).ok().and_then(|value| value.parse::<T>().ok())
}
