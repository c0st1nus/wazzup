use actix_web::{get, web, HttpResponse};
use sea_orm::{DatabaseConnection, EntityTrait, QueryFilter, ColumnTrait, QuerySelect, QueryOrder, PaginatorTrait};
use crate::{
    database::client::models as client_models,
    errors::AppError,
    AppState,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;


#[derive(Serialize, Deserialize, ToSchema)]
pub struct ChatResponse {
    pub id: String,
    pub channel_id: String,
    pub channel_type: Option<String>,
    pub chat_name: String,
    pub client: Option<ClientInfo>, // Заменяем client_id на полную информацию о клиенте
    pub responsible_user: Option<ResponsibleUserInfo>,
    pub last_message: Option<MessageInfo>,
    pub last_message_date: Option<chrono::DateTime<chrono::Utc>>,
    pub unread_count: i64,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct ResponsibleUserInfo {
    pub id: i64,
    pub name: String,
    pub role: String,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct ClientInfo {
    pub id: i64,
    pub full_name: String,
    pub email: String,
    pub phone: Option<String>,
    pub wazzup_chat: Option<String>,
    pub responsible_user_id: Option<i64>,
    #[schema(value_type = String, format = DateTime)]
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct MessageInfo {
    pub id: String,
    pub r#type: String,
    pub content: String,
    pub client_id: Option<i64>, // Добавляем client_id
    #[schema(value_type = String, format = DateTime)]
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct ChatListResponse {
    pub chats: Vec<ChatResponse>,
    pub total: usize,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct ChatDetailsResponse {
    pub id: String,
    pub channel_id: String,
    pub channel_type: Option<String>,
    pub client: Option<ClientInfo>, // Заменяем client_id на полную информацию о клиенте
    pub messages: Vec<MessageInfo>,
    pub messages_count: i64,
}

// --- Helper Functions ---

/// Получает количество непрочитанных сообщений для чата
/// В данном случае считаем все сообщения как непрочитанные для простоты
async fn get_unread_count_for_chat(
    chat_id: &str,
    db: &DatabaseConnection,
) -> Result<i64, AppError> {
    let count = client_models::wazzup_message::Entity::find()
        .filter(client_models::wazzup_message::Column::ChatId.eq(chat_id))
        .count(db)
        .await?;
    
    Ok(count as i64)
}

/// Получает последнее сообщение для чата
async fn get_last_message_for_chat(
    chat_id: &str,
    client_id: Option<i64>,
    db: &DatabaseConnection,
) -> Result<Option<MessageInfo>, AppError> {
    let message = client_models::wazzup_message::Entity::find()
        .filter(client_models::wazzup_message::Column::ChatId.eq(chat_id))
        .order_by_desc(client_models::wazzup_message::Column::CreatedAt)
        .one(db)
        .await?;

    Ok(message.map(|m| MessageInfo {
        id: m.id,
        r#type: m.r#type,
        content: m.content,
        client_id,
        created_at: m.created_at,
    }))
}

// --- Route Handlers ---

#[utoipa::path(
    get,
    path = "/api/chats/{companyId}/{user_id}",
    tag = "Chats",
    params(
        ("companyId" = i64, Path, description = "Company ID"),
        ("user_id" = i64, Path, description = "User ID for filtering chats by access rights")
    ),
    responses(
        (status = 200, description = "List of chats with basic information", body = ChatListResponse),
        (status = 404, description = "Company not found"),
        (status = 403, description = "Access denied")
    )
)]
#[get("/{companyId}/{user_id}")]
async fn get_chats(
    app_state: web::Data<AppState>,
    path: web::Path<(i64, i64)>,
) -> Result<HttpResponse, AppError> {
    let (company_id, user_id) = path.into_inner();
    
    // Get client database connection using pool manager
    let client_db = crate::api::helpers::get_client_db_connection(company_id, &app_state).await?;

    // Получаем информацию о пользователе
    let user = client_models::user::Entity::find_by_id(user_id)
        .one(&client_db)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    // Получаем всех клиентов с их чатами (только с ответственными)
    let clients_with_chats = client_models::Entity::find()
        .filter(client_models::Column::WazzupChat.is_not_null())
        .filter(client_models::Column::ResponsibleUserId.is_not_null())
        .find_also_related(client_models::wazzup_chat::Entity)
        .all(&client_db)
        .await?;

    let mut accessible_clients = Vec::new();

    // Фильтруем клиентов по правам доступа
    for (client, chat) in clients_with_chats {
        if let Some(_chat) = chat {
            let has_access = match user.role.as_str() {
                "admin" => true, // Админ видит все чаты
                "manager" => {
                    // У всех клиентов уже есть ответственный (фильтр выше)
                    let responsible_id = client.responsible_user_id.unwrap();
                    
                    // Получаем информацию об ответственном
                    let responsible = client_models::user::Entity::find_by_id(responsible_id)
                        .one(&client_db)
                        .await?;
                    
                    if let Some(resp) = responsible {
                        // Если ответственный - бот, то видят все менеджеры
                        // Если ответственный - менеджер/админ, то видит только он сам
                        resp.role == "bot" || resp.id == user_id
                    } else {
                        false
                    }
                },
                "quality_controll" => {
                    // quality_controll видит все чаты (для контроля качества)
                    true
                },
                _ => false,
            };

            if has_access {
                accessible_clients.push(client);
            }
        }
    }

    let mut chat_responses = Vec::new();

    for client in accessible_clients {
        if let Some(chat_id) = &client.wazzup_chat {
            // Получаем информацию о чате
            let chat = client_models::wazzup_chat::Entity::find_by_id(chat_id)
                .find_also_related(client_models::wazzup_channel::Entity)
                .one(&client_db)
                .await?;

            if let Some((chat_data, channel)) = chat {
                let last_message = get_last_message_for_chat(chat_id, Some(client.id), &client_db).await?;
                let unread_count = get_unread_count_for_chat(chat_id, &client_db).await?;

                // Получаем информацию об ответственном
                let responsible_user = {
                    let responsible_id = client.responsible_user_id.unwrap(); // Уже проверено выше
                    let responsible = client_models::user::Entity::find_by_id(responsible_id)
                        .one(&client_db)
                        .await?;
                    
                    responsible.map(|r| ResponsibleUserInfo {
                        id: r.id,
                        name: r.name,
                        role: r.role,
                    })
                };

                // Извлекаем дату последнего сообщения
                let last_message_date = last_message.as_ref().map(|m| m.created_at);

                chat_responses.push(ChatResponse {
                    id: chat_data.id,
                    channel_id: chat_data.channel_id,
                    channel_type: channel.map(|c| c.r#type),
                    chat_name: client.full_name.clone(), // Используем имя клиента как название чата
                    client: Some(ClientInfo {
                        id: client.id,
                        full_name: client.full_name,
                        email: client.email,
                        phone: client.phone,
                        wazzup_chat: client.wazzup_chat,
                        responsible_user_id: client.responsible_user_id,
                        created_at: client.created_at,
                    }),
                    responsible_user,
                    last_message,
                    last_message_date,
                    unread_count,
                });
            }
        }
    }

    // Сортируем по дате последнего сообщения (новые сверху)
    chat_responses.sort_by(|a, b| {
        match (a.last_message_date, b.last_message_date) {
            (Some(a_date), Some(b_date)) => b_date.cmp(&a_date),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.chat_name.cmp(&b.chat_name),
        }
    });

    let total = chat_responses.len();

    Ok(HttpResponse::Ok().json(ChatListResponse {
        chats: chat_responses,
        total,
    }))
}

#[utoipa::path(
    get,
    path = "/api/chats/{companyId}/{chatId}/{user_id}",
    tag = "Chats",
    params(
        ("companyId" = i64, Path, description = "Company ID"),
        ("chatId" = String, Path, description = "Chat ID"),
        ("user_id" = i64, Path, description = "User ID for access validation")
    ),
    responses(
        (status = 200, description = "Chat details with all messages", body = ChatDetailsResponse),
        (status = 404, description = "Company or chat not found"),
        (status = 403, description = "Access denied")
    )
)]
#[get("/{companyId}/{chatId}/{user_id}")]
async fn get_chat_details(
    app_state: web::Data<AppState>,
    path: web::Path<(i64, String, i64)>,
) -> Result<HttpResponse, AppError> {
    let (company_id, chat_id, user_id) = path.into_inner();

    // Get client database connection using pool manager
    let client_db = crate::api::helpers::get_client_db_connection(company_id, &app_state).await?;

    // Получаем информацию о пользователе
    let user = client_models::user::Entity::find_by_id(user_id)
        .one(&client_db)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    // Находим клиента по chat_id
    let client = client_models::Entity::find()
        .filter(client_models::Column::WazzupChat.eq(&chat_id))
        .filter(client_models::Column::ResponsibleUserId.is_not_null())
        .one(&client_db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Client with chat {} not found or has no responsible user", chat_id)))?;

    // Проверяем права доступа к чату
    let has_access = match user.role.as_str() {
        "admin" | "quality_controll" => true,
        "manager" => {
            let responsible_id = client.responsible_user_id.unwrap();
            let responsible = client_models::user::Entity::find_by_id(responsible_id)
                .one(&client_db)
                .await?;
            
            if let Some(resp) = responsible {
                resp.role == "bot" || resp.id == user_id
            } else {
                false
            }
        },
        _ => false,
    };

    if !has_access {
        return Err(AppError::Forbidden("Access denied to this chat".to_string()));
    }

    // Проверяем существование чата
    let chat = client_models::wazzup_chat::Entity::find_by_id(&chat_id)
        .find_also_related(client_models::wazzup_channel::Entity)
        .one(&client_db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Chat with id {} not found", chat_id)))?;

    // Получаем все сообщения чата
    let messages = client_models::wazzup_message::Entity::find()
        .filter(client_models::wazzup_message::Column::ChatId.eq(&chat_id))
        .order_by_asc(client_models::wazzup_message::Column::CreatedAt)
        .all(&client_db)
        .await?;

    let message_infos: Vec<MessageInfo> = messages
        .into_iter()
        .map(|m| MessageInfo {
            id: m.id,
            r#type: m.r#type,
            content: m.content,
            client_id: Some(client.id),
            created_at: m.created_at,
        })
        .collect();

    let messages_count = message_infos.len() as i64;

    let (chat, channel) = chat;

    Ok(HttpResponse::Ok().json(ChatDetailsResponse {
        id: chat.id,
        channel_id: chat.channel_id,
        channel_type: channel.map(|c| c.r#type),
        client: Some(ClientInfo {
            id: client.id,
            full_name: client.full_name,
            email: client.email,
            phone: client.phone,
            wazzup_chat: client.wazzup_chat,
            responsible_user_id: client.responsible_user_id,
            created_at: client.created_at,
        }),
        messages: message_infos,
        messages_count,
    }))
}

#[derive(Deserialize, ToSchema)]
struct ChatMessagesQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

#[utoipa::path(
    get,
    path = "/api/chats/{companyId}/{chatId}/{user_id}/messages",
    tag = "Chats",
    params(
        ("companyId" = i64, Path, description = "Company ID"),
        ("chatId" = String, Path, description = "Chat ID"),
        ("user_id" = i64, Path, description = "User ID for access validation"),
        ("limit" = Option<i64>, Query, description = "Maximum number of messages to return"),
        ("offset" = Option<i64>, Query, description = "Number of messages to skip")
    ),
    responses(
        (status = 200, description = "Messages from the chat", body = Vec<MessageInfo>),
        (status = 404, description = "Company or chat not found"),
        (status = 403, description = "Access denied")
    )
)]
#[get("/{companyId}/{chatId}/messages")]
async fn get_chat_messages(
    app_state: web::Data<AppState>,
    path: web::Path<(i64, String, i64)>,
    query: web::Query<ChatMessagesQuery>,
) -> Result<HttpResponse, AppError> {
    let (company_id, chat_id, user_id) = path.into_inner();

    // Get client database connection using pool manager
    let client_db = crate::api::helpers::get_client_db_connection(company_id, &app_state).await?;

    // Получаем информацию о пользователе
    let user = client_models::user::Entity::find_by_id(user_id)
        .one(&client_db)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    // Находим клиента по chat_id
    let client = client_models::Entity::find()
        .filter(client_models::Column::WazzupChat.eq(&chat_id))
        .filter(client_models::Column::ResponsibleUserId.is_not_null())
        .one(&client_db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Client with chat {} not found or has no responsible user", chat_id)))?;

    // Проверяем права доступа к чату
    let has_access = match user.role.as_str() {
        "admin" | "quality_controll" => true,
        "manager" => {
            let responsible_id = client.responsible_user_id.unwrap();
            let responsible = client_models::user::Entity::find_by_id(responsible_id)
                .one(&client_db)
                .await?;
            
            if let Some(resp) = responsible {
                resp.role == "bot" || resp.id == user_id
            } else {
                false
            }
        },
        _ => false,
    };

    if !has_access {
        return Err(AppError::Forbidden("Access denied to this chat".to_string()));
    }

    // Проверяем существование чата
    let chat_exists = client_models::wazzup_chat::Entity::find_by_id(&chat_id)
        .one(&client_db)
        .await?
        .is_some();

    if !chat_exists {
        return Err(AppError::NotFound(format!("Chat with id {} not found", chat_id)));
    }

    let limit = query.limit.unwrap_or(50).min(100); // Максимум 100 сообщений
    let offset = query.offset.unwrap_or(0);

    let messages = client_models::wazzup_message::Entity::find()
        .filter(client_models::wazzup_message::Column::ChatId.eq(&chat_id))
        .order_by_asc(client_models::wazzup_message::Column::CreatedAt)
        .limit(limit as u64)
        .offset(offset as u64)
        .all(&client_db)
        .await?;

    let message_infos: Vec<MessageInfo> = messages
        .into_iter()
        .map(|m| MessageInfo {
            id: m.id,
            r#type: m.r#type,
            content: m.content,
            client_id: Some(client.id),
            created_at: m.created_at,
        })
        .collect();

    Ok(HttpResponse::Ok().json(message_infos))
}

// Функция для регистрации всех маршрутов этого модуля
pub fn init_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/chats")
            .service(get_chats)
            .service(get_chat_details)
            .service(get_chat_messages)
    );
}