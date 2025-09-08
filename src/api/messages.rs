use actix_web::{get, post, web, HttpResponse};
use sea_orm::{EntityTrait, ColumnTrait, QueryFilter, QueryOrder};
use sea_orm::prelude::DateTimeWithTimeZone;
use crate::{
    api::helpers,
    api::clients::transfer_responsibility,
    errors::AppError,
    services::wazzup_api::{self, SendMessageRequest},
    database::{client::{clients::{Entity as Client}, users}, types::MessageType},
    app_state::AppState,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

// Функция для определения типа сообщения из JSON контента
fn determine_message_type_from_content(content: &serde_json::Value) -> MessageType {
    if let Some(content_array) = content.as_array() {
        // Определяем тип по первому элементу массива
        if let Some(first_item) = content_array.first() {
            if let Some(item_type) = first_item.get("type").and_then(|t| t.as_str()) {
                return match item_type {
                    "missing_call" => MessageType::MissedCall,
                    "image" => MessageType::Image,
                    "video" => MessageType::Video,
                    "audio" => MessageType::Audio,
                    "document" | "file" => MessageType::Docs,
                    _ => MessageType::Text,
                };
            }
        }
    }
    // По умолчанию считаем текстовым
    MessageType::Text
}

// --- API Response Structures ---

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct MessageResponse {
    pub id: String,
    #[schema(inline)]
    pub message_type: MessageType,
    pub content: String,
    pub chat_id: String,
    pub client_id: Option<i64>, // Добавляем client_id
    #[schema(value_type = String, format = DateTime)]
    pub created_at: DateTimeWithTimeZone,
    pub is_inbound: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct MessageListResponse {
    pub messages: Vec<MessageResponse>,
    pub total: i64,
}

// --- Route Handlers ---

#[utoipa::path(
    post,
    path = "/api/messages/{companyId}/send",
    tag = "Messages",
    params(
        ("companyId" = i64, Path, description = "Company ID")
    ),
    request_body = wazzup_api::SendMessageRequest,
    responses(
        (status = 200, description = "Message sent successfully", body = wazzup_api::SendMessageResponse),
        (status = 404, description = "Company not found")
    )
)]
#[post("/{companyId}/send")]
async fn send_message(
    app_state: web::Data<AppState>,
    path: web::Path<i64>,
    body: web::Json<SendMessageRequest>,
) -> Result<HttpResponse, AppError> {
    let company_id = path.into_inner();
    let request = body.into_inner();
    if let Some(txt) = &request.text { if txt.len() > 4000 { return Err(AppError::InvalidInput("Message text too long".into())); } }
    
    // Получаем chat_id из запроса
    let chat_id = request.chat_id.as_ref()
        .ok_or_else(|| AppError::InvalidInput("chat_id is required".to_string()))?;
    
    // Получаем подключение к клиентской базе данных
    let client_db = helpers::get_client_db_connection(company_id, &app_state).await?;
    
    // Находим клиента по chat_id
    let client = Client::find()
        .filter(crate::database::client::clients::Column::WazzupChat.eq(chat_id))
        .one(&client_db)
        .await?
        .ok_or_else(|| AppError::NotFound("Client with this chat not found".to_string()))?;
    
    // Получаем информацию об отправителе
            let sender = users::Entity::find_by_id(request.sender_id)
        .one(&client_db)
        .await?
        .ok_or_else(|| AppError::NotFound("Sender not found".to_string()))?;
    
    // Проверяем права доступа
    if sender.role == "admin" {
        // Админ может писать кому угодно - пропускаем все проверки
    } else if let Some(responsible_user_id) = client.responsible_user_id {
        // Получаем информацию об ответственном
        let responsible = users::Entity::find_by_id(responsible_user_id)
            .one(&client_db)
            .await?
            .ok_or_else(|| AppError::NotFound("Responsible user not found".to_string()))?;
        
        match responsible.role.as_str() {
            "bot" => {
                // Если ответственный - бот, то может писать любой менеджер или админ
                if !matches!(sender.role.as_str(), "manager" | "admin") {
                    return Err(AppError::Forbidden("Only managers and admins can send messages when responsible is bot".to_string()));
                }
            },
            "manager" | "admin" => {
                // Если ответственный - менеджер или админ, то только он может писать
                if sender.id != responsible_user_id {
                    return Err(AppError::Forbidden("Only the responsible user can send messages".to_string()));
                }
            },
            _ => {
                return Err(AppError::Forbidden("Invalid responsible user role".to_string()));
            }
        }
    } else {
        // Если нет ответственного, то может писать любой менеджер или админ
        if !matches!(sender.role.as_str(), "manager" | "admin") {
            return Err(AppError::Forbidden("Only managers and admins can send messages".to_string()));
        }
    }
    
    // Проверяем, что отправитель не quality_control
    if sender.role == "quality_control" {
        return Err(AppError::Forbidden("Quality control users cannot send messages".to_string()));
    }
    
    // Выполняем перевод ответственности (если нужно)
    transfer_responsibility(
        &client_db,
        chat_id,
        client.responsible_user_id,
        request.sender_id,
        None, // message_id заполним позже если нужно
    ).await?;
    
    // Получаем информацию о чате и канале для определения chatType
    let chat_info = crate::database::client::wazzup_chats::Entity::find_by_id(chat_id)
        .find_also_related(crate::database::client::wazzup_channels::Entity)
        .one(&client_db)
        .await?
        .ok_or_else(|| AppError::NotFound("Chat not found".to_string()))?;
    
    let (chat, channel) = chat_info;
    let channel_info = channel.ok_or_else(|| AppError::NotFound("Channel not found".to_string()))?;
    
    // Создаем новый запрос с правильными параметрами для Wazzup API
    let wazzup_request = SendMessageRequest {
        chat_id: Some(chat_id.clone()),
        channel_id: Some(chat.channel_id),
        chat_type: Some(channel_info.r#type),
        sender_id: request.sender_id,
        text: request.text.clone(),
        content_uri: None, // TODO: добавить поддержку файлов позже
        crm_user_id: Some(request.sender_id.to_string()),
        crm_message_id: None, // TODO: добавить для идемпотентности
    };
    
    // Получаем API ключ и отправляем сообщение
    let api_key = helpers::get_company_api_key(company_id, &app_state.db).await?;
    let response = app_state.wazzup_api.send_message(&api_key, &wazzup_request).await?;

    Ok(HttpResponse::Ok().json(response))
}

#[utoipa::path(
    get,
    path = "/api/messages/{companyId}/chat/{chatId}",
    tag = "Messages",
    params(
        ("companyId" = i64, Path, description = "Company ID"),
        ("chatId" = String, Path, description = "Chat ID")
    ),
    responses(
        (status = 200, description = "List of messages in the chat", body = wazzup_api::MessageListResponse),
        (status = 404, description = "Company not found")
    )
)]
#[get("/{companyId}/chat/{chatId}")]
async fn get_messages(
    app_state: web::Data<AppState>,
    path: web::Path<(i64, String)>,
) -> Result<HttpResponse, AppError> {
    let (company_id, chat_id) = path.into_inner();
    let api_key = helpers::get_company_api_key(company_id, &app_state.db).await?;
    let response = app_state.wazzup_api.get_messages(&api_key, &chat_id).await?;

    Ok(HttpResponse::Ok().json(response))
}

#[utoipa::path(
    get,
    path = "/api/messages/{companyId}/unread-count",
    tag = "Messages",
    params(
        ("companyId" = i64, Path, description = "Company ID")
    ),
    responses(
        (status = 200, description = "Total unread message count", body = wazzup_api::UnreadCountResponse),
        (status = 404, description = "Company not found")
    )
)]
#[get("/{companyId}/unread-count")]
async fn get_unread_count(
    app_state: web::Data<AppState>,
    path: web::Path<i64>,
) -> Result<HttpResponse, AppError> {
    let company_id = path.into_inner();
    let api_key = helpers::get_company_api_key(company_id, &app_state.db).await?;
    let response = app_state.wazzup_api.get_unread_count(&api_key).await?;

    Ok(HttpResponse::Ok().json(response))
}

#[utoipa::path(
    get,
    path = "/api/messages/{companyId}/local/{chatId}",
    tag = "Messages",
    params(
        ("companyId" = i64, Path, description = "Company ID"),
        ("chatId" = String, Path, description = "Chat ID")
    ),
    responses(
        (status = 200, description = "List of messages from local database", body = MessageListResponse),
        (status = 404, description = "Company not found")
    )
)]
#[get("/{companyId}/local/{chatId}")]
async fn get_local_messages(
    app_state: web::Data<AppState>,
    path: web::Path<(i64, String)>,
) -> Result<HttpResponse, AppError> {
    use crate::database::client::wazzup_messages;
    
    let (company_id, chat_id) = path.into_inner();
    
    // Получаем подключение к клиентской базе данных
    let client_db = helpers::get_client_db_connection(company_id, &app_state).await?;
    
    // Находим клиента для данного чата
    let client = Client::find()
        .filter(crate::database::client::clients::Column::WazzupChat.eq(&chat_id))
        .one(&client_db)
        .await?;
    
    let client_id = client.map(|c| c.id);
    
    // Получаем сообщения из локальной базы данных
    let messages = wazzup_messages::Entity::find()
        .filter(wazzup_messages::Column::ChatId.eq(&chat_id))
        .order_by_asc(wazzup_messages::Column::CreatedAt)
        .all(&client_db)
        .await?;
    let total = messages.len() as i64; // избегаем второго запроса
    
    // Преобразуем в API ответ
    let message_responses: Vec<MessageResponse> = messages
        .into_iter()
        .map(|msg| {
            MessageResponse {
                id: msg.id,
                message_type: determine_message_type_from_content(&msg.content),
                content: serde_json::to_string(&msg.content).unwrap_or_else(|_| "[]".to_string()), // Конвертируем JSON в строку для API
                chat_id: msg.chat_id,
                client_id, // Добавляем client_id
                created_at: msg.created_at,
                is_inbound: msg.is_inbound,
            }
        })
        .collect();
    
    let response = MessageListResponse {
        messages: message_responses,
        total,
    };

    Ok(HttpResponse::Ok().json(response))
}


// Функция для регистрации всех маршрутов этого модуля
pub fn init_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/messages")
            .service(send_message)
            .service(get_messages)
            .service(get_unread_count)
            .service(get_local_messages)
    );
}