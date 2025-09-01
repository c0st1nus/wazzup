use crate::database::main::models as main_models;
use crate::database::client::models::{
    wazzup_channel,
    wazzup_chat,
    wazzup_message,
    Entity as ClientEntity,
    Column as ClientColumn,
    ActiveModel as ClientActiveModel,
};
use crate::services::wazzup_api::{WazzupApiService, WazzupContact, WazzupContactData};
use sea_orm::{Database, DatabaseConnection, EntityTrait, Set, NotSet, ActiveModelTrait, QueryFilter, ColumnTrait};
use utoipa::ToSchema;
use crate::errors::AppError;
use serde::Deserialize;
use crate::config::Config;
use chrono::Utc;

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
    pub client_name: Option<String>,
    pub client_phone: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WebhookContact {
    pub contact_id: String,
    pub name: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub chat_id: Option<String>,
    pub channel_id: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WebhookRequest {
    pub test: Option<bool>,
    pub messages: Option<Vec<WebhookMessage>>,
    pub contacts: Option<Vec<WebhookContact>>,
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

    if let Some(contacts) = webhook.contacts {
        let client_db = get_client_db_conn(&company, config).await?;
        handle_contacts(company_id, contacts, &client_db).await?;
    }

    if let Some(messages) = webhook.messages {
        let client_db = get_client_db_conn(&company, config).await?;
        handle_messages(company_id, messages, &client_db, &company).await?;
    }

    // ... Handle other webhook types

    Ok(())
}

async fn handle_contacts(
    company_id: i64,
    contacts: Vec<WebhookContact>,
    client_db: &DatabaseConnection,
) -> Result<(), AppError> {
    log::info!("=== HANDLING CONTACTS START ===");
    log::info!("Company ID: {}, Contacts count: {}", company_id, contacts.len());
    
    for (index, contact) in contacts.iter().enumerate() {
        log::info!("=== PROCESSING CONTACT {} of {} ===", index + 1, contacts.len());
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

        // Создаем нового клиента - подробный разбор полей
        let full_name = if let Some(ref name) = contact.name {
            if !name.trim().is_empty() {
                log::info!("Using contact name as full_name: '{}'", name);
                name.trim().to_string()
            } else {
                log::info!("Contact name is empty, falling back to phone or default");
                contact.phone.as_ref()
                    .filter(|p| !p.trim().is_empty())
                    .map(|p| {
                        log::info!("Using phone as full_name: '{}'", p);
                        p.trim().to_string()
                    })
                    .unwrap_or_else(|| {
                        log::info!("Using default 'Неизвестный контакт' as full_name");
                        "Неизвестный контакт".to_string()
                    })
            }
        } else {
            log::info!("Contact name is None, falling back to phone or default");
            contact.phone.as_ref()
                .filter(|p| !p.trim().is_empty())
                .map(|p| {
                    log::info!("Using phone as full_name: '{}'", p);
                    p.trim().to_string()
                })
                .unwrap_or_else(|| {
                    log::info!("Using default 'Неизвестный контакт' as full_name");
                    "Неизвестный контакт".to_string()
                })
        };
        
        let email = if let Some(ref email) = contact.email {
            if !email.trim().is_empty() {
                log::info!("Using provided email: '{}'", email);
                email.trim().to_string()
            } else {
                log::info!("Email is empty, generating one from contact_id");
                format!("{}@generated.wazzup", contact.contact_id)
            }
        } else {
            log::info!("Email is None, generating one from contact_id");
            format!("{}@generated.wazzup", contact.contact_id)
        };

        let phone = contact.phone.as_ref()
            .filter(|p| !p.trim().is_empty())
            .map(|p| {
                log::info!("Using phone: '{}'", p);
                p.trim().to_string()
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

        let new_client = ClientActiveModel {
            id: Set(0), // auto-increment
            full_name: Set(full_name.clone()),
            email: Set(email.clone()),
            phone: Set(phone.clone()),
            wazzup_chat: Set(wazzup_chat.clone()),
            created_at: Set(Utc::now()),
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
    company: &main_models::Model,
) -> Result<(), AppError> {
    log::info!("=== HANDLING MESSAGES START ===");
    log::info!("Company ID: {}, Messages count: {}", company_id, messages.len());
    
    for (index, msg) in messages.iter().enumerate() {
        log::info!("=== PROCESSING MESSAGE {} of {} ===", index + 1, messages.len());
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
        if let Some(existing_message) = wazzup_message::Entity::find_by_id(msg.message_id.clone()).one(client_db).await? {
            log::info!("MESSAGE EXISTS BY ID: Message '{}' already exists in database, skipping processing", msg.message_id);
            log::info!("Existing message details:");
            log::info!("  id: '{}'", existing_message.id);
            log::info!("  type: '{}'", existing_message.r#type);
            log::info!("  content: '{}'", existing_message.content);
            log::info!("  chat_id: '{}'", existing_message.chat_id);
            continue; // Пропускаем обработку этого сообщения
        }

        log::info!("MESSAGE NOT EXISTS BY ID: Message '{}' not found in database by ID", msg.message_id);

        // Дополнительная проверка на дубликаты по содержимому
        // Сначала получаем содержимое сообщения для проверки
        let content_for_check = match msg.r#type.as_str() {
            "missing_call" => {
                let phone = msg.client_phone.clone().unwrap_or_else(|| msg.chat_id.clone());
                let name = msg.client_name.clone().unwrap_or_else(|| "Неизвестный контакт".to_string());
                format!("Пропущенный звонок от {} ({})", name, phone)
            },
            _ => {
                msg.content_uri.clone().or(msg.text.clone()).unwrap_or_default()
            }
        };

        // Проверяем наличие сообщения с таким же содержимым, типом и chat_id
        log::info!("Checking for duplicate message by content in chat '{}' with type '{}' and content '{}'", 
                   msg.chat_id, msg.r#type, content_for_check);
        
        let duplicate_messages = wazzup_message::Entity::find()
            .filter(wazzup_message::Column::ChatId.eq(&msg.chat_id))
            .filter(wazzup_message::Column::Type.eq(&msg.r#type))
            .filter(wazzup_message::Column::Content.eq(&content_for_check))
            .all(client_db)
            .await?;

        if !duplicate_messages.is_empty() {
            log::warn!("DUPLICATE MESSAGE DETECTED: Found {} existing message(s) with same content", duplicate_messages.len());
            for (idx, dup_msg) in duplicate_messages.iter().enumerate() {
                log::warn!("  Duplicate {}: id='{}', content='{}'", idx + 1, dup_msg.id, dup_msg.content);
            }
            log::warn!("SKIPPING: Message '{}' appears to be a duplicate based on content", msg.message_id);
            continue; // Пропускаем обработку этого сообщения
        }

        log::info!("MESSAGE NOT DUPLICATE: No duplicate found by content, proceeding with processing");

        // Ensure channel exists
        log::info!("Checking if channel '{}' exists", msg.channel_id);
        if wazzup_channel::Entity::find_by_id(msg.channel_id.clone()).one(client_db).await?.is_none() {
            log::info!("CHANNEL NOT EXISTS: Creating new channel with id '{}' and type '{}'", msg.channel_id, msg.chat_type);
            let new_channel = wazzup_channel::ActiveModel {
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
        if wazzup_chat::Entity::find_by_id(msg.chat_id.clone()).one(client_db).await?.is_none() {
            log::info!("CHAT NOT EXISTS: Creating new chat with id '{}' and channel_id '{}'", msg.chat_id, msg.channel_id);
            let new_chat = wazzup_chat::ActiveModel {
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

        // Определяем содержимое сообщения в зависимости от типа
        let content = match msg.r#type.as_str() {
            "missing_call" => {
                // Для пропущенного звонка создаем специальное сообщение
                let phone = msg.client_phone.clone().unwrap_or_else(|| msg.chat_id.clone());
                let name = msg.client_name.clone().unwrap_or_else(|| "Неизвестный контакт".to_string());
                let content = format!("Пропущенный звонок от {} ({})", name, phone);
                log::info!("MESSAGE TYPE missing_call: Generated content: '{}'", content);
                content
            },
            _ => {
                // Для обычных сообщений берем текст или URI контента
                let content = msg.content_uri.clone().or(msg.text.clone()).unwrap_or_default();
                log::info!("MESSAGE TYPE '{}': Using content: '{}'", msg.r#type, content);
                content
            }
        };

        log::info!("SAVING MESSAGE:");
        log::info!("  id: '{}'", msg.message_id);
        log::info!("  type: '{}'", msg.r#type);
        log::info!("  content: '{}'", content);
        log::info!("  chat_id: '{}'", msg.chat_id);

        // Save message
        let new_message = wazzup_message::ActiveModel {
            id: Set(msg.message_id.clone()),
            r#type: Set(msg.r#type.clone()),
            content: Set(content.clone()),
            chat_id: Set(msg.chat_id.clone()),
            created_at: Set(Utc::now()),
        };
        
        match new_message.insert(client_db).await {
            Ok(message) => {
                log::info!("SUCCESS: Saved message:");
                log::info!("  id: '{}'", message.id);
                log::info!("  type: '{}'", message.r#type);
                log::info!("  content: '{}'", message.content);
                log::info!("  chat_id: '{}'", message.chat_id);
                log::info!("  created_at: {}", message.created_at);
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
    company: &main_models::Model,
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

    // Создаем email на основе chat_id
    let email = format!("{}@generated.wazzup", msg.chat_id);
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

    let new_client = ClientActiveModel {
        id: NotSet, // Пусть база данных сама генерирует ID
        full_name: Set(full_name.clone()),
        email: Set(email.clone()),
        phone: Set(phone.clone()),
        wazzup_chat: Set(wazzup_chat.clone()),
        created_at: Set(Utc::now()),
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
    client: &crate::database::client::models::Model,
    msg: &WebhookMessage,
    company: &main_models::Model,
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