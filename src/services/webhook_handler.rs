use chrono::{DateTime, Utc};
use sea_orm::{ActiveModelTrait, DatabaseConnection, EntityTrait, IntoActiveModel, Set};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    api::helpers::uuid_to_bytes,
    api::validation,
    database::models::{channels, chats, clients, companies, messages},
    errors::AppError,
    services::bot_service::BotService,
    services::wazzup_api::WazzupApiService,
};

#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WebhookContact {
    pub name: Option<String>,
    pub avatar_uri: Option<String>,
    pub username: Option<String>,
    pub phone: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WebhookMessage {
    pub message_id: String,
    pub channel_id: String,
    pub chat_type: String,
    pub chat_id: String,
    pub r#type: String,
    pub text: Option<String>,
    pub content_uri: Option<String>,
    pub client_name: Option<String>,
    pub client_phone: Option<String>,
    pub date_time: Option<String>,
    pub is_echo: Option<bool>,
    pub status: Option<String>,
    pub contact: Option<WebhookContact>,
    pub author_name: Option<String>,
    pub author_id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WebhookContactEvent {
    pub contact_id: String,
    pub name: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub chat_id: Option<String>,
    pub channel_id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WebhookRequest {
    pub test: Option<bool>,
    pub messages: Option<Vec<WebhookMessage>>,
    pub contacts: Option<Vec<WebhookContactEvent>>,
}

pub fn determine_message_direction(msg: &WebhookMessage) -> (bool, String) {
    match msg.is_echo {
        Some(false) => (true, "incoming".to_string()),
        Some(true) => (false, "outgoing".to_string()),
        None => match msg.status.as_deref() {
            Some("inbound") => (true, "incoming".to_string()),
            Some(status) => (false, status.to_string()),
            None => (false, "unknown".to_string()),
        },
    }
}

fn parse_uuid(value: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(value).map_err(|_| AppError::InvalidInput("Invalid UUID value".to_string()))
}

/// Парсит ID, который может быть UUID или другим форматом (например, числом)
/// Если это не UUID, создает детерминированный UUID v5 на основе строки.
///
/// Wazzup API может отправлять ID в разных форматах:
/// - UUID для каналов и некоторых чатов
/// - Числовые ID для чатов (например, WhatsApp chat ID)
///
/// UUID v5 гарантирует, что один и тот же input всегда даст один и тот же UUID,
/// что важно для идемпотентности обработки webhook'ов.
fn parse_flexible_uuid(value: &str) -> Uuid {
    // Пытаемся парсить как UUID
    if let Ok(uuid) = Uuid::parse_str(value) {
        return uuid;
    }

    // Если не UUID, создаем детерминированный UUID v5 из строки
    // Используем namespace DNS для согласованности
    Uuid::new_v5(&Uuid::NAMESPACE_DNS, value.as_bytes())
}

fn parse_uuid_bytes(value: &str) -> Result<Vec<u8>, AppError> {
    Ok(uuid_to_bytes(&parse_uuid(value)?))
}

fn parse_optional_uuid_bytes(value: Option<&String>) -> Option<Vec<u8>> {
    value
        .and_then(|val| Uuid::parse_str(val).ok())
        .map(|uuid| uuid_to_bytes(&uuid))
}

fn parse_date_time(value: Option<&String>) -> DateTime<Utc> {
    value
        .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(Utc::now)
}

fn build_message_content(message: &WebhookMessage) -> Value {
    let mut parts = Vec::new();

    if let Some(text) = &message.text {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            parts.push(json!({
                "type": "text",
                "content": trimmed,
            }));
        }
    }

    if let Some(uri) = &message.content_uri {
        let trimmed = uri.trim();
        if !trimmed.is_empty() {
            parts.push(json!({
                "type": "attachment",
                "content": trimmed,
            }));
        }
    }

    Value::Array(parts)
}

async fn ensure_channel(
    db: &DatabaseConnection,
    channel_bytes: Vec<u8>,
    chat_type: &str,
) -> Result<(), AppError> {
    if let Some(existing) = channels::Entity::find_by_id(channel_bytes.clone())
        .one(db)
        .await?
    {
        if existing.r#type != chat_type {
            let mut active = existing.into_active_model();
            active.r#type = Set(chat_type.to_string());
            active.update(db).await?;
        }
    } else {
        let record = channels::ActiveModel {
            id: Set(channel_bytes),
            r#type: Set(chat_type.to_string()),
        };
        record.insert(db).await?;
    }

    Ok(())
}

async fn ensure_chat(
    db: &DatabaseConnection,
    chat_uuid: &Uuid,
    channel_bytes: Vec<u8>,
    name_hint: Option<&str>,
) -> Result<(), AppError> {
    let chat_id_str = chat_uuid.to_string();

    if let Some(existing) = chats::Entity::find_by_id(chat_id_str.clone())
        .one(db)
        .await?
    {
        if let Some(name) = name_hint.and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        }) {
            if existing.name != name {
                let mut active = existing.into_active_model();
                active.name = Set(name.to_string());
                active.update(db).await?;
            }
        }
    } else {
        let chat_name = name_hint
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| chat_uuid.to_string());

        log::debug!(
            "Creating new chat: uuid={}, channel_bytes len={}, name={}",
            chat_uuid,
            channel_bytes.len(),
            chat_name
        );

        let record = chats::ActiveModel {
            id: Set(chat_id_str.clone()),
            channel_id: Set(channel_bytes),
            client_id: Set(None),
            name: Set(chat_name),
        };

        record.insert(db).await?;
        log::info!("Successfully created chat: {}", chat_uuid);
    }

    Ok(())
}

async fn process_contact(
    company_bytes: &[u8],
    contact: WebhookContactEvent,
    db: &DatabaseConnection,
) -> Result<(), AppError> {
    let contact_uuid = match parse_uuid(&contact.contact_id) {
        Ok(uuid) => uuid,
        Err(err) => {
            log::warn!(
                "Skipping contact with invalid ID {}: {}",
                contact.contact_id,
                err
            );
            return Ok(());
        }
    };

    let id_bytes = uuid_to_bytes(&contact_uuid);
    let email = contact
        .email
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("{}@wazzup.local", contact_uuid));

    let sanitized_phone = contact
        .phone
        .and_then(|value| validation::sanitize_phone(&value));

    if let Some(existing) = clients::Entity::find_by_id(id_bytes.clone())
        .one(db)
        .await?
    {
        let mut active = existing.into_active_model();
        active.full_name = Set(contact
            .name
            .unwrap_or_else(|| "Unnamed contact".to_string()));
        active.email = Set(Some(email));
        active.phone = Set(sanitized_phone);
        active.company_id = Set(Some(company_bytes.to_vec()));
        active.update(db).await?;
    } else {
        let record = clients::ActiveModel {
            id: Set(id_bytes),
            company_id: Set(Some(company_bytes.to_vec())),
            full_name: Set(contact
                .name
                .unwrap_or_else(|| "Unnamed contact".to_string())),
            email: Set(Some(email)),
            phone: Set(sanitized_phone),
            responsible_user_id: Set(Uuid::nil().as_bytes().to_vec()),
            created_at: Set(Utc::now().into()),
        };
        record.insert(db).await?;
    }

    Ok(())
}

async fn handle_contacts(
    company_uuid: &Uuid,
    contacts: Vec<WebhookContactEvent>,
    db: &DatabaseConnection,
) -> Result<(), AppError> {
    let company_bytes = uuid_to_bytes(company_uuid);
    let total = contacts.len();
    log::info!(
        "Processing {} contact(s) for company {}",
        total,
        company_uuid
    );

    for (idx, contact) in contacts.into_iter().enumerate() {
        let contact_id = contact.contact_id.clone();
        if let Err(err) = process_contact(&company_bytes, contact, db).await {
            log::error!(
                "Failed to process contact #{} (id={}) for company {}: {}",
                idx + 1,
                contact_id,
                company_uuid,
                err
            );
        }
    }

    Ok(())
}

async fn process_message(
    company_uuid: &Uuid,
    message: WebhookMessage,
    db: &DatabaseConnection,
    bot_service: &BotService,
    wazzup_api: &WazzupApiService,
) -> Result<(), AppError> {
    let _ = (bot_service, wazzup_api);

    log::debug!(
        "Processing message: message_id={}, channel_id={}, chat_id={}",
        message.message_id,
        message.channel_id,
        message.chat_id
    );

    // Channel ID должен быть UUID
    let channel_bytes = parse_uuid_bytes(&message.channel_id).map_err(|e| {
        log::error!("Invalid channel_id '{}': {}", message.channel_id, e);
        e
    })?;
    ensure_channel(db, channel_bytes.clone(), &message.chat_type).await?;

    // Chat ID может быть числом или UUID - используем гибкий парсинг
    let chat_uuid = parse_flexible_uuid(&message.chat_id);
    log::debug!(
        "Chat ID '{}' converted to UUID: {}",
        message.chat_id,
        chat_uuid
    );
    ensure_chat(
        db,
        &chat_uuid,
        channel_bytes.clone(),
        message.client_name.as_deref(),
    )
    .await?;

    // Message ID может быть в любом формате - используем гибкий парсинг
    let message_uuid = parse_flexible_uuid(&message.message_id);
    log::debug!(
        "Message ID '{}' converted to UUID: {}",
        message.message_id,
        message_uuid
    );

    let message_bytes = uuid_to_bytes(&message_uuid);
    if messages::Entity::find_by_id(message_bytes.clone())
        .one(db)
        .await?
        .is_some()
    {
        return Ok(());
    }

    let (is_inbound, direction_status) = determine_message_direction(&message);
    let created_at = parse_date_time(message.date_time.as_ref());
    let author_bytes = parse_optional_uuid_bytes(message.author_id.as_ref());

    let record = messages::ActiveModel {
        id: Set(uuid_to_bytes(&message_uuid)),
        content: Set(build_message_content(&message)),
        chat_id: Set(chat_uuid.to_string()),
        is_inbound: Set(Some(if is_inbound { 1 } else { 0 })),
        is_echo: Set(message.is_echo.map(|value| if value { 1 } else { 0 })),
        direction_status: Set(Some(direction_status)),
        author_user_id: Set(author_bytes),
        created_at: Set(created_at.into()),
    };

    if let Err(err) = record.insert(db).await {
        log::error!(
            "Failed to store message {} for company {}: {}",
            message_uuid,
            company_uuid,
            err
        );
    }

    Ok(())
}

async fn handle_messages(
    company_uuid: &Uuid,
    messages: Vec<WebhookMessage>,
    db: &DatabaseConnection,
    bot_service: &BotService,
    wazzup_api: &WazzupApiService,
) -> Result<(), AppError> {
    let total = messages.len();
    log::info!(
        "Processing {} message(s) for company {}",
        total,
        company_uuid
    );

    for (idx, message) in messages.into_iter().enumerate() {
        let msg_id = message.message_id.clone();
        log::debug!("Processing message {}/{}: id={}", idx + 1, total, msg_id);

        if let Err(err) = process_message(company_uuid, message, db, bot_service, wazzup_api).await
        {
            log::error!(
                "Failed to process message #{} (id={}) for company {}: {}",
                idx + 1,
                msg_id,
                company_uuid,
                err
            );
        }
    }

    Ok(())
}

pub async fn handle_webhook(
    company_uuid: Uuid,
    webhook: WebhookRequest,
    db: &DatabaseConnection,
    bot_service: &BotService,
    wazzup_api: &WazzupApiService,
) -> Result<(), AppError> {
    let company_bytes = uuid_to_bytes(&company_uuid);
    let company = companies::Entity::find_by_id(company_bytes)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound("Company not found".to_string()))?;

    if company.is_active == Some(0) {
        log::warn!("Webhook received for inactive company {}", company_uuid);
        return Ok(());
    }

    if webhook.test == Some(true) {
        log::info!("Test webhook received for company {}", company_uuid);
        return Ok(());
    }

    if let Some(contacts) = webhook.contacts {
        handle_contacts(&company_uuid, contacts, db).await?;
    }

    if let Some(messages) = webhook.messages {
        handle_messages(&company_uuid, messages, db, bot_service, wazzup_api).await?;
    }

    Ok(())
}
