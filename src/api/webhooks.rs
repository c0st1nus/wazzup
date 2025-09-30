use actix_web::{HttpRequest, HttpResponse, get, post, web};
use sea_orm::EntityTrait;
use serde::Serialize;
use url::Url;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    api::helpers::{get_company_api_key, uuid_to_bytes},
    app_state::AppState,
    database::models::companies,
    errors::AppError,
    services::{
        wazzup_api::{WebhookSubscriptionRequest, WebhookSubscriptions},
        webhook_handler,
    },
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

fn parse_company_id(raw: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(raw).map_err(|_| AppError::InvalidInput("Invalid company ID".to_string()))
}

#[utoipa::path(
    get,
    path = "/api/webhook/{id}",
    tag = "Webhooks",
    params(("id" = String, Path, description = "Company UUID")),
    responses(
        (status = 200, description = "Webhook validation successful", body = inline(serde_json::Value)),
        (status = 404, description = "Company not found")
    )
)]
#[get("/{id}")]
async fn validate_webhook(
    app_state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let company_uuid = parse_company_id(&path.into_inner())?;
    let company_bytes = uuid_to_bytes(&company_uuid);

    companies::Entity::find_by_id(company_bytes)
        .one(&app_state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Company not found".to_string()))?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "ok",
        "message": "Webhook endpoint is valid"
    })))
}

#[utoipa::path(
    post,
    path = "/api/webhook/{id}",
    tag = "Webhooks",
    params(("id" = String, Path, description = "Company UUID")),
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

    Ok(HttpResponse::Ok().json(serde_json::json!({ "status": "ok" })))
}

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
async fn connect_webhooks(
    app_state: web::Data<AppState>,
    req: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let company_uuid = parse_company_id(&path.into_inner())?;
    let api_key = get_company_api_key(&company_uuid, &app_state.db).await?;

    let webhooks_uri = build_webhook_uri(&app_state, &req, &company_uuid);

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

    app_state
        .wazzup_api
        .connect_webhooks(&api_key, &request)
        .await?;

    Ok(HttpResponse::Ok().json(ConnectWebhooksResponse {
        ok: true,
        webhooks_uri,
        subscriptions,
    }))
}

fn build_webhook_uri(app_state: &AppState, req: &HttpRequest, company_uuid: &Uuid) -> String {
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

#[utoipa::path(
    post,
    path = "/api/webhook/{id}/test",
    tag = "Webhooks",
    params(("id" = String, Path, description = "Company UUID")),
    responses(
        (status = 200, description = "Test webhook processed successfully"),
        (status = 400, description = "Test webhook failed")
    )
)]
#[post("/{id}/test")]
async fn test_webhook(
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
            .service(handle_webhook),
    );
}
