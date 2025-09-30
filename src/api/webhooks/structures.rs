use serde::Serialize;
use utoipa::ToSchema;

use crate::services::wazzup_api::WebhookSubscriptions;

#[derive(Serialize, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConnectWebhooksResponse {
    pub ok: bool,
    pub webhooks_uri: String,
    pub subscriptions: WebhookSubscriptions,
}

#[derive(Serialize, ToSchema, Clone)]
pub struct WebhookValidationResponse {
    pub status: String,
    pub message: String,
}

#[derive(Serialize, ToSchema, Clone)]
pub struct WebhookStatusResponse {
    pub status: String,
}

#[derive(Serialize, ToSchema, Clone)]
pub struct TestWebhookResponse {
    pub status: String,
    pub message: String,
}
