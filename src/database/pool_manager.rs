use sea_orm::{Database, DatabaseConnection, DbErr};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::config::Config;
use crate::errors::AppError;

/// Менеджер пулов подключений для клиентских баз данных
#[derive(Clone)]
pub struct ClientDbPoolManager {
    pools: Arc<RwLock<HashMap<String, DatabaseConnection>>>,
    config: Config,
}

impl ClientDbPoolManager {
    pub fn new(config: Config) -> Self {
        Self {
            pools: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Получает подключение к клиентской базе данных
    /// Если подключение не существует, создает новое и кэширует его
    pub async fn get_connection(&self, database_name: &str) -> Result<DatabaseConnection, AppError> {
        // Валидация имени базы данных
        if !database_name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Err(AppError::InvalidInput("Invalid database name format".to_string()));
        }

        // Сначала пытаемся получить существующее подключение
        {
            let pools = self.pools.read().await;
            if let Some(connection) = pools.get(database_name) {
                // Проверяем, что подключение еще активно
                if self.is_connection_alive(connection).await {
                    log::debug!("Reusing existing connection for database: {}", database_name);
                    return Ok(connection.clone());
                } else {
                    log::warn!("Connection to database {} is dead, will create new one", database_name);
                }
            }
        }

        // Если подключения нет или оно мертвое, создаем новое
        log::info!("Creating new connection pool for database: {}", database_name);
        let db_url = self.config.client_database_url_template.replace("{db_name}", database_name);
        
        let connection = Database::connect(&db_url).await.map_err(|e| {
            log::error!("Failed to connect to database {}: {}", database_name, e);
            AppError::DbError(e)
        })?;

        // Кэшируем новое подключение
        {
            let mut pools = self.pools.write().await;
            pools.insert(database_name.to_string(), connection.clone());
        }

        log::info!("Successfully created and cached connection for database: {}", database_name);
        Ok(connection)
    }

    /// Проверяет, что подключение еще активно
    async fn is_connection_alive(&self, connection: &DatabaseConnection) -> bool {
        // Простая проверка - выполняем простой запрос
        match connection.ping().await {
            Ok(_) => true,
            Err(e) => {
                log::debug!("Connection ping failed: {}", e);
                false
            }
        }
    }

    /// Закрывает все подключения (полезно при shutdown)
    #[allow(dead_code)]
    pub async fn close_all(&self) -> Result<(), DbErr> {
        let mut pools = self.pools.write().await;
        
        for (db_name, connection) in pools.drain() {
            log::info!("Closing connection to database: {}", db_name);
            if let Err(e) = connection.close().await {
                log::error!("Error closing connection to {}: {}", db_name, e);
            }
        }
        
        Ok(())
    }

    /// Удаляет конкретное подключение из кэша (если оно мертвое)
    #[allow(dead_code)]
    pub async fn remove_connection(&self, database_name: &str) {
        let mut pools = self.pools.write().await;
        if pools.remove(database_name).is_some() {
            log::info!("Removed cached connection for database: {}", database_name);
        }
    }

    /// Возвращает количество активных подключений
    #[allow(dead_code)]
    pub async fn active_connections_count(&self) -> usize {
        let pools = self.pools.read().await;
        pools.len()
    }

    /// Возвращает список активных подключений для мониторинга
    #[allow(dead_code)]
    pub async fn get_active_databases(&self) -> Vec<String> {
        let pools = self.pools.read().await;
        pools.keys().cloned().collect()
    }
}