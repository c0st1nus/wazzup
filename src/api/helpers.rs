use sea_orm::{DatabaseConnection, EntityTrait};
use crate::{
    database::main,
    errors::AppError,
    app_state::AppState,
};
use actix_web::web;

/// Находит компанию и возвращает ее API ключ.
pub async fn get_company_api_key(
    company_id: i64,
    db: &DatabaseConnection,
) -> Result<String, AppError> {
    let company = main::companies::Entity::find_by_id(company_id)
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
    let company = main::companies::Entity::find_by_id(company_id)
        .one(&app_state.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Company {} not found", company_id)))?;
    // Используем пул вместо прямого подключения каждый раз
    let conn = app_state.client_db_pool.get_connection(&company.database_name).await?;
    Ok(conn)
}
