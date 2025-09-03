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
        
        let config: Config = cfg.try_deserialize()?;
        
        // Валидация конфигурации
        config.validate()?;
        
        Ok(config)
    }
    
    /// Валидирует конфигурацию на наличие потенциальных проблем безопасности
    fn validate(&self) -> Result<(), config::ConfigError> {
        // Проверяем, что host не содержит подозрительных символов
        if !self.host.chars().all(|c| c.is_alphanumeric() || ".:-_".contains(c)) {
            return Err(config::ConfigError::Message("Invalid host format".to_string()));
        }
        
        // Проверяем разумные ограничения для порта (u16 максимум 65535)
        if self.port < 1024 {
            return Err(config::ConfigError::Message("Port must be 1024 or higher for security reasons".to_string()));
        }
        
        // Проверяем, что шаблон URL базы данных содержит необходимый плейсхолдер
        if !self.client_database_url_template.contains("{db_name}") {
            return Err(config::ConfigError::Message("Database URL template must contain {db_name} placeholder".to_string()));
        }
        
        // Проверяем, что URL не содержит очевидно небезопасных элементов path traversal
        if self.client_database_url_template.contains("..") {
            return Err(config::ConfigError::Message("Database URL template contains potentially unsafe path traversal".to_string()));
        }
        
        Ok(())
    }
}