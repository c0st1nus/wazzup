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
    pub last_message: Option<MessageInfo>,
    pub unread_count: i64,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct MessageInfo {
    pub id: String,
    pub r#type: String,
    pub content: String,
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
        created_at: m.created_at,
    }))
}

// --- Route Handlers ---

#[utoipa::path(
    get,
    path = "/api/chats",
    tag = "Chats",
    responses(
        (status = 200, description = "List of chats with basic information", body = ChatListResponse)
    )
)]
#[get("/")]
async fn get_chats(
    app_state: web::Data<AppState>,
) -> Result<HttpResponse, AppError> {
    let chats = client_models::wazzup_chat::Entity::find()
        .find_also_related(client_models::wazzup_channel::Entity)
        .all(&app_state.db)
        .await?;

    let mut chat_responses = Vec::new();

    for (chat, channel) in chats {
        let last_message = get_last_message_for_chat(&chat.id, &app_state.db).await?;
        let unread_count = get_unread_count_for_chat(&chat.id, &app_state.db).await?;

        chat_responses.push(ChatResponse {
            id: chat.id,
            channel_id: chat.channel_id,
            channel_type: channel.map(|c| c.r#type),
            last_message,
            unread_count,
        });
    }

    let total = chat_responses.len();

    Ok(HttpResponse::Ok().json(ChatListResponse {
        chats: chat_responses,
        total,
    }))
}

#[utoipa::path(
    get,
    path = "/api/chats/{chatId}",
    tag = "Chats",
    params(
        ("chatId" = String, Path, description = "Chat ID")
    ),
    responses(
        (status = 200, description = "Chat details with all messages", body = ChatDetailsResponse),
        (status = 404, description = "Chat not found")
    )
)]
#[get("/{chatId}")]
async fn get_chat_details(
    app_state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let chat_id = path.into_inner();

    // Проверяем существование чата
    let chat = client_models::wazzup_chat::Entity::find_by_id(&chat_id)
        .find_also_related(client_models::wazzup_channel::Entity)
        .one(&app_state.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Chat with id {} not found", chat_id)))?;

    // Получаем все сообщения чата
    let messages = client_models::wazzup_message::Entity::find()
        .filter(client_models::wazzup_message::Column::ChatId.eq(&chat_id))
        .order_by_asc(client_models::wazzup_message::Column::CreatedAt)
        .all(&app_state.db)
        .await?;

    let message_infos: Vec<MessageInfo> = messages
        .into_iter()
        .map(|m| MessageInfo {
            id: m.id,
            r#type: m.r#type,
            content: m.content,
            created_at: m.created_at,
        })
        .collect();

    let messages_count = message_infos.len() as i64;

    let (chat, channel) = chat;

    Ok(HttpResponse::Ok().json(ChatDetailsResponse {
        id: chat.id,
        channel_id: chat.channel_id,
        channel_type: channel.map(|c| c.r#type),
        messages: message_infos,
        messages_count,
    }))
}

#[utoipa::path(
    get,
    path = "/api/chats/{chatId}/messages",
    tag = "Chats",
    params(
        ("chatId" = String, Path, description = "Chat ID"),
        ("limit" = Option<i64>, Query, description = "Maximum number of messages to return"),
        ("offset" = Option<i64>, Query, description = "Number of messages to skip")
    ),
    responses(
        (status = 200, description = "Messages from the chat", body = Vec<MessageInfo>),
        (status = 404, description = "Chat not found")
    )
)]
#[get("/{chatId}/messages")]
async fn get_chat_messages(
    app_state: web::Data<AppState>,
    path: web::Path<String>,
    query: web::Query<PaginationQuery>,
) -> Result<HttpResponse, AppError> {
    let chat_id = path.into_inner();

    // Проверяем существование чата
    let chat_exists = client_models::wazzup_chat::Entity::find_by_id(&chat_id)
        .one(&app_state.db)
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
        .all(&app_state.db)
        .await?;

    let message_infos: Vec<MessageInfo> = messages
        .into_iter()
        .map(|m| MessageInfo {
            id: m.id,
            r#type: m.r#type,
            content: m.content,
            created_at: m.created_at,
        })
        .collect();

    Ok(HttpResponse::Ok().json(message_infos))
}

#[derive(Deserialize, ToSchema)]
struct PaginationQuery {
    limit: Option<i64>,
    offset: Option<i64>,
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