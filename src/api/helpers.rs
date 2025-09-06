use sea_orm::{DatabaseConnection, EntityTrait};
use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use crate::{
    database::main::models as main_models,
    errors::AppError,
    AppState,
};
use actix_web::web;

/// Утилиты для работы с временными зонами
struct TimezoneUtils;

impl TimezoneUtils {
    /// Конвертирует UTC время в указанную временную зону
    fn utc_to_timezone(utc_time: DateTime<Utc>, timezone: Tz) -> DateTime<Tz> {
        utc_time.with_timezone(&timezone)
    }

    /// Форматирует время с учетом временной зоны для API
    fn format_time_with_timezone(utc_time: DateTime<Utc>, timezone: Tz) -> String {
        let tz_time = Self::utc_to_timezone(utc_time, timezone);
        tz_time.format("%Y-%m-%d %H:%M:%S %Z").to_string()
    }
}

/// Находит компанию и возвращает ее API ключ.
pub async fn get_company_api_key(
    company_id: i64,
    db: &DatabaseConnection,
) -> Result<String, AppError> {
    let company = main_models::Entity::find_by_id(company_id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Company with id {} not found", company_id)))?;

    if company.wazzup_api_key.is_empty() {
        Err(AppError::InvalidInput(format!(
            "API key for company {} is not set",
            company_id
        )))
    } else {
        Ok(company.wazzup_api_key)
    }
}

/// Получает подключение к базе данных клиента по ID компании.
#[allow(dead_code)]
pub async fn get_client_db_connection(
    company_id: i64,
    app_state: &web::Data<AppState>,
) -> Result<DatabaseConnection, AppError> {
    let company = main_models::Entity::find_by_id(company_id)
        .one(&app_state.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Company {} not found", company_id)))?;
    
    let client_db_url = app_state
        .config
        .client_database_url_template
        .replace("{db_name}", &company.database_name);
        
    Ok(sea_orm::Database::connect(&client_db_url).await?)
}

/// Конвертирует UTC время в настроенную временную зону сервера
pub fn convert_to_server_timezone(
    utc_time: DateTime<Utc>,
    app_state: &web::Data<AppState>,
) -> Result<String, AppError> {
    let server_tz = app_state.config.get_timezone()
        .map_err(|e| AppError::InvalidInput(format!("Invalid server timezone: {}", e)))?;
    
    Ok(TimezoneUtils::format_time_with_timezone(utc_time, server_tz))
}
