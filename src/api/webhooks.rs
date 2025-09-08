use actix_web::{get, post, web, HttpRequest, HttpResponse};
use sea_orm::EntityTrait;
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    database::main,
    errors::AppError,
    services::{
    wazzup_api::{WebhookSubscriptionRequest, WebhookSubscriptions},
        webhook_handler,
    },
    app_state::AppState,
};

// --- DTOs (Data Transfer Objects) ---

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConnectWebhooksResponse {
    ok: bool,
    webhooks_uri: String,
    subscriptions: WebhookSubscriptions,
}

// --- Route Handlers ---

#[utoipa::path(
    get,
    path = "/api/webhook/{id}",
    tag = "Webhooks",
    params(
        ("id" = i64, Path, description = "Company ID")
    ),
    responses(
        (status = 200, description = "Webhook validation successful", body = inline(serde_json::Value)),
        (status = 404, description = "Company not found")
    )
)]
#[get("/{id}")]
async fn validate_webhook(
    app_state: web::Data<AppState>,
    path: web::Path<i64>,
) -> Result<HttpResponse, AppError> {
    let company_id = path.into_inner();
    
    // Базовая валидация company_id
    if company_id <= 0 {
        log::error!("Invalid company_id for validation: {}", company_id);
        return Err(AppError::InvalidInput("Invalid company ID".to_string()));
    }
    
    log::info!("Webhook validation request for company {}", company_id);
    
    // Проверяем, что компания существует
    let _company = main::companies::Entity::find_by_id(company_id)
        .one(&app_state.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Company {} not found", company_id)))?;
    
    // Возвращаем статус 200 для валидации webhook'а
    Ok(HttpResponse::Ok().json(serde_json::json!({ 
        "status": "ok",
        "message": "Webhook endpoint is valid"
    })))
}

#[utoipa::path(
    post,
    path = "/api/webhook/{id}",
    tag = "Webhooks",
    params(
        ("id" = i64, Path, description = "Company ID")
    ),
    request_body = webhook_handler::WebhookRequest,
    responses(
        (status = 200, description = "Webhook processed successfully", body = inline(serde_json::Value)),
        (status = 400, description = "Failed to process webhook"),
        (status = 404, description = "Company not found"),
        (status = 500, description = "Internal Server Error")
    )
)]
#[post("/{id}")]
async fn handle_webhook(
    app_state: web::Data<AppState>,
    path: web::Path<i64>,
    body: web::Json<webhook_handler::WebhookRequest>,
) -> Result<HttpResponse, AppError> {
    let company_id = path.into_inner();
    
    // Базовая валидация company_id
    if company_id <= 0 {
        log::error!("Invalid company_id: {}", company_id);
        return Err(AppError::InvalidInput("Invalid company ID".to_string()));
    }
    
    // Ограничиваем размер payload
    let json_string = serde_json::to_string(&body)?;
    if json_string.len() > 1024 * 1024 {  // 1MB limit
        log::error!("Webhook payload too large: {} bytes", json_string.len());
        return Err(AppError::InvalidInput("Webhook payload too large".to_string()));
    }
    
    webhook_handler::handle_webhook(
        company_id, 
        body.into_inner(), 
        &app_state.db, 
        &app_state.config,
        &app_state.bot_service,
        &app_state.wazzup_api,
    ).await?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "status": "ok" })))
}

#[utoipa::path(
    get,
    path = "/api/webhook/{id}/connect",
    tag = "Webhooks",
    params(
        ("id" = i64, Path, description = "Company ID")
    ),
    responses(
        (status = 200, description = "Webhooks connected successfully", body = ConnectWebhooksResponse),
        (status = 400, description = "Failed to connect webhooks, e.g., test POST failed"),
        (status = 404, description = "Company not found or API key not set")
    )
)]
#[get("/{id}/connect")]
async fn connect_webhooks(
    app_state: web::Data<AppState>,
    req: HttpRequest,
    path: web::Path<i64>,
) -> Result<HttpResponse, AppError> {
    let company_id = path.into_inner();
    log::info!("Connecting webhooks for company {}", company_id);

    let company = main::companies::Entity::find_by_id(company_id)
        .one(&app_state.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Company {} not found", company_id)))?;
    
    if company.wazzup_api_key.is_empty() {
        return Err(AppError::InvalidInput("Company API key not set".to_string()));
    }

    let webhooks_uri = if let Some(public_url) = &app_state.config.public_url {
        format!("{}/api/webhook/{}", public_url, company_id)
    } else {
        let host = req.connection_info().host().to_string();
        let scheme = req.connection_info().scheme().to_string();
        format!("{}://{}/api/webhook/{}", scheme, host, company_id)
    };
    
    log::info!("Generated webhooks_uri: {}", webhooks_uri);

    let subscriptions = WebhookSubscriptions {
        messages_and_statuses: true,
        contacts_and_deals_creation: true,
        channels_updates: true,
        template_status: true,
    };

    let request = WebhookSubscriptionRequest {
        webhooks_uri: webhooks_uri.clone(),
        subscriptions: subscriptions.clone(),
    };
    
    log::info!("Sending webhook request: {:?}", request);

    let response = app_state.wazzup_api.connect_webhooks(&company.wazzup_api_key, &request).await?;

    log::info!("Successfully connected webhooks for company {}, response: {}", company_id, response);
    Ok(HttpResponse::Ok().json(ConnectWebhooksResponse {
        ok: true,
        webhooks_uri,
        subscriptions: request.subscriptions,
    }))
}


#[utoipa::path(
    post,
    path = "/api/webhook/{id}/test",
    tag = "Webhooks",
    params(
        ("id" = i64, Path, description = "Company ID")
    ),
    responses(
        (status = 200, description = "Test webhook processed successfully"),
        (status = 400, description = "Test webhook failed")
    )
)]
#[post("/{id}/test")]
async fn test_webhook(
    app_state: web::Data<AppState>,
    path: web::Path<i64>,
) -> Result<HttpResponse, AppError> {
    let company_id = path.into_inner();
    log::info!("Test webhook endpoint called for company {}", company_id);

    let test_webhook_request = webhook_handler::WebhookRequest {
        test: Some(true),
        messages: None,
        contacts: None,
    };

    webhook_handler::handle_webhook(
        company_id, 
        test_webhook_request, 
        &app_state.db, 
        &app_state.config,
        &app_state.bot_service,
        &app_state.wazzup_api,
    ).await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "ok",
        "message": "Test webhook processed successfully"
    })))
}


// Функция для регистрации всех маршрутов этого модуля
pub fn init_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/webhook")
            .service(connect_webhooks)
            .service(test_webhook)
            .service(validate_webhook)
            .service(handle_webhook)
    );
}
