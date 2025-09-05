use actix_web::{get, post, web, HttpResponse};
use sea_orm::{EntityTrait, ColumnTrait, QueryFilter};
use crate::{
    api::helpers,
    api::clients::transfer_responsibility,
    errors::AppError,
    services::wazzup_api::{self, SendMessageRequest},
    database::client::models::{Entity as Client, user, MessageType},
    AppState,
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
    pub created_at: chrono::DateTime<chrono::Utc>,
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
    
    // Получаем chat_id из запроса
    let chat_id = request.chat_id.as_ref()
        .ok_or_else(|| AppError::InvalidInput("chat_id is required".to_string()))?;
    
    // Находим клиента по chat_id
    let client = Client::find()
        .filter(crate::database::client::models::Column::WazzupChat.eq(chat_id))
        .one(&app_state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Client with this chat not found".to_string()))?;
    
    // Получаем информацию об отправителе
    let sender = user::Entity::find_by_id(request.sender_id)
        .one(&app_state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Sender not found".to_string()))?;
    
    // Проверяем права доступа
    if sender.role == "admin" {
        // Админ может писать кому угодно - пропускаем все проверки
    } else if let Some(responsible_user_id) = client.responsible_user_id {
        // Получаем информацию об ответственном
        let responsible = user::Entity::find_by_id(responsible_user_id)
            .one(&app_state.db)
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
    
    // Проверяем, что отправитель не quality_controll
    if sender.role == "quality_controll" {
        return Err(AppError::Forbidden("Quality control users cannot send messages".to_string()));
    }
    
    // Выполняем перевод ответственности (если нужно)
    transfer_responsibility(
        &app_state.db,
        chat_id,
        client.responsible_user_id,
        request.sender_id,
        None, // message_id заполним позже если нужно
    ).await?;
    
    // Получаем API ключ и отправляем сообщение
    let api_key = helpers::get_company_api_key(company_id, &app_state.db).await?;
    let wazzup_api = wazzup_api::WazzupApiService::new();
    let response = wazzup_api.send_message(&api_key, &request).await?;

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
    let wazzup_api = wazzup_api::WazzupApiService::new();

    let response = wazzup_api.get_messages(&api_key, &chat_id).await?;

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
    let wazzup_api = wazzup_api::WazzupApiService::new();

    let response = wazzup_api.get_unread_count(&api_key).await?;

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
    use sea_orm::{PaginatorTrait, QueryOrder};
    use crate::database::client::models::wazzup_message;
    
    let (company_id, chat_id) = path.into_inner();
    
    // Получаем подключение к клиентской базе данных
    let client_db = helpers::get_client_db_connection(company_id, &app_state).await?;
    
    // Находим клиента для данного чата
    let client = Client::find()
        .filter(crate::database::client::models::Column::WazzupChat.eq(&chat_id))
        .one(&client_db)
        .await?;
    
    let client_id = client.map(|c| c.id);
    
    // Получаем сообщения из локальной базы данных
    let messages = wazzup_message::Entity::find()
        .filter(wazzup_message::Column::ChatId.eq(&chat_id))
        .order_by_asc(wazzup_message::Column::CreatedAt)
        .all(&client_db)
        .await?;
    
    let total = wazzup_message::Entity::find()
        .filter(wazzup_message::Column::ChatId.eq(&chat_id))
        .count(&client_db)
        .await? as i64;
    
    // Преобразуем в API ответ
    let message_responses: Vec<MessageResponse> = messages
        .into_iter()
        .map(|msg| MessageResponse {
            id: msg.id,
            message_type: determine_message_type_from_content(&msg.content),
            content: serde_json::to_string(&msg.content).unwrap_or_else(|_| "[]".to_string()), // Конвертируем JSON в строку для API
            chat_id: msg.chat_id,
            client_id, // Добавляем client_id
            created_at: msg.created_at,
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