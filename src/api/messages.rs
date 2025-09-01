use actix_web::{get, post, web, HttpResponse};
use crate::{
    api::helpers,
    errors::AppError,
    services::wazzup_api::{self, SendMessageRequest},
    AppState,
};

// --- Route Handlers ---

#[utoipa::path(
    post,
    path = "/api/messages/{companyId}/send",
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
#[post("/{companyId}/send")]
async fn send_message(
    app_state: web::Data<AppState>,
    path: web::Path<i64>,
    body: web::Json<SendMessageRequest>,
) -> Result<HttpResponse, AppError> {
    let company_id = path.into_inner();
    let api_key = helpers::get_company_api_key(company_id, &app_state.db).await?;
    let wazzup_api = wazzup_api::WazzupApiService::new();

    let response = wazzup_api.send_message(&api_key, &body.into_inner()).await?;

    Ok(HttpResponse::Ok().json(response))
}

#[utoipa::path(
    get,
    path = "/api/messages/{companyId}/chat/{chatId}",
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
#[get("/{companyId}/chat/{chatId}")]
async fn get_messages(
    app_state: web::Data<AppState>,
    path: web::Path<(i64, String)>,
) -> Result<HttpResponse, AppError> {
    let (company_id, chat_id) = path.into_inner();
    let api_key = helpers::get_company_api_key(company_id, &app_state.db).await?;
    let wazzup_api = wazzup_api::WazzupApiService::new();

    let response = wazzup_api.get_messages(&api_key, &chat_id).await?;

    Ok(HttpResponse::Ok().json(response))
}

#[utoipa::path(
    get,
    path = "/api/messages/{companyId}/unread-count",
    tag = "Messages",
    params(
        ("companyId" = i64, Path, description = "Company ID")
    ),
    responses(
        (status = 200, description = "Total unread message count", body = wazzup_api::UnreadCountResponse),
        (status = 404, description = "Company not found")
    )
)]
#[get("/{companyId}/unread-count")]
async fn get_unread_count(
    app_state: web::Data<AppState>,
    path: web::Path<i64>,
) -> Result<HttpResponse, AppError> {
    let company_id = path.into_inner();
    let api_key = helpers::get_company_api_key(company_id, &app_state.db).await?;
    let wazzup_api = wazzup_api::WazzupApiService::new();

    let response = wazzup_api.get_unread_count(&api_key).await?;

    Ok(HttpResponse::Ok().json(response))
}


// Функция для регистрации всех маршрутов этого модуля
pub fn init_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/messages")
            .service(send_message)
            .service(get_messages)
            .service(get_unread_count)
    );
}