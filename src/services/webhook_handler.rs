use crate::database::main;
use crate::database::client::{
    wazzup_channels,
    wazzup_chats,
    wazzup_messages,
    users,
    clients::{Entity as ClientEntity, Column as ClientColumn, ActiveModel as ClientActiveModel},
};
use crate::services::wazzup_api::{WazzupApiService, WazzupContact, WazzupContactData, SendMessageRequest};
use crate::services::bot_service::{BotService, BotHookRequest};
use sea_orm::{Database, DatabaseConnection, EntityTrait, Set, NotSet, ActiveModelTrait, QueryFilter, ColumnTrait};
use utoipa::ToSchema;
use crate::errors::AppError;
use serde::{Deserialize, Serialize};
use serde_json;
use crate::config::Config;
use chrono::Utc;

#[derive(Debug, Serialize, Deserialize)]
struct MessageContentItem {
    r#type: String,
    content: String,
}

/// Валидирует и нормализует email адрес
fn validate_and_normalize_email(email: &str) -> Option<String> {
    let trimmed = email.trim();
    if trimmed.is_empty() || trimmed.len() > 254 {
        return None;
    }
    
    // Базовая проверка на валидность email
    if !trimmed.contains('@') || trimmed.starts_with('@') || trimmed.ends_with('@') {
        return None;
    }
    
    Some(trimmed.to_lowercase())
}

/// Валидирует и нормализует имя
fn validate_and_normalize_name(name: &str) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.len() > 255 {
        return None;
    }
    
    // Удаляем потенциально опасные символы
    let cleaned: String = trimmed.chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace() || "-_.,()[]{}".contains(*c))
        .collect();
    
    if cleaned.is_empty() {
        return None;
    }
    
    Some(cleaned)
}

/// Валидирует и нормализует номер телефона
fn validate_and_normalize_phone(phone: &str) -> Option<String> {
    let trimmed = phone.trim();
    if trimmed.is_empty() || trimmed.len() > 20 {
        return None;
    }
    
    // Удаляем все кроме цифр, +, -, (, ), пробелов
    let cleaned: String = trimmed.chars()
        .filter(|c| c.is_ascii_digit() || "+()-. ".contains(*c))
        .collect();
    
    if cleaned.is_empty() {
        return None;
    }
    
    Some(cleaned)
}

/// Валидирует ID (должен содержать только безопасные символы)
fn validate_id(id: &str) -> bool {
    if id.is_empty() || id.len() > 100 {
        return false;
    }
    
    // ID должен содержать только буквы, цифры, дефисы и подчеркивания
    id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

/// Определяет направление сообщения на основе полей isEcho и status
/// Возвращает (is_inbound: bool, direction_description: String)
pub fn determine_message_direction(msg: &WebhookMessage) -> (bool, String) {
    match msg.is_echo {
        Some(false) => (true, "ВХОДЯЩЕЕ (от клиента)".to_string()),
        Some(true) => (false, "ИСХОДЯЩЕЕ (отправлено не из API)".to_string()),
        None => {
            // Если is_echo отсутствует, проверяем status
            match msg.status.as_deref() {
                Some("inbound") => (true, "ВХОДЯЩЕЕ (по статусу)".to_string()),
                _ => (false, "НАПРАВЛЕНИЕ НЕ ОПРЕДЕЛЕНО".to_string())
            }
        }
    }
}

/// Находит первого пользователя с ролью "bot" в базе данных
async fn find_bot_user(client_db: &DatabaseConnection) -> Result<Option<i64>, sea_orm::DbErr> {
    let bot = users::Entity::find()
        .filter(users::Column::Role.eq("bot"))
        .one(client_db)
        .await?;
    
    Ok(bot.map(|b| b.id))
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WebhookContact {
    pub name: Option<String>,
    pub avatar_uri: Option<String>,
    pub username: Option<String>, // Только для Telegram
    pub phone: Option<String>, // Только для Telegram
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
    pub date_time: Option<String>, // Время отправки сообщения
    pub is_echo: Option<bool>, // false - входящее, true - исходящее
    pub status: Option<String>, // может содержать "inbound" для входящих
    pub contact: Option<WebhookContact>, // информация о контакте
    pub author_name: Option<String>, // имя пользователя отправившего сообщение
    pub author_id: Option<String>, // ID пользователя CRM
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

async fn get_client_db_conn(
    company: &main::companies::Model,
    config: &Config,
) -> Result<DatabaseConnection, AppError> {
    // Валидация имени базы данных
    if !company.database_name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        log::error!("Invalid database name: {}", company.database_name);
        return Err(AppError::InvalidInput("Invalid database name format".to_string()));
    }
    
    let db_url = config.client_database_url_template.replace("{db_name}", &company.database_name);
    Ok(Database::connect(&db_url).await?)
}

pub async fn handle_webhook(
    company_id: i64,
    webhook: WebhookRequest,
    main_db: &DatabaseConnection,
    config: &Config,
    bot_service: &BotService,
    wazzup_api: &WazzupApiService,
) -> Result<(), AppError> {
    let company = main::companies::Entity::find_by_id(company_id)
        .one(main_db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Company with id {} not found", company_id)))?;
    
    if company.is_active != Some(true) {
        log::warn!("Webhook for inactive company {}", company_id);
        return Ok(());
    }

    // Базовая защита от спама - ограничиваем количество контактов и сообщений
    if let Some(ref contacts) = webhook.contacts {
        if contacts.len() > 100 {
            log::error!("Too many contacts in webhook: {} (max 100)", contacts.len());
            return Err(AppError::InvalidInput("Too many contacts in single webhook request".to_string()));
        }
    }
    
    if let Some(ref messages) = webhook.messages {
        if messages.len() > 100 {
            log::error!("Too many messages in webhook: {} (max 100)", messages.len());
            return Err(AppError::InvalidInput("Too many messages in single webhook request".to_string()));
        }
    }

    log::info!("Processing webhook for company: {}", company.name);
    
    // ПОЛНЫЙ ДЕБАГ ВХОДЯЩИХ ДАННЫХ
    log::info!("=== WEBHOOK DEBUG START ===");
    log::info!("Company ID: {}", company_id);
    log::info!("Company Name: {}", company.name);
    log::info!("Webhook test flag: {:?}", webhook.test);
    
    if let Some(ref contacts) = webhook.contacts {
        log::info!("CONTACTS COUNT: {}", contacts.len());
        for (i, contact) in contacts.iter().enumerate() {
            log::info!("CONTACT [{}]:", i);
            log::info!("  - contact_id: '{}'", contact.contact_id);
            log::info!("  - name: {:?}", contact.name);
            log::info!("  - phone: {:?}", contact.phone);
            log::info!("  - email: {:?}", contact.email);
            log::info!("  - chat_id: {:?}", contact.chat_id);
            log::info!("  - channel_id: {:?}", contact.channel_id);
            log::info!("  - RAW JSON: Debug={:?}", contact);
        }
    } else {
        log::info!("NO CONTACTS in webhook");
    }
    
    if let Some(ref messages) = webhook.messages {
        log::info!("MESSAGES COUNT: {}", messages.len());
        for (i, message) in messages.iter().enumerate() {
            log::info!("MESSAGE [{}]:", i);
            log::info!("  - message_id: '{}'", message.message_id);
            log::info!("  - channel_id: '{}'", message.channel_id);
            log::info!("  - chat_type: '{}'", message.chat_type);
            log::info!("  - chat_id: '{}'", message.chat_id);
            log::info!("  - type: '{}'", message.r#type);
            log::info!("  - text: {:?}", message.text);
            log::info!("  - content_uri: {:?}", message.content_uri);
            log::info!("  - client_name: {:?}", message.client_name);
            log::info!("  - client_phone: {:?}", message.client_phone);
            log::info!("  - RAW JSON: Debug={:?}", message);
        }
    } else {
        log::info!("NO MESSAGES in webhook");
    }
    
    log::info!("FULL WEBHOOK RAW JSON: Debug={:?}", webhook);
    log::info!("=== WEBHOOK DEBUG END ===");

    if webhook.test == Some(true) {
        log::info!("Test webhook received for company {}", company_id);
        return Ok(());
    }

    // Создаем подключение к клиентской БД один раз для всех операций
    // ПРИМЕЧАНИЕ: Здесь мы пока используем прямое подключение, но в будущем 
    // можно передать AppState и использовать pool manager
    let client_db = get_client_db_conn(&company, config).await?;

    if let Some(contacts) = webhook.contacts {
        handle_contacts(company_id, contacts, &client_db).await?;
    }

    if let Some(messages) = webhook.messages {
        handle_messages(company_id, messages, &client_db, &company, bot_service, wazzup_api).await?;
    }

    // ... Handle other webhook types

    Ok(())
}

async fn handle_contacts(
    company_id: i64,
    contacts: Vec<WebhookContactEvent>,
    client_db: &DatabaseConnection,
) -> Result<(), AppError> {
    log::info!("=== HANDLING CONTACTS START ===");
    log::info!("Company ID: {}, Contacts count: {}", company_id, contacts.len());
    
    for (index, contact) in contacts.iter().enumerate() {
        log::info!("=== PROCESSING CONTACT {} of {} ===", index + 1, contacts.len());
        
        // Валидация входных данных
        if !validate_id(&contact.contact_id) {
            log::error!("INVALID contact_id: '{}' - skipping", contact.contact_id);
            continue;
        }
        
        log::info!("Processing contact {} for company {}", contact.contact_id, company_id);
        log::info!("Contact RAW data dump:");
        log::info!("  contact_id: '{}'", contact.contact_id);
        log::info!("  name: {:?} (value: '{}')", contact.name, contact.name.as_deref().unwrap_or("NULL"));
        log::info!("  phone: {:?} (value: '{}')", contact.phone, contact.phone.as_deref().unwrap_or("NULL"));
        log::info!("  email: {:?} (value: '{}')", contact.email, contact.email.as_deref().unwrap_or("NULL"));
        log::info!("  chat_id: {:?} (value: '{}')", contact.chat_id, contact.chat_id.as_deref().unwrap_or("NULL"));
        log::info!("  channel_id: {:?} (value: '{}')", contact.channel_id, contact.channel_id.as_deref().unwrap_or("NULL"));
        
        // Дополнительная проверка на пустые строки
        if let Some(ref name) = contact.name {
            log::info!("  name length: {}, is_empty: {}, trimmed: '{}'", name.len(), name.is_empty(), name.trim());
        }
        if let Some(ref phone) = contact.phone {
            log::info!("  phone length: {}, is_empty: {}, trimmed: '{}'", phone.len(), phone.is_empty(), phone.trim());
        }
        if let Some(ref email) = contact.email {
            log::info!("  email length: {}, is_empty: {}, trimmed: '{}'", email.len(), email.is_empty(), email.trim());
        }
        if let Some(ref chat_id) = contact.chat_id {
            log::info!("  chat_id length: {}, is_empty: {}, trimmed: '{}'", chat_id.len(), chat_id.is_empty(), chat_id.trim());
        }

        // Проверяем, существует ли уже клиент по нескольким критериям
        let mut client_exists = false;
        let mut existing_client_info = String::new();

        // Проверка по email
        if let Some(email) = &contact.email {
            if !email.trim().is_empty() {
                log::info!("Checking for existing client by email: '{}'", email);
                if let Some(existing_client) = ClientEntity::find()
                    .filter(ClientColumn::Email.eq(email))
                    .one(client_db)
                    .await? 
                {
                    log::info!("FOUND: Client with email '{}' already exists (ID: {}), full_name: '{}'", 
                              email, existing_client.id, existing_client.full_name);
                    existing_client_info = format!("by email '{}' - ID: {}, name: '{}'", email, existing_client.id, existing_client.full_name);
                    client_exists = true;
                } else {
                    log::info!("NOT FOUND: No client found with email '{}'", email);
                }
            } else {
                log::info!("SKIPPING email check: email is empty or whitespace only");
            }
        } else {
            log::info!("SKIPPING email check: email is None");
        }

        // Проверка по chat_id, если email не найден
        if !client_exists {
            if let Some(chat_id) = &contact.chat_id {
                if !chat_id.trim().is_empty() {
                    log::info!("Checking for existing client by chat_id: '{}'", chat_id);
                    if let Some(existing_client) = ClientEntity::find()
                        .filter(ClientColumn::WazzupChat.eq(chat_id))
                        .one(client_db)
                        .await? 
                    {
                        log::info!("FOUND: Client with chat_id '{}' already exists (ID: {}), full_name: '{}'", 
                                  chat_id, existing_client.id, existing_client.full_name);
                        existing_client_info = format!("by chat_id '{}' - ID: {}, name: '{}'", chat_id, existing_client.id, existing_client.full_name);
                        client_exists = true;
                    } else {
                        log::info!("NOT FOUND: No client found with chat_id '{}'", chat_id);
                    }
                } else {
                    log::info!("SKIPPING chat_id check: chat_id is empty or whitespace only");
                }
            } else {
                log::info!("SKIPPING chat_id check: chat_id is None");
            }
        }

        // Если клиент уже существует, пропускаем создание
        if client_exists {
            log::info!("CLIENT EXISTS: Skipping creation for contact_id '{}', found {}", contact.contact_id, existing_client_info);
            continue;
        }

        log::info!("CLIENT NOT EXISTS: Creating new client for contact_id '{}'", contact.contact_id);

        // Создаем нового клиента - подробный разбор полей с валидацией
        let full_name = if let Some(ref name) = contact.name {
            if let Some(validated_name) = validate_and_normalize_name(name) {
                log::info!("Using validated contact name as full_name: '{}'", validated_name);
                validated_name
            } else {
                log::info!("Contact name validation failed, falling back to phone or default");
                if let Some(ref phone) = contact.phone {
                    if let Some(validated_phone) = validate_and_normalize_phone(phone) {
                        log::info!("Using validated phone as full_name: '{}'", validated_phone);
                        validated_phone
                    } else {
                        log::info!("Phone validation failed, using default 'Неизвестный контакт' as full_name");
                        "Неизвестный контакт".to_string()
                    }
                } else {
                    log::info!("Using default 'Неизвестный контакт' as full_name");
                    "Неизвестный контакт".to_string()
                }
            }
        } else {
            log::info!("Contact name is None, falling back to phone or default");
            if let Some(ref phone) = contact.phone {
                if let Some(validated_phone) = validate_and_normalize_phone(phone) {
                    log::info!("Using validated phone as full_name: '{}'", validated_phone);
                    validated_phone
                } else {
                    log::info!("Phone validation failed, using default 'Неизвестный контакт' as full_name");
                    "Неизвестный контакт".to_string()
                }
            } else {
                log::info!("Using default 'Неизвестный контакт' as full_name");
                "Неизвестный контакт".to_string()
            }
        };
        
        let email = if let Some(ref email) = contact.email {
            if let Some(validated_email) = validate_and_normalize_email(email) {
                log::info!("Using validated email: '{}'", validated_email);
                validated_email
            } else {
                log::info!("Email validation failed, generating one from contact_id and timestamp");
                let timestamp = chrono::Utc::now().timestamp_millis();
                format!("{}+{}@generated.wazzup", contact.contact_id, timestamp)
            }
        } else {
            log::info!("Email is None, generating one from contact_id and timestamp");
            let timestamp = chrono::Utc::now().timestamp_millis();
            format!("{}+{}@generated.wazzup", contact.contact_id, timestamp)
        };

        let phone = contact.phone.as_ref()
            .and_then(|p| validate_and_normalize_phone(p))
            .map(|p| {
                log::info!("Using validated phone: '{}'", p);
                p
            });
        
        let wazzup_chat = contact.chat_id.as_ref()
            .filter(|c| !c.trim().is_empty())
            .map(|c| {
                log::info!("Using chat_id: '{}'", c);
                c.trim().to_string()
            });

        log::info!("CREATING CLIENT with fields:");
        log::info!("  full_name: '{}'", full_name);
        log::info!("  email: '{}'", email);
        log::info!("  phone: {:?}", phone);
        log::info!("  wazzup_chat: {:?}", wazzup_chat);

        // Ищем бота для автоматического назначения
        let bot_user_id = find_bot_user(client_db).await?;
        if bot_user_id.is_none() {
            log::warn!("No bot user found in database - creating client without responsible user");
        }

        let new_client = ClientActiveModel {
            id: NotSet, // Пусть база данных сама генерирует ID
            full_name: Set(full_name.clone()),
            email: Set(email.clone()),
            phone: Set(phone.clone()),
            wazzup_chat: Set(wazzup_chat.clone()),
            responsible_user_id: Set(bot_user_id),
            created_at: Set(Utc::now().into()),
        };

        match new_client.insert(client_db).await {
            Ok(client) => {
                log::info!("SUCCESS: Created new client from contact:");
                log::info!("  Client ID: {}", client.id);
                log::info!("  Client full_name: '{}'", client.full_name);
                log::info!("  Client email: '{}'", client.email);
                log::info!("  Client phone: {:?}", client.phone);
                log::info!("  Client wazzup_chat: {:?}", client.wazzup_chat);
                log::info!("  Client created_at: {}", client.created_at);
            },
            Err(e) => {
                log::error!("FAILED to create client from contact '{}': {}", contact.contact_id, e);
                log::error!("Error details: {:?}", e);
            }
        }
        
        log::info!("=== CONTACT {} PROCESSING COMPLETE ===", index + 1);
    }
    
    log::info!("=== HANDLING CONTACTS END ===");
    Ok(())
}

async fn handle_messages(
    company_id: i64,
    messages: Vec<WebhookMessage>,
    client_db: &DatabaseConnection,
    company: &main::companies::Model,
    bot_service: &BotService,
    wazzup_api: &WazzupApiService,
) -> Result<(), AppError> {
    log::info!("=== HANDLING MESSAGES START ===");
    log::info!("Company ID: {}, Messages count: {}", company_id, messages.len());
    
    for (index, msg) in messages.iter().enumerate() {
        log::info!("=== PROCESSING MESSAGE {} of {} ===", index + 1, messages.len());
        
        // Валидация входных данных
        if !validate_id(&msg.message_id) {
            log::error!("INVALID message_id: '{}' - skipping", msg.message_id);
            continue;
        }
        
        if !validate_id(&msg.channel_id) {
            log::error!("INVALID channel_id: '{}' - skipping", msg.channel_id);
            continue;
        }
        
        if !validate_id(&msg.chat_id) {
            log::error!("INVALID chat_id: '{}' - skipping", msg.chat_id);
            continue;
        }
        
        log::info!("Processing message {} of type '{}' for company {}", msg.message_id, msg.r#type, company_id);
        
        log::info!("Message RAW data dump:");
        log::info!("  message_id: '{}'", msg.message_id);
        log::info!("  channel_id: '{}'", msg.channel_id);
        log::info!("  chat_type: '{}'", msg.chat_type);
        log::info!("  chat_id: '{}'", msg.chat_id);
        log::info!("  type: '{}'", msg.r#type);
        log::info!("  text: {:?} (value: '{}')", msg.text, msg.text.as_deref().unwrap_or("NULL"));
        log::info!("  content_uri: {:?} (value: '{}')", msg.content_uri, msg.content_uri.as_deref().unwrap_or("NULL"));
        log::info!("  client_name: {:?} (value: '{}')", msg.client_name, msg.client_name.as_deref().unwrap_or("NULL"));
        log::info!("  client_phone: {:?} (value: '{}')", msg.client_phone, msg.client_phone.as_deref().unwrap_or("NULL"));
        log::info!("  date_time: {:?} (value: '{}')", msg.date_time, msg.date_time.as_deref().unwrap_or("NULL"));
        log::info!("  is_echo: {:?}", msg.is_echo);
        log::info!("  status: {:?} (value: '{}')", msg.status, msg.status.as_deref().unwrap_or("NULL"));
        log::info!("  author_name: {:?} (value: '{}')", msg.author_name, msg.author_name.as_deref().unwrap_or("NULL"));
        log::info!("  author_id: {:?} (value: '{}')", msg.author_id, msg.author_id.as_deref().unwrap_or("NULL"));
        
        // Определяем направление сообщения
        let (is_inbound, direction_description) = determine_message_direction(msg);
        log::info!("  MESSAGE DIRECTION: {} (is_inbound: {})", direction_description, is_inbound);
        
        if let Some(ref contact) = msg.contact {
            log::info!("  contact.name: {:?}", contact.name);
            log::info!("  contact.avatar_uri: {:?}", contact.avatar_uri);
            log::info!("  contact.username: {:?}", contact.username);
            log::info!("  contact.phone: {:?}", contact.phone);
        }
        
        // Дополнительная проверка на пустые строки
        if let Some(ref text) = msg.text {
            log::info!("  text length: {}, is_empty: {}, trimmed: '{}'", text.len(), text.is_empty(), text.trim());
        }
        if let Some(ref content_uri) = msg.content_uri {
            log::info!("  content_uri length: {}, is_empty: {}, trimmed: '{}'", content_uri.len(), content_uri.is_empty(), content_uri.trim());
        }
        if let Some(ref client_name) = msg.client_name {
            log::info!("  client_name length: {}, is_empty: {}, trimmed: '{}'", client_name.len(), client_name.is_empty(), client_name.trim());
        }
        if let Some(ref client_phone) = msg.client_phone {
            log::info!("  client_phone length: {}, is_empty: {}, trimmed: '{}'", client_phone.len(), client_phone.is_empty(), client_phone.trim());
        }

        // Проверяем, существует ли уже такое сообщение в базе данных по ID
        log::info!("Checking if message '{}' already exists in database", msg.message_id);
        if let Some(existing_message) = wazzup_messages::Entity::find()
            .filter(wazzup_messages::Column::Id.eq(&msg.message_id))
            .one(client_db)
            .await? 
        {
            log::info!("MESSAGE EXISTS BY ID: Message '{}' already exists in database, skipping processing", msg.message_id);
            log::info!("Existing message details:");
            log::info!("  id: '{}'", existing_message.id);
            log::info!("  content: '{}'", serde_json::to_string(&existing_message.content).unwrap_or_default());
            log::info!("  chat_id: '{}'", existing_message.chat_id);
            continue; // Пропускаем обработку этого сообщения
        }

        log::info!("MESSAGE NOT EXISTS BY ID: Message '{}' not found in database", msg.message_id);

        // Дополнительная проверка на дубликаты по содержимому
        // Генерируем содержимое для проверки в том же JSON формате
        let content_for_check = match msg.r#type.as_str() {
            "missing_call" => {
                let phone = msg.client_phone.clone().unwrap_or_else(|| msg.chat_id.clone());
                let name = msg.client_name.clone().unwrap_or_else(|| "Неизвестный контакт".to_string());
                let call_text = format!("Пропущенный звонок от {} ({})", name, phone);
                
                let content_items = vec![MessageContentItem {
                    r#type: "missing_call".to_string(),
                    content: call_text,
                }];
                
                serde_json::to_value(&content_items).unwrap_or(serde_json::json!([]))
            },
            _ => {
                // Генерируем тот же JSON для проверки дубликатов
                let mut content_items = Vec::new();
                
                if let Some(text) = &msg.text {
                    if !text.trim().is_empty() {
                        content_items.push(MessageContentItem {
                            r#type: "text".to_string(),
                            content: text.trim().to_string(),
                        });
                    }
                }
                
                if let Some(uri) = &msg.content_uri {
                    if !uri.trim().is_empty() {
                        content_items.push(MessageContentItem {
                            r#type: msg.r#type.clone(),
                            content: uri.trim().to_string(),
                        });
                    }
                }
                
                if content_items.is_empty() {
                    content_items.push(MessageContentItem {
                        r#type: msg.r#type.clone(),
                        content: format!("[{}]", msg.r#type),
                    });
                }
                
                serde_json::to_value(&content_items).unwrap_or(serde_json::json!([]))
            }
        };

        // Проверяем наличие сообщения с таким же содержимым и chat_id
        log::info!("Checking for duplicate message by content in chat '{}'", msg.chat_id);
        
        let duplicate_messages = wazzup_messages::Entity::find()
            .filter(wazzup_messages::Column::ChatId.eq(&msg.chat_id))
            .filter(wazzup_messages::Column::Content.eq(content_for_check.clone()))
            .all(client_db)
            .await?;

        if !duplicate_messages.is_empty() {
            log::warn!("DUPLICATE MESSAGE DETECTED: Found {} existing message(s) with same content", duplicate_messages.len());
            for (idx, dup_msg) in duplicate_messages.iter().enumerate() {
                log::warn!("  Duplicate {}: id='{}', content='{}'", idx + 1, dup_msg.id, serde_json::to_string(&dup_msg.content).unwrap_or_default());
            }
            log::warn!("SKIPPING: Message '{}' appears to be a duplicate based on content", msg.message_id);
            continue; // Пропускаем обработку этого сообщения
        }

        log::info!("MESSAGE NOT DUPLICATE: No duplicate found by content, proceeding with processing");

        // Ensure channel exists
        log::info!("Checking if channel '{}' exists", msg.channel_id);
        if wazzup_channels::Entity::find_by_id(msg.channel_id.clone()).one(client_db).await?.is_none() {
            log::info!("CHANNEL NOT EXISTS: Creating new channel with id '{}' and type '{}'", msg.channel_id, msg.chat_type);
            let new_channel = wazzup_channels::ActiveModel {
                id: Set(msg.channel_id.clone()),
                r#type: Set(msg.chat_type.clone()),
            };
            match new_channel.insert(client_db).await {
                Ok(channel) => {
                    log::info!("SUCCESS: Created channel with id '{}' and type '{}'", channel.id, channel.r#type);
                },
                Err(e) => {
                    log::error!("FAILED to create channel '{}': {}", msg.channel_id, e);
                }
            }
        } else {
            log::info!("CHANNEL EXISTS: Channel '{}' already exists", msg.channel_id);
        }

        // Ensure chat exists
        log::info!("Checking if chat '{}' exists", msg.chat_id);
        if wazzup_chats::Entity::find_by_id(msg.chat_id.clone()).one(client_db).await?.is_none() {
            log::info!("CHAT NOT EXISTS: Creating new chat with id '{}' and channel_id '{}'", msg.chat_id, msg.channel_id);
            let new_chat = wazzup_chats::ActiveModel {
                id: Set(msg.chat_id.clone()),
                channel_id: Set(msg.channel_id.clone()),
            };
            match new_chat.insert(client_db).await {
                Ok(chat) => {
                    log::info!("SUCCESS: Created chat with id '{}' and channel_id '{}'", chat.id, chat.channel_id);
                },
                Err(e) => {
                    log::error!("FAILED to create chat '{}': {}", msg.chat_id, e);
                }
            }
        } else {
            log::info!("CHAT EXISTS: Chat '{}' already exists", msg.chat_id);
        }

        // Всегда проверяем и создаем клиента, если его нет (независимо от того, новый чат или существующий)
        log::info!("Creating or checking client from message data");
        create_client_from_message(&msg, client_db, &company).await?;

        // Определяем содержимое сообщения в зависимости от типа как JSON массив элементов
        let content = match msg.r#type.as_str() {
            "missing_call" => {
                // Для пропущенного звонка создаем специальное сообщение
                let phone = msg.client_phone.as_ref()
                    .and_then(|p| validate_and_normalize_phone(p))
                    .unwrap_or_else(|| msg.chat_id.clone());
                let name = msg.client_name.as_ref()
                    .and_then(|n| validate_and_normalize_name(n))
                    .unwrap_or_else(|| "Неизвестный контакт".to_string());
                let call_text = format!("Пропущенный звонок от {} ({})", name, phone);
                
                let content_items = vec![MessageContentItem {
                    r#type: "missing_call".to_string(),
                    content: call_text,
                }];
                
                let json_value = serde_json::to_value(&content_items).unwrap_or(serde_json::json!([]));
                log::info!("MESSAGE TYPE missing_call: Generated JSON content: '{}'", serde_json::to_string(&json_value).unwrap_or_default());
                json_value
            },
            _ => {
                // Для всех типов сообщений создаем массив элементов контента
                let mut content_items = Vec::new();
                
                // Добавляем текст, если есть
                if let Some(text) = &msg.text {
                    if !text.trim().is_empty() {
                        content_items.push(MessageContentItem {
                            r#type: "text".to_string(),
                            content: text.trim().to_string(),
                        });
                        log::info!("Added text element: '{}'", text.trim());
                    }
                }
                
                // Добавляем медиа-контент, если есть
                if let Some(uri) = &msg.content_uri {
                    if !uri.trim().is_empty() {
                        content_items.push(MessageContentItem {
                            r#type: msg.r#type.clone(), // image, video, audio, document, etc.
                            content: uri.trim().to_string(),
                        });
                        log::info!("Added {} element: '{}'", msg.r#type, uri.trim());
                    }
                }
                
                // Если нет никакого контента, создаем пустой элемент с типом сообщения
                if content_items.is_empty() {
                    content_items.push(MessageContentItem {
                        r#type: msg.r#type.clone(),
                        content: format!("[{}]", msg.r#type),
                    });
                    log::info!("No content found, added placeholder for type: '{}'", msg.r#type);
                }
                
                let json_value = serde_json::to_value(&content_items).unwrap_or(serde_json::json!([]));
                
                // Проверяем размер JSON (при необходимости можно ограничить)
                let json_string = serde_json::to_string(&json_value).unwrap_or_default();
                if json_string.len() > 10000 {
                    log::warn!("JSON content too long ({}), but keeping full JSON structure", json_string.len());
                }
                
                log::info!("MESSAGE TYPE '{}': Generated JSON content: '{}'", msg.r#type, json_string);
                json_value
            }
        };

        log::info!("SAVING MESSAGE:");
        log::info!("  id: '{}'", msg.message_id);
        log::info!("  content (JSON): '{}'", serde_json::to_string(&content).unwrap_or_default());
        log::info!("  chat_id: '{}'", msg.chat_id);

        // Определяем направление сообщения для сохранения
        let (is_inbound, _direction_description) = determine_message_direction(msg);

        // Save message
        let new_message = wazzup_messages::ActiveModel {
            id: Set(msg.message_id.clone()),
            content: Set(content.clone()),
            chat_id: Set(msg.chat_id.clone()),
            created_at: Set(Utc::now().into()),
            is_inbound: Set(Some(is_inbound)),
            is_echo: Set(msg.is_echo),
            direction_status: Set(msg.status.clone()),
            author_name: Set(msg.author_name.clone()),
            author_id: Set(msg.author_id.clone()),
        };
        
        match new_message.insert(client_db).await {
            Ok(message) => {
                log::info!("SUCCESS: Saved message:");
                log::info!("  id: '{}'", message.id);
                log::info!("  content (JSON): '{}'", serde_json::to_string(&message.content).unwrap_or_default());
                log::info!("  chat_id: '{}'", message.chat_id);
                log::info!("  created_at: {}", message.created_at);
                log::info!("  is_inbound: {:?}", message.is_inbound);
                log::info!("  is_echo: {:?}", message.is_echo);
                log::info!("  direction_status: {:?}", message.direction_status);
                log::info!("  author_name: {:?}", message.author_name);
                log::info!("  author_id: {:?}", message.author_id);
                
                // Обрабатываем бота только для входящих сообщений
                if is_inbound {
                    if let Err(e) = handle_bot_interaction(
                        company_id,
                        &message,
                        client_db,
                        company,
                        bot_service,
                        wazzup_api,
                    ).await {
                        log::error!("Bot interaction failed: {}", e);
                    }
                }
            },
            Err(e) => {
                log::error!("FAILED to save message '{}': {}", msg.message_id, e);
                log::error!("Error details: {:?}", e);
            }
        }
        
        log::info!("=== MESSAGE {} PROCESSING COMPLETE ===", index + 1);
    }
    
    log::info!("=== HANDLING MESSAGES END ===");
    Ok(())
}

async fn create_client_from_message(
    msg: &WebhookMessage,
    client_db: &DatabaseConnection,
    company: &main::companies::Model,
) -> Result<(), AppError> {
    log::info!("=== CREATE CLIENT FROM MESSAGE START ===");
    log::info!("Checking if client exists for chat_id: '{}'", msg.chat_id);
    
    // Проверяем, есть ли уже клиент для этого чата
    if let Some(existing_client) = ClientEntity::find()
        .filter(ClientColumn::WazzupChat.eq(&msg.chat_id))
        .one(client_db)
        .await? 
    {
        log::info!("CLIENT EXISTS: Client already exists for chat '{}':", msg.chat_id);
        log::info!("  Client ID: {}", existing_client.id);
        log::info!("  Client full_name: '{}'", existing_client.full_name);
        log::info!("  Client email: '{}'", existing_client.email);
        log::info!("  Client phone: {:?}", existing_client.phone);
        log::info!("  Client wazzup_chat: {:?}", existing_client.wazzup_chat);
        log::info!("  Client created_at: {}", existing_client.created_at);
        log::info!("=== CREATE CLIENT FROM MESSAGE END (client exists) ===");
        return Ok(()); // Клиент уже существует
    }

    log::info!("CLIENT NOT EXISTS: No client found for chat_id: '{}', creating new client", msg.chat_id);

    // Логируем входные данные для создания клиента
    log::info!("Message data for client creation:");
    log::info!("  client_name: {:?} (value: '{}')", msg.client_name, msg.client_name.as_deref().unwrap_or("NULL"));
    log::info!("  client_phone: {:?} (value: '{}')", msg.client_phone, msg.client_phone.as_deref().unwrap_or("NULL"));
    log::info!("  chat_type: '{}'", msg.chat_type);
    log::info!("  chat_id: '{}'", msg.chat_id);

    // Определяем имя клиента с подробным логированием
    let full_name = if let Some(name) = &msg.client_name {
        if !name.trim().is_empty() {
            log::info!("USING client_name as full_name: '{}'", name);
            name.trim().to_string()
        } else {
            log::info!("client_name is empty, checking client_phone");
            if let Some(phone) = &msg.client_phone {
                if !phone.trim().is_empty() {
                    let name = format!("Клиент {}", phone.trim());
                    log::info!("USING phone-based full_name: '{}'", name);
                    name
                } else {
                    log::info!("client_phone is also empty, using chat_type and chat_id");
                    if msg.chat_type == "whatsapp" {
                        let name = format!("WhatsApp {}", msg.chat_id);
                        log::info!("USING WhatsApp-based full_name: '{}'", name);
                        name
                    } else {
                        let name = format!("Клиент {}", msg.chat_id);
                        log::info!("USING chat_id-based full_name: '{}'", name);
                        name
                    }
                }
            } else {
                log::info!("client_phone is None, using chat_type and chat_id");
                if msg.chat_type == "whatsapp" {
                    let name = format!("WhatsApp {}", msg.chat_id);
                    log::info!("USING WhatsApp-based full_name: '{}'", name);
                    name
                } else {
                    let name = format!("Клиент {}", msg.chat_id);
                    log::info!("USING chat_id-based full_name: '{}'", name);
                    name
                }
            }
        }
    } else {
        log::info!("client_name is None, checking client_phone");
        if let Some(phone) = &msg.client_phone {
            if !phone.trim().is_empty() {
                let name = format!("Клиент {}", phone.trim());
                log::info!("USING phone-based full_name: '{}'", name);
                name
            } else {
                log::info!("client_phone is empty, using chat_type and chat_id");
                if msg.chat_type == "whatsapp" {
                    let name = format!("WhatsApp {}", msg.chat_id);
                    log::info!("USING WhatsApp-based full_name: '{}'", name);
                    name
                } else {
                    let name = format!("Клиент {}", msg.chat_id);
                    log::info!("USING chat_id-based full_name: '{}'", name);
                    name
                }
            }
        } else {
            log::info!("client_phone is None, using chat_type and chat_id");
            if msg.chat_type == "whatsapp" {
                let name = format!("WhatsApp {}", msg.chat_id);
                log::info!("USING WhatsApp-based full_name: '{}'", name);
                name
            } else {
                let name = format!("Клиент {}", msg.chat_id);
                log::info!("USING chat_id-based full_name: '{}'", name);
                name
            }
        }
    };

    // Создаем уникальный email на основе chat_id и timestamp
    let timestamp = chrono::Utc::now().timestamp_millis();
    let email = format!("{}+{}@generated.wazzup", msg.chat_id, timestamp);
    log::info!("GENERATED email: '{}'", email);

    // Обрабатываем phone
    let phone = msg.client_phone.as_ref()
        .filter(|p| !p.trim().is_empty())
        .map(|p| {
            log::info!("USING phone: '{}'", p.trim());
            p.trim().to_string()
        });
    if phone.is_none() {
        log::info!("PHONE set to None (empty or missing)");
    }

    // Обрабатываем wazzup_chat
    let wazzup_chat = Some(msg.chat_id.clone());
    log::info!("USING wazzup_chat: {:?}", wazzup_chat);

    log::info!("FINAL CLIENT DATA TO INSERT:");
    log::info!("  full_name: '{}'", full_name);
    log::info!("  email: '{}'", email);
    log::info!("  phone: {:?}", phone);
    log::info!("  wazzup_chat: {:?}", wazzup_chat);

    // Ищем бота для автоматического назначения
    let bot_user_id = find_bot_user(client_db).await?;
    if bot_user_id.is_none() {
        log::warn!("No bot user found in database - creating client without responsible user");
    }

    let new_client = ClientActiveModel {
        id: NotSet, // Пусть база данных сама генерирует ID
        full_name: Set(full_name.clone()),
        email: Set(email.clone()),
        phone: Set(phone.clone()),
        wazzup_chat: Set(wazzup_chat.clone()),
        responsible_user_id: Set(bot_user_id),
        created_at: Set(Utc::now().into()),
    };

    match new_client.insert(client_db).await {
        Ok(client) => {
            log::info!("SUCCESS: Auto-created client from message:");
            log::info!("  Client ID: {}", client.id);
            log::info!("  Client full_name: '{}'", client.full_name);
            log::info!("  Client email: '{}'", client.email);
            log::info!("  Client phone: {:?}", client.phone);
            log::info!("  Client wazzup_chat: {:?}", client.wazzup_chat);
            log::info!("  Client created_at: {}", client.created_at);
            log::info!("  For chat: '{}'", msg.chat_id);
            
            // Теперь создаем контакт в Wazzup API
            log::info!("Creating Wazzup API contact for new client");
            create_wazzup_contact(&client, &msg, company).await;
        },
        Err(e) => {
            log::error!("FAILED to auto-create client from message: {}", e);
            log::error!("Error details: {:?}", e);
            log::error!("Attempted data:");
            log::error!("  full_name: '{}'", full_name);
            log::error!("  email: '{}'", email);
            log::error!("  phone: {:?}", phone);
            log::error!("  wazzup_chat: {:?}", wazzup_chat);
        }
    }

    log::info!("=== CREATE CLIENT FROM MESSAGE END ===");
    Ok(())
}

async fn create_wazzup_contact(
    client: &crate::database::client::clients::Model,
    msg: &WebhookMessage,
    company: &main::companies::Model,
) {
    log::info!("=== CREATE WAZZUP CONTACT START ===");
    log::info!("Creating Wazzup contact for client {} with chat_id {}", client.id, msg.chat_id);
    
    if company.wazzup_api_key.is_empty() {
        log::warn!("CANNOT CREATE WAZZUP CONTACT: API key not set for company {}", company.id);
        log::info!("=== CREATE WAZZUP CONTACT END (no API key) ===");
        return;
    }

    log::info!("Company API key is available (length: {})", company.wazzup_api_key.len());
    
    let wazzup_api = WazzupApiService::new();
    
    // Создаем контакт согласно API Wazzup
    let contact_data = WazzupContactData {
        chat_type: msg.chat_type.clone(),
        chat_id: msg.chat_id.clone(),
        username: None, // TODO: извлечь из msg если есть
        phone: msg.client_phone.clone(),
    };
    
    log::info!("WAZZUP CONTACT DATA:");
    log::info!("  chat_type: '{}'", contact_data.chat_type);
    log::info!("  chat_id: '{}'", contact_data.chat_id);
    log::info!("  username: {:?}", contact_data.username);
    log::info!("  phone: {:?}", contact_data.phone);
    
    let wazzup_contact = WazzupContact {
        id: format!("client_{}", client.id), // Уникальный ID в нашей CRM
        responsible_user_id: "1".to_string(),  // TODO: настроить ответственного
        name: client.full_name.clone(),
        contact_data: vec![contact_data],
        uri: None, // TODO: добавить ссылку на клиента в CRM если нужно
    };
    
    log::info!("WAZZUP CONTACT STRUCTURE:");
    log::info!("  id: '{}'", wazzup_contact.id);
    log::info!("  responsible_user_id: '{}'", wazzup_contact.responsible_user_id);
    log::info!("  name: '{}'", wazzup_contact.name);
    log::info!("  contact_data count: {}", wazzup_contact.contact_data.len());
    log::info!("  uri: {:?}", wazzup_contact.uri);
    log::info!("  RAW JSON: {}", serde_json::to_string(&wazzup_contact).unwrap_or_else(|_| "Failed to serialize".to_string()));
    
    log::info!("Making API call to create contact...");
    match wazzup_api.create_contacts(&company.wazzup_api_key, vec![wazzup_contact]).await {
        Ok(result) => {
            log::info!("SUCCESS: Successfully created Wazzup contact for client {} (chat_id: {})", 
                      client.id, msg.chat_id);
            log::info!("API response: {:?}", result);
        },
        Err(e) => {
            log::error!("FAILED: Failed to create Wazzup contact for client {} (chat_id: {}): {}", 
                       client.id, msg.chat_id, e);
            log::error!("Error details: {:?}", e);
        }
    }
    
    log::info!("=== CREATE WAZZUP CONTACT END ===");
}

/// Обрабатывает взаимодействие с ботом для входящих сообщений
async fn handle_bot_interaction(
    company_id: i64,
    message: &wazzup_messages::Model,
    client_db: &DatabaseConnection,
    company: &main::companies::Model,
    bot_service: &BotService,
    wazzup_api: &WazzupApiService,
) -> Result<(), AppError> {
    log::info!("=== BOT INTERACTION START ===");
    
    // Находим клиента по chat_id
    let client = ClientEntity::find()
        .filter(ClientColumn::WazzupChat.eq(&message.chat_id))
        .one(client_db)
        .await?;
    
    let Some(client) = client else {
        log::info!("No client found for chat_id: {}", message.chat_id);
        return Ok(());
    };
    
    // Проверяем, есть ли ответственный пользователь
    let Some(responsible_user_id) = client.responsible_user_id else {
        log::info!("No responsible user for client: {}", client.id);
        return Ok(());
    };
    
    // Получаем hook URL бота
    let hook_url = bot_service
        .get_bot_hook_url(client_db, responsible_user_id)
        .await?;
    
    let Some(hook_url) = hook_url else {
        log::info!("Responsible user {} is not a bot or has no hook URL", responsible_user_id);
        return Ok(());
    };
    
    // Извлекаем текст сообщения из JSON контента
    let message_text = extract_message_text(&message.content);
    
    // Создаем запрос к боту
    let bot_request = BotHookRequest {
        message: message_text,
        client: client.id,
        company: company_id,
    };
    
    log::info!("Sending request to bot: {}", hook_url);
    
    // Отправляем запрос к боту
    match bot_service.send_hook_request(&hook_url, &bot_request).await {
        Ok(bot_response) => {
            log::info!("Bot response: status={}, message={}", bot_response.status, bot_response.message);
            
            match bot_response.status.as_str() {
                "success" => {
                    // Отправляем ответ бота через Wazzup API
                    if let Err(e) = send_bot_message(
                        &message.chat_id,
                        &bot_response.message,
                        responsible_user_id,
                        client_db,
                        company,
                        wazzup_api,
                    ).await {
                        log::error!("Failed to send bot message: {}", e);
                    }
                },
                "error" => {
                    log::warn!("Bot returned error: {}", bot_response.message);
                    // Перенаправляем клиента на случайного менеджера
                    if let Err(e) = transfer_to_random_manager(
                        client.id,
                        client_db,
                        bot_service,
                    ).await {
                        log::error!("Failed to transfer to random manager: {}", e);
                    }
                },
                _ => {
                    log::warn!("Unknown bot response status: {}", bot_response.status);
                }
            }
        },
        Err(e) => {
            log::error!("Failed to contact bot: {}", e);
            // Перенаправляем клиента на случайного менеджера при ошибке связи с ботом
            if let Err(e) = transfer_to_random_manager(
                client.id,
                client_db,
                bot_service,
            ).await {
                log::error!("Failed to transfer to random manager after bot error: {}", e);
            }
        }
    }
    
    log::info!("=== BOT INTERACTION END ===");
    Ok(())
}

/// Извлекает текст сообщения из JSON контента
fn extract_message_text(content: &serde_json::Value) -> String {
    if let Some(content_array) = content.as_array() {
        for item in content_array {
            if let Some(item_type) = item.get("type").and_then(|t| t.as_str()) {
                if item_type == "text" {
                    if let Some(text) = item.get("content").and_then(|c| c.as_str()) {
                        return text.to_string();
                    }
                }
            }
        }
    }
    "".to_string()
}

/// Отправляет сообщение от бота через Wazzup API
async fn send_bot_message(
    chat_id: &str,
    message_text: &str,
    sender_id: i64,
    client_db: &DatabaseConnection,
    company: &main::companies::Model,
    wazzup_api: &WazzupApiService,
) -> Result<(), AppError> {
    log::info!("Sending bot message to chat: {}", chat_id);
    
    // Получаем информацию о чате и канале
    let chat_info = wazzup_chats::Entity::find_by_id(chat_id)
        .find_also_related(wazzup_channels::Entity)
        .one(client_db)
        .await?
        .ok_or_else(|| AppError::NotFound("Chat not found".to_string()))?;
    
    let (chat, channel) = chat_info;
    let channel_info = channel.ok_or_else(|| AppError::NotFound("Channel not found".to_string()))?;
    
    // Создаем запрос для отправки сообщения
    let send_request = SendMessageRequest {
        chat_id: Some(chat_id.to_string()),
        channel_id: Some(chat.channel_id),
        chat_type: Some(channel_info.r#type),
        sender_id,
        text: Some(message_text.to_string()),
        content_uri: None,
        crm_user_id: Some(sender_id.to_string()),
        crm_message_id: None,
    };
    
    // Отправляем сообщение
    wazzup_api.send_message(&company.wazzup_api_key, &send_request).await?;
    
    log::info!("Bot message sent successfully");
    Ok(())
}

/// Переназначает ответственного на случайного менеджера
async fn transfer_to_random_manager(
    client_id: i64,
    client_db: &DatabaseConnection,
    bot_service: &BotService,
) -> Result<(), AppError> {
    log::info!("Transferring client {} to random manager", client_id);
    
    // Выбираем случайного менеджера
    let manager = bot_service.select_random_manager(client_db).await?;
    
    // Обновляем ответственного для клиента
    let mut client: ClientActiveModel = ClientEntity::find_by_id(client_id)
        .one(client_db)
        .await?
        .ok_or_else(|| AppError::NotFound("Client not found".to_string()))?
        .into();
    
    client.responsible_user_id = Set(Some(manager.id));
    client.update(client_db).await?;
    
    log::info!("Client {} transferred to manager {}", client_id, manager.id);
    Ok(())
}