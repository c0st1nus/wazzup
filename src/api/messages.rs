use actix_web::{get, post, web, HttpResponse};
use sea_orm::{DatabaseConnection, EntityTrait};
use crate::{
    database::main::models as main_models,
    errors::AppError,
    services::wazzup_api::{self, SendMessageRequest},
    AppState,
};

// --- Helper Functions ---

/// Находит компанию и возвращает ее API ключ. (Эта функция-хелпер может быть вынесена в общий модуль)
async fn get_company_api_key(
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

// --- Route Handlers ---

#[utoipa::path(
    post,
    path = "/api/company/{companyId}/messages/send",
    tag = "Messages",
    params(
        ("companyId" = i64, Path, description = "Company ID")
    ),
    request_body = wazzup_api::SendMessageRequest,
    responses(
        (status = 200, description = "Message sent successfully", body = wazzup_api::SendMessageResponse),
        (status = 404, description = "Company not found")
    )
)]
#[post("/{companyId}/messages/send")]
async fn send_message(
    app_state: web::Data<AppState>,
    path: web::Path<i64>,
    body: web::Json<SendMessageRequest>,
) -> Result<HttpResponse, AppError> {
    let company_id = path.into_inner();
    let api_key = get_company_api_key(company_id, &app_state.db).await?;
    let wazzup_api = wazzup_api::WazzupApiService::new();

    let response = wazzup_api.send_message(&api_key, &body.into_inner()).await?;

    Ok(HttpResponse::Ok().json(response))
}

#[utoipa::path(
    get,
    path = "/api/company/{companyId}/messages/chat/{chatId}",
    tag = "Messages",
    params(
        ("companyId" = i64, Path, description = "Company ID"),
        ("chatId" = String, Path, description = "Chat ID")
    ),
    responses(
        (status = 200, description = "List of messages in the chat", body = wazzup_api::MessageListResponse),
        (status = 404, description = "Company not found")
    )
)]
#[get("/{companyId}/messages/chat/{chatId}")]
async fn get_messages(
    app_state: web::Data<AppState>,
    path: web::Path<(i64, String)>,
) -> Result<HttpResponse, AppError> {
    let (company_id, chat_id) = path.into_inner();
    let api_key = get_company_api_key(company_id, &app_state.db).await?;
    let wazzup_api = wazzup_api::WazzupApiService::new();

    let response = wazzup_api.get_messages(&api_key, &chat_id).await?;

    Ok(HttpResponse::Ok().json(response))
}

#[utoipa::path(
    get,
    path = "/api/company/{companyId}/messages/unread-count",
    tag = "Messages",
    params(
        ("companyId" = i64, Path, description = "Company ID")
    ),
    responses(
        (status = 200, description = "Total unread message count", body = wazzup_api::UnreadCountResponse),
        (status = 404, description = "Company not found")
    )
)]
#[get("/{companyId}/messages/unread-count")]
async fn get_unread_count(
    app_state: web::Data<AppState>,
    path: web::Path<i64>,
) -> Result<HttpResponse, AppError> {
    let company_id = path.into_inner();
    let api_key = get_company_api_key(company_id, &app_state.db).await?;
    let wazzup_api = wazzup_api::WazzupApiService::new();

    let response = wazzup_api.get_unread_count(&api_key).await?;

    Ok(HttpResponse::Ok().json(response))
}


// Функция для регистрации всех маршрутов этого модуля
pub fn init_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/company")
            .service(send_message)
            .service(get_messages)
            .service(get_unread_count)
    );
}