use crate::database::client::users;
use crate::errors::AppError;
use reqwest::Client;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct BotHookRequest {
    pub message: String,
    pub client: i64,
    pub company: i64,
}

#[derive(Debug, Deserialize)]
pub struct BotHookResponse {
    pub status: String,
    pub message: String,
}

#[derive(Clone)]
pub struct BotService {
    client: Client,
}

impl BotService {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    /// Отправляет POST запрос на hook URL бота
    pub async fn send_hook_request(
        &self,
        hook_url: &str,
        request: &BotHookRequest,
    ) -> Result<BotHookResponse, AppError> {
        let response = self
            .client
            .post(hook_url)
            .json(request)
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await
            .map_err(|e| AppError::ExternalApiError(format!("Bot hook request failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(AppError::ExternalApiError(format!(
                "Bot hook returned error status: {}",
                response.status()
            )));
        }

        let bot_response: BotHookResponse = response.json().await.map_err(|e| {
            AppError::ExternalApiError(format!("Failed to parse bot response: {}", e))
        })?;

        Ok(bot_response)
    }

    /// Выбирает случайного менеджера для перенаправления клиента
    pub async fn select_random_manager(
        &self,
        db: &DatabaseConnection,
    ) -> Result<users::Model, AppError> {
        use sea_orm::QueryOrder;

        // Получаем всех активных менеджеров (исключая ботов и quality_control)
        let managers = users::Entity::find()
            .filter(users::Column::Role.eq("manager"))
            .order_by_asc(users::Column::Id) // Для детерминированности
            .all(db)
            .await?;

        if managers.is_empty() {
            return Err(AppError::NotFound("No managers available".to_string()));
        }

        // Выбираем случайного менеджера
        // TODO: Здесь можно реализовать более сложную логику балансировки нагрузки
        let index = fastrand::usize(..managers.len());
        Ok(managers[index].clone())
    }

    /// Проверяет, является ли пользователь ботом и имеет ли он hook URL
    pub async fn get_bot_hook_url(
        &self,
        db: &DatabaseConnection,
        user_id: i64,
    ) -> Result<Option<String>, AppError> {
        let user = users::Entity::find_by_id(user_id)
            .one(db)
            .await?
            .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

        if user.role == "bot" {
            Ok(user.hook)
        } else {
            Ok(None)
        }
    }
}
