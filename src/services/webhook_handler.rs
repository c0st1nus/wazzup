use crate::database::main::models as main_models;
use crate::database::client::models::{
    wazzup_channel,
    wazzup_chat,
    wazzup_message,
};
use sea_orm::{Database, DatabaseConnection, EntityTrait, Set, ActiveModelTrait};
use utoipa::ToSchema;
use crate::errors::AppError;
use serde::Deserialize;
use crate::config::Config;

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WebhookMessage {
    pub message_id: String,
    pub channel_id: String,
    pub chat_type: String,
    pub chat_id: String,
    pub r#type: String,
    pub text: Option<String>,
    pub content_uri: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WebhookRequest {
    pub test: Option<bool>,
    pub messages: Option<Vec<WebhookMessage>>,
}

async fn get_client_db_conn(
    company: &main_models::Model,
    config: &Config,
) -> Result<DatabaseConnection, AppError> {
    let db_url = config.client_database_url_template.replace("{db_name}", &company.database_name);
    Ok(Database::connect(&db_url).await?)
}

pub async fn handle_webhook(
    company_id: i64,
    webhook: WebhookRequest,
    main_db: &DatabaseConnection,
    config: &Config,
) -> Result<(), AppError> {
    let company = main_models::Entity::find_by_id(company_id)
        .one(main_db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Company with id {} not found", company_id)))?;
    
    if company.is_active != Some(true) {
        log::warn!("Webhook for inactive company {}", company_id);
        return Ok(());
    }

    log::info!("Processing webhook for company: {}", company.name);

    if webhook.test == Some(true) {
        log::info!("Test webhook received for company {}", company_id);
        return Ok(());
    }

    if let Some(messages) = webhook.messages {
        let client_db = get_client_db_conn(&company, config).await?;
        handle_messages(company_id, messages, &client_db).await?;
    }

    // ... Handle other webhook types

    Ok(())
}

async fn handle_messages(
    company_id: i64,
    messages: Vec<WebhookMessage>,
    client_db: &DatabaseConnection,
) -> Result<(), AppError> {
    for msg in messages {
        log::info!("Processing message {} for company {}", msg.message_id, company_id);

        // Ensure channel exists
        if wazzup_channel::Entity::find_by_id(msg.channel_id.clone()).one(client_db).await?.is_none() {
            let new_channel = wazzup_channel::ActiveModel {
                id: Set(msg.channel_id.clone()),
                r#type: Set(msg.chat_type.clone()),
            };
            new_channel.insert(client_db).await?;
        }

        // Ensure chat exists
        if wazzup_chat::Entity::find_by_id(msg.chat_id.clone()).one(client_db).await?.is_none() {
            let new_chat = wazzup_chat::ActiveModel {
                id: Set(msg.chat_id.clone()),
                channel_id: Set(msg.channel_id.clone()),
            };
            new_chat.insert(client_db).await?;
        }

        // Save message
        let content = msg.content_uri.clone().or(msg.text.clone()).unwrap_or_default();
        let new_message = wazzup_message::ActiveModel {
            id: Set(msg.message_id.clone()),
            r#type: Set(msg.r#type.clone()),
            content: Set(content),
            chat_id: Set(msg.chat_id.clone()),
        };
        new_message.insert(client_db).await?;
    }
    Ok(())
}