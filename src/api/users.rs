use actix_web::{get, patch, post, web, HttpResponse};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set,
};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{
    database::{
        client::models::{self as client_models},
    },
    errors::AppError,
    services::wazzup_api::{self, UpdateUserSettingsRequest},
    AppState,
};

// --- DTOs (Data Transfer Objects) ---

#[derive(Deserialize, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CreateUserDto {
    pub name: String,
    pub login: String,
    pub email: String,
    // В реальном приложении здесь должен быть пароль, а не хеш
    pub password_hash: String,
    pub salt: String,
    pub role: Option<String>,
}

// --- Helper Functions ---

/// Получает подключение к базе данных клиента по ID компании.
async fn get_client_db_conn(
    company_id: i64,
    app_state: &web::Data<AppState>,
) -> Result<DatabaseConnection, AppError> {
    crate::api::helpers::get_client_db_connection(company_id, app_state).await
}

/// Находит компанию и возвращает ее API ключ.
async fn get_company_api_key(
    company_id: i64,
    db: &DatabaseConnection,
) -> Result<String, AppError> {
    crate::api::helpers::get_company_api_key(company_id, db).await
}


// --- Route Handlers ---

#[utoipa::path(
    get,
    path = "/api/users/{companyId}",
    tag = "Users",
    params(
        ("companyId" = i64, Path, description = "Company ID")
    ),
    responses(
        (status = 200, description = "List of users for the company", body = [client_models::user::Model]),
        (status = 404, description = "Company not found")
    )
)]
#[get("/{companyId}")]
async fn get_users(
    app_state: web::Data<AppState>,
    path: web::Path<i64>,
) -> Result<HttpResponse, AppError> {
    let company_id = path.into_inner();
    let client_db = get_client_db_conn(company_id, &app_state).await?;
    let users = client_models::user::Entity::find().all(&client_db).await?;
    Ok(HttpResponse::Ok().json(users))
}


#[utoipa::path(
    post,
    path = "/api/users/{companyId}",
    tag = "Users",
    params(
        ("companyId" = i64, Path, description = "Company ID")
    ),
    request_body = CreateUserDto,
    responses(
        (status = 201, description = "User created successfully", body = client_models::user::Model),
        (status = 404, description = "Company not found"),
        (status = 400, description = "Invalid input, e.g., user already exists")
    )
)]
#[post("/{companyId}")]
async fn create_user(
    app_state: web::Data<AppState>,
    path: web::Path<i64>,
    body: web::Json<CreateUserDto>,
) -> Result<HttpResponse, AppError> {
    let company_id = path.into_inner();
    let client_db = get_client_db_conn(company_id, &app_state).await?;

    // Проверка на существующего пользователя (опционально, но рекомендуется)
    let existing_user = client_models::user::Entity::find()
        .filter(client_models::user::Column::Email.eq(body.email.clone()))
        .one(&client_db)
        .await?;

    if existing_user.is_some() {
        return Err(AppError::InvalidInput(format!("User with email {} already exists", body.email)));
    }

    let new_user = client_models::user::ActiveModel {
        name: Set(body.name.clone()),
        login: Set(body.login.clone()),
        email: Set(body.email.clone()),
        password_hash: Set(body.password_hash.clone()), // В проде здесь должно быть хеширование
        salt: Set(body.salt.clone()),
        role: Set(body.role.clone().unwrap_or_else(|| "manager".to_string())),
        created_at: Set(chrono::Utc::now()),
        ..Default::default()
    };

    let created_user = new_user.insert(&client_db).await?;
    Ok(HttpResponse::Created().json(created_user))
}


#[utoipa::path(
    get,
    path = "/api/users/{companyId}/settings",
    tag = "Users",
    params(
        ("companyId" = i64, Path, description = "Company ID")
    ),
    responses(
        (status = 200, description = "Wazzup user settings", body = wazzup_api::UserSettings),
        (status = 404, description = "Company not found or API key not set")
    )
)]
#[get("/{companyId}/settings")]
async fn get_settings(
    app_state: web::Data<AppState>,
    path: web::Path<i64>,
) -> Result<HttpResponse, AppError> {
    let company_id = path.into_inner();
    let api_key = get_company_api_key(company_id, &app_state.db).await?;
    let wazzup_api = wazzup_api::WazzupApiService::new();

    let settings = wazzup_api.get_user_settings(&api_key).await?;
    Ok(HttpResponse::Ok().json(settings))
}


#[utoipa::path(
    patch,
    path = "/api/users/{companyId}/settings",
    tag = "Users",
    params(
        ("companyId" = i64, Path, description = "Company ID")
    ),
    request_body = wazzup_api::UpdateUserSettingsRequest,
    responses(
        (status = 200, description = "Settings updated successfully"),
        (status = 404, description = "Company not found or API key not set")
    )
)]
#[patch("/{companyId}/settings")]
async fn update_settings(
    app_state: web::Data<AppState>,
    path: web::Path<i64>,
    body: web::Json<UpdateUserSettingsRequest>,
) -> Result<HttpResponse, AppError> {
    let company_id = path.into_inner();
    let api_key = get_company_api_key(company_id, &app_state.db).await?;
    let wazzup_api = wazzup_api::WazzupApiService::new();

    wazzup_api.update_user_settings(&api_key, &body.into_inner()).await?;
    Ok(HttpResponse::Ok().finish())
}


// Функция для регистрации всех маршрутов этого модуля
pub fn init_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/users")
            .service(get_users)
            .service(create_user)
            .service(get_settings)
            .service(update_settings)
    );
}