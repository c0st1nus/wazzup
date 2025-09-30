use actix_web::{HttpRequest, HttpResponse, get, post, web};
use sea_orm::EntityTrait;

use crate::{
    app_state::AppState,
    database::models::companies,
    errors::AppError,
    services::{
        wazzup_api::WebhookSubscriptionRequest,
        webhook_handler,
    },
};

use super::functions::{
    build_webhook_uri, default_webhook_subscriptions, get_company_api_key_by_uuid,
    parse_company_id, uuid_to_bytes,
};
use super::structures::{
    ConnectWebhooksResponse, TestWebhookResponse, WebhookStatusResponse,
    WebhookValidationResponse,
};

/// Валидация webhook endpoint
#[utoipa::path(
    get,
    path = "/api/webhook/{id}",
    tag = "Webhooks",
    params(("id" = String, Path, description = "Company UUID")),
    responses(
        (status = 200, description = "Webhook validation successful", body = WebhookValidationResponse),
        (status = 404, description = "Company not found")
    )
)]
#[get("/{id}")]
pub async fn validate_webhook(
    app_state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let company_uuid = parse_company_id(&path.into_inner())?;
    let company_bytes = uuid_to_bytes(&company_uuid);

    companies::Entity::find_by_id(company_bytes)
        .one(&app_state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Company not found".to_string()))?;

    let response = WebhookValidationResponse {
        status: "ok".to_string(),
        message: "Webhook endpoint is valid".to_string(),
    };

    Ok(HttpResponse::Ok().json(response))
}

/// Обработка входящих вебхуков
#[utoipa::path(
    post,
    path = "/api/webhook/{id}",
    tag = "Webhooks",
    params(("id" = String, Path, description = "Company UUID")),
    request_body = webhook_handler::WebhookRequest,
    responses(
        (status = 200, description = "Webhook processed successfully", body = WebhookStatusResponse),
        (status = 400, description = "Failed to process webhook"),
        (status = 404, description = "Company not found"),
        (status = 500, description = "Internal Server Error")
    )
)]
#[post("/{id}")]
pub async fn handle_webhook(
    app_state: web::Data<AppState>,
    path: web::Path<String>,
    body: web::Json<webhook_handler::WebhookRequest>,
) -> Result<HttpResponse, AppError> {
    let company_uuid = parse_company_id(&path.into_inner())?;

    let payload = body.into_inner();
    let json_string = serde_json::to_string(&payload)?;
    if json_string.len() > 1024 * 1024 {
        return Err(AppError::InvalidInput(
            "Webhook payload too large".to_string(),
        ));
    }

    webhook_handler::handle_webhook(
        company_uuid,
        payload,
        &app_state.db,
        &app_state.bot_service,
        &app_state.wazzup_api,
    )
    .await?;

    let response = WebhookStatusResponse {
        status: "ok".to_string(),
    };

    Ok(HttpResponse::Ok().json(response))
}

/// Подключение вебхуков для компании
#[utoipa::path(
    get,
    path = "/api/webhook/{id}/connect",
    tag = "Webhooks",
    params(("id" = String, Path, description = "Company UUID")),
    responses(
        (status = 200, description = "Webhooks connected successfully", body = ConnectWebhooksResponse),
        (status = 400, description = "Failed to connect webhooks"),
        (status = 404, description = "Company not found or API key not set")
    )
)]
#[get("/{id}/connect")]
pub async fn connect_webhooks(
    app_state: web::Data<AppState>,
    req: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let raw_id = path.into_inner();
    log::debug!("Connecting webhooks for company ID: {}", raw_id);
    
    let company_uuid = parse_company_id(&raw_id)?;
    log::debug!("Parsed UUID: {}", company_uuid);
    log::debug!("UUID bytes: {:?}", uuid_to_bytes(&company_uuid));
    
    let api_key = get_company_api_key_by_uuid(&company_uuid, &app_state.db).await?;

    let webhooks_uri = build_webhook_uri(&app_state, &req, &company_uuid);
    let subscriptions = default_webhook_subscriptions();

    let request = WebhookSubscriptionRequest {
        webhooks_uri: webhooks_uri.clone(),
        subscriptions: subscriptions.clone(),
    };

    app_state
        .wazzup_api
        .connect_webhooks(&api_key, &request)
        .await?;

    let response = ConnectWebhooksResponse {
        ok: true,
        webhooks_uri,
        subscriptions,
    };

    Ok(HttpResponse::Ok().json(response))
}

/// Тестирование webhook endpoint
#[utoipa::path(
    post,
    path = "/api/webhook/{id}/test",
    tag = "Webhooks",
    params(("id" = String, Path, description = "Company UUID")),
    responses(
        (status = 200, description = "Test webhook processed successfully", body = TestWebhookResponse),
        (status = 400, description = "Test webhook failed")
    )
)]
#[post("/{id}/test")]
pub async fn test_webhook(
    app_state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let company_uuid = parse_company_id(&path.into_inner())?;

    let test_webhook_request = webhook_handler::WebhookRequest {
        test: Some(true),
        messages: None,
        contacts: None,
    };

    webhook_handler::handle_webhook(
        company_uuid,
        test_webhook_request,
        &app_state.db,
        &app_state.bot_service,
        &app_state.wazzup_api,
    )
    .await?;

    let response = TestWebhookResponse {
        status: "ok".to_string(),
        message: "Test webhook processed successfully".to_string(),
    };

    Ok(HttpResponse::Ok().json(response))
}

/// Регистрация всех маршрутов вебхуков
pub fn init_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/webhook")
            .service(connect_webhooks)
            .service(test_webhook)
            .service(validate_webhook)
            .service(handle_webhook),
    );
}
