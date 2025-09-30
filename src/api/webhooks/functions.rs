use actix_web::HttpRequest;
use sea_orm::{DatabaseConnection, DbErr};
use url::Url;
use uuid::Uuid;

use crate::{
    api::helpers::get_company_api_key,
    app_state::AppState,
    errors::AppError,
    services::wazzup_api::WebhookSubscriptions,
};

/// Конвертирует UUID в бинарный формат для хранения в базе данных
pub fn uuid_to_bytes(uuid: &Uuid) -> Vec<u8> {
    uuid.as_bytes().to_vec()
}

/// Конвертирует бинарные данные обратно в UUID
pub fn uuid_from_bytes(bytes: &[u8]) -> Result<Uuid, DbErr> {
    match bytes.len() {
        16 => Uuid::from_slice(bytes).map_err(|e| DbErr::Custom(format!("Invalid UUID data: {e}"))),
        8 => {
            let mut padded = [0u8; 16];
            padded[8..].copy_from_slice(bytes);
            Uuid::from_slice(&padded).map_err(|e| DbErr::Custom(format!("Invalid UUID data: {e}")))
        }
        0 => Ok(Uuid::nil()),
        other => Err(DbErr::Custom(format!(
            "Invalid UUID length: expected 16 or 8 bytes, found {}",
            other
        ))),
    }
}

/// Получает API ключ компании из базы данных по UUID
pub async fn get_company_api_key_by_uuid(
    company_uuid: &Uuid,
    db: &DatabaseConnection,
) -> Result<String, AppError> {
    match get_company_api_key(company_uuid, db).await {
        Ok(key) => Ok(key),
        Err(AppError::NotFound(_)) => Err(AppError::Unauthorized("Company not found".to_string())),
        Err(err) => Err(err),
    }
}

/// Парсит строку company ID в UUID
pub fn parse_company_id(raw: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(raw).map_err(|_| AppError::InvalidInput("Invalid company ID".to_string()))
}

/// Строит URI для webhook endpoint
pub fn build_webhook_uri(app_state: &AppState, req: &HttpRequest, company_uuid: &Uuid) -> String {
    let webhook_port = app_state.config.effective_webhook_port();

    if let Some(public_url) = &app_state.config.public_url {
        if let Ok(mut url) = Url::parse(public_url) {
            let _ = url.set_port(Some(webhook_port));
            url.set_path(&format!("/api/webhook/{}", company_uuid));
            return url.to_string();
        }
    }

    let conn_info = req.connection_info().clone();
    let scheme = conn_info.scheme().to_owned();
    let host = conn_info.host().to_owned();
    let base = format!("{}://{}", scheme, host);

    match Url::parse(&base) {
        Ok(mut url) => {
            let _ = url.set_port(Some(webhook_port));
            url.set_path(&format!("/api/webhook/{}", company_uuid));
            url.to_string()
        }
        Err(_) => format!(
            "http://localhost:{}/api/webhook/{}",
            webhook_port, company_uuid
        ),
    }
}

/// Создает дефолтные настройки подписок на вебхуки
pub fn default_webhook_subscriptions() -> WebhookSubscriptions {
    WebhookSubscriptions {
        messages_and_statuses: true,
        contacts_and_deals_creation: true,
        channels_updates: true,
        template_status: true,
    }
}
