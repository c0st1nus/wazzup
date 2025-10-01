use std::collections::{HashMap, HashSet};

use actix_web::{HttpRequest, HttpResponse, get, post, web};
use chrono::{DateTime, Utc};
use sea_orm::prelude::DateTimeUtc;
use sea_orm::{
    ColumnTrait, EntityTrait, FromQueryResult, JoinType, QueryFilter, QueryOrder, QuerySelect,
    RelationTrait, sea_query::Expr, sea_query::extension::postgres::PgExpr,
};
use serde_json::Value as JsonValue;
use uuid::Uuid;

use crate::{
    app_state::AppState,
    database::models::{channels, chats, clients, messages, users},
    errors::AppError,
    services::wazzup_api::SendMessageRequest,
};

use super::functions::{
    option_i8_to_bool,
    uuid_bytes_to_string, uuid_to_bytes,
};
use crate::api::helpers::get_company_api_key;
use super::structures::{
    AssigneeSummary, ChannelSummary, ChatDetails, ChatInfoSummary, ChatMessagesResponse,
    ChatPreview, ChatPreviewList, ChatPreviewsQuery, ClientSummary, MessageContentItem,
    MessageSender, MessageView, MessagesQuery, OutgoingMessage, SendChatMessageRequest,
    SendChatMessageResponse,
};

#[derive(FromQueryResult)]
struct LastMessageMeta {
    chat_id: String,
    last_created_at: Option<DateTimeUtc>,
}

#[derive(FromQueryResult)]
struct InboundCountMeta {
    chat_id: String,
    unread_count: i64,
}

#[utoipa::path(
    get,
    path = "/api/chats/{companyId}/previews",
    tag = "Chats",
    params(
        ("companyId" = String, Path, description = "Company UUID"),
        ("offset" = Option<u64>, Query, description = "Number of previews to skip"),
        ("count" = Option<u64>, Query, description = "Maximum number of previews to return"),
        ("filter" = Option<String>, Query, description = "Filter chats by name (case-insensitive substring)"),
        ("bot" = Option<bool>, Query, description = "If true return only bot-driven chats, false for human-driven"),
    ),
    responses(
        (status = 200, description = "Chat previews", body = ChatPreviewList),
        (status = 404, description = "Company not found"),
    )
)]
#[get("/{companyId}/previews")]
pub async fn get_chat_previews(
    app_state: web::Data<AppState>,
    path: web::Path<String>,
    query: web::Query<ChatPreviewsQuery>,
) -> Result<HttpResponse, AppError> {
    let company_uuid = Uuid::parse_str(&path.into_inner())
        .map_err(|_| AppError::InvalidInput("companyId must be a valid UUID".to_string()))?;
    let company_id_bytes = uuid_to_bytes(&company_uuid);

    let params = query.into_inner();

    let chat_records = load_company_chats(&company_id_bytes, &app_state, params.filter.clone()).await?;
    let chat_ids: Vec<String> = chat_records
        .iter()
        .map(|record| record.chat.id.clone())
        .collect();

    let last_message_map = load_last_message_metadata(&app_state, &chat_ids).await?;
    let unread_map = load_inbound_counts(&app_state, &chat_ids).await?;

    let mut user_ids: HashSet<Vec<u8>> = HashSet::new();
    for record in &chat_records {
        if let Some(client) = &record.client {
            user_ids.insert(client.responsible_user_id.clone());
        }
    }
    let user_map = load_users(&app_state, &user_ids).await?;

    let mut previews: Vec<(ChatPreview, Option<DateTimeUtc>)> = Vec::new();
    for record in &chat_records {
        let last_timestamp = last_message_map.get(record.chat.id.as_str()).cloned();
        let unread_count = unread_map
            .get(record.chat.id.as_str())
            .copied()
            .unwrap_or(0);

        let preview = build_chat_preview(record, unread_count, &user_map)?;

        if let Some(bot_only) = params.bot {
            if bot_only != assigned_to_bot(&preview) {
                continue;
            }
        }

        previews.push((preview, last_timestamp));
    }

    previews.sort_by(|a, b| match (&a.1, &b.1) {
        (Some(a_date), Some(b_date)) => b_date.cmp(a_date),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.0.chat_info.name.cmp(&b.0.chat_info.name),
    });

    let (offset, count) = normalize_pagination(params.offset, params.count)?;
    let sliced = slice_previews(previews, offset, count);

    let selected_ids: Vec<String> = sliced
        .iter()
        .map(|(preview, _)| preview.id.clone())
        .collect();

    let (last_messages, author_ids) = load_last_messages(&app_state, &selected_ids).await?;
    let author_map = load_users(&app_state, &author_ids).await?;

    let mut response_data = Vec::new();
    for ((mut preview, _), chat_id_str) in sliced.into_iter().zip(selected_ids.into_iter()) {
        if let Some(message) = last_messages.get(chat_id_str.as_str()) {
            let sender =
                resolve_message_sender(message, &author_map, &user_map, preview.client.as_ref());
            preview.last_message = Some(build_message_view(message, sender)?);
        }
        response_data.push(preview);
    }

    Ok(HttpResponse::Ok().json(ChatPreviewList {
        data: response_data,
    }))
}

#[utoipa::path(
    get,
    path = "/api/chats/{companyId}/{chatId}",
    tag = "Chats",
    params(
        ("companyId" = String, Path, description = "Company UUID"),
        ("chatId" = String, Path, description = "Chat identifier (UUID)")
    ),
    responses(
        (status = 200, description = "Chat details", body = ChatDetails),
        (status = 404, description = "Chat not found"),
    )
)]
#[get("/{companyId}/{chatId}")]
pub async fn get_chat(
    app_state: web::Data<AppState>,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, AppError> {
    let (company_id_raw, chat_id) = path.into_inner();
    let company_uuid = Uuid::parse_str(&company_id_raw)
        .map_err(|_| AppError::InvalidInput("companyId must be a valid UUID".to_string()))?;
    let company_id_bytes = uuid_to_bytes(&company_uuid);

    let chat_uuid = Uuid::parse_str(&chat_id)
        .map_err(|_| AppError::InvalidInput("Invalid chat id".to_string()))?;
    let record = load_single_chat(&company_id_bytes, &app_state, &chat_uuid).await?;

    let unread_map = load_inbound_counts(&app_state, &[record.chat.id.clone()]).await?;
    let unread_count = unread_map
        .get(record.chat.id.as_str())
        .copied()
        .unwrap_or(0);

    let mut user_ids = HashSet::new();
    if let Some(client) = &record.client {
        user_ids.insert(client.responsible_user_id.clone());
    }
    let user_map = load_users(&app_state, &user_ids).await?;

    let mut preview = build_chat_preview(&record, unread_count, &user_map)?;

    let (last_messages, author_ids) =
        load_last_messages(&app_state, &[record.chat.id.clone()]).await?;
    let author_map = load_users(&app_state, &author_ids).await?;

    if let Some(message) = last_messages.get(record.chat.id.as_str()) {
        let sender =
            resolve_message_sender(message, &author_map, &user_map, preview.client.as_ref());
        preview.last_message = Some(build_message_view(message, sender)?);
    }

    Ok(HttpResponse::Ok().json(preview))
}

#[utoipa::path(
    get,
    path = "/api/chats/{companyId}/{chatId}/messages",
    tag = "Chats",
    params(
        ("companyId" = String, Path, description = "Company UUID"),
        ("chatId" = String, Path, description = "Chat identifier (UUID)"),
        ("offset" = Option<u64>, Query, description = "Number of messages to skip"),
        ("count" = Option<u64>, Query, description = "Maximum number of messages to return"),
    ),
    responses(
        (status = 200, description = "Chat messages", body = ChatMessagesResponse),
        (status = 404, description = "Chat not found"),
    )
)]
#[get("/{companyId}/{chatId}/messages")]
pub async fn get_chat_messages(
    app_state: web::Data<AppState>,
    path: web::Path<(String, String)>,
    query: web::Query<MessagesQuery>,
) -> Result<HttpResponse, AppError> {
    let (company_id_raw, chat_id) = path.into_inner();
    let company_uuid = Uuid::parse_str(&company_id_raw)
        .map_err(|_| AppError::InvalidInput("companyId must be a valid UUID".to_string()))?;
    let company_id_bytes = uuid_to_bytes(&company_uuid);

    let chat_uuid = Uuid::parse_str(&chat_id)
        .map_err(|_| AppError::InvalidInput("Invalid chat id".to_string()))?;
    let record = load_single_chat(&company_id_bytes, &app_state, &chat_uuid).await?;

    let mut user_ids = HashSet::new();
    if let Some(client) = &record.client {
        user_ids.insert(client.responsible_user_id.clone());
    }
    let user_map = load_users(&app_state, &user_ids).await?;

    let preview = build_chat_preview(&record, 0, &user_map)?;

    let (offset, count) = normalize_pagination(query.offset, query.count)?;

    let chat_id_bytes = uuid_to_bytes(&chat_uuid);
    let mut message_query = messages::Entity::find()
        .filter(messages::Column::ChatId.eq(chat_id_bytes.clone()))
        .order_by_desc(messages::Column::CreatedAt);

    if let Some(limit) = count {
        message_query = message_query.limit(limit);
    }
    message_query = message_query.offset(offset);

    let message_models = message_query.all(&app_state.db).await?;

    let mut author_ids: HashSet<Vec<u8>> = HashSet::new();
    for message in &message_models {
        if let Some(author_id) = &message.author_user_id {
            author_ids.insert(author_id.clone());
        }
    }
    let author_map = load_users(&app_state, &author_ids).await?;

    let mut responses = Vec::new();
    for message in message_models {
        let sender =
            resolve_message_sender(&message, &author_map, &user_map, preview.client.as_ref());
        responses.push(build_message_view(&message, sender)?);
    }

    Ok(HttpResponse::Ok().json(ChatMessagesResponse { data: responses }))
}

#[utoipa::path(
    post,
    path = "/api/chats/{companyId}/{chatId}/send",
    tag = "Chats",
    params(
        ("companyId" = String, Path, description = "Company UUID"),
        ("chatId" = String, Path, description = "Chat identifier (UUID)")
    ),
    request_body = SendChatMessageRequest,
    responses(
        (status = 200, description = "Message sent", body = SendChatMessageResponse),
        (status = 404, description = "Chat not found"),
        (status = 422, description = "Invalid payload"),
    )
)]
#[post("/{companyId}/{chatId}/send")]
pub async fn send_chat_message(
    app_state: web::Data<AppState>,
    path: web::Path<(String, String)>,
    body: web::Json<SendChatMessageRequest>,
) -> Result<HttpResponse, AppError> {
    let (company_id_raw, chat_id) = path.into_inner();
    let company_uuid = Uuid::parse_str(&company_id_raw)
        .map_err(|_| AppError::InvalidInput("companyId must be a valid UUID".to_string()))?;
    let company_id_bytes = uuid_to_bytes(&company_uuid);

    let chat_uuid = Uuid::parse_str(&chat_id)
        .map_err(|_| AppError::InvalidInput("Invalid chat id".to_string()))?;
    let record = load_single_chat(&company_id_bytes, &app_state, &chat_uuid).await?;

    let payload = body.into_inner();
    if payload.message.content.is_empty() {
        return Err(AppError::InvalidInput(
            "Message content cannot be empty".to_string(),
        ));
    }

    let (text_content, media_url) = extract_outgoing_content(&payload.message)?;

    let channel = record
        .channel
        .as_ref()
        .ok_or_else(|| AppError::NotFound("Channel not found for chat".to_string()))?;

    let send_request = SendMessageRequest {
        chat_id: Some(chat_uuid.to_string()),
        channel_id: Some(uuid_bytes_to_string(&channel.id)?),
        chat_type: Some(channel.r#type.clone()),
        sender_id: 0, // Legacy placeholder, CRM-side sender stored separately
        text: text_content.clone(),
        content_uri: media_url.clone(),
        crm_user_id: None, // Internal service - no user context
        crm_message_id: None,
    };

    let api_key = get_company_api_key(&company_uuid, &app_state.db).await?;

    let response = app_state
        .wazzup_api
        .send_message(&api_key, &send_request)
        .await?;

    let message_id = response
        .message_id
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    Ok(HttpResponse::Ok().json(SendChatMessageResponse {
        id: message_id,
        created_at: Utc::now().to_rfc3339(),
    }))
}

pub fn init_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/chats")
            .service(get_chat_previews)
            .service(get_chat)
            .service(get_chat_messages)
            .service(send_chat_message),
    );
}

struct ChatRecord {
    chat: chats::Model,
    client: Option<clients::Model>,
    channel: Option<channels::Model>,
}

async fn load_company_chats(
    company_id_bytes: &[u8],
    app_state: &web::Data<AppState>,
    filter: Option<String>,
) -> Result<Vec<ChatRecord>, AppError> {
    use crate::database::models::{channel_settings, company_users};
    
    // Find all users in the company
    let company_user_records = company_users::Entity::find()
        .filter(company_users::Column::CompanyId.eq(company_id_bytes.to_vec()))
        .all(&app_state.db)
        .await?;
    
    let user_ids: Vec<Vec<u8>> = company_user_records.iter().map(|cu| cu.user_id.clone()).collect();
    
    if user_ids.is_empty() {
        return Ok(Vec::new());
    }
    
    // Find all channels for these users
    let channel_settings_records = channel_settings::Entity::find()
        .filter(channel_settings::Column::UserId.is_in(user_ids))
        .all(&app_state.db)
        .await?;
    
    let channel_ids: Vec<Vec<u8>> = channel_settings_records
        .iter()
        .map(|cs| cs.channel_id.clone())
        .collect();
    
    if channel_ids.is_empty() {
        return Ok(Vec::new());
    }
    
    // Load all chats for these channels
    let mut query = chats::Entity::find()
        .filter(chats::Column::ChannelId.is_in(channel_ids));

    if let Some(filter) = filter.filter(|value| !value.trim().is_empty()) {
        let pattern = format!("%{}%", filter.trim());
        query = query.filter(Expr::col(chats::Column::Name).ilike(pattern));
    }

    let chats_list = query
        .find_with_related(channels::Entity)
        .all(&app_state.db)
        .await?;

    // Load all clients for the company
    let company_clients = clients::Entity::find()
        .filter(clients::Column::CompanyId.eq(company_id_bytes.to_vec()))
        .all(&app_state.db)
        .await?;

    let client_map: HashMap<Vec<u8>, clients::Model> = company_clients
        .into_iter()
        .map(|c| (c.id.clone(), c))
        .collect();

    Ok(chats_list
        .into_iter()
        .map(|(chat, channels)| {
            let client = chat.client_id.as_ref().and_then(|id| client_map.get(id).cloned());
            let channel = channels.into_iter().next();
            ChatRecord {
                chat,
                client,
                channel,
            }
        })
        .collect())
}

async fn load_single_chat(
    company_id_bytes: &[u8],
    app_state: &web::Data<AppState>,
    chat_uuid: &Uuid,
) -> Result<ChatRecord, AppError> {
    let chat_id_str = chat_uuid.to_string();

    let chat = chats::Entity::find_by_id(chat_id_str.clone())
        .one(&app_state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Chat not found".to_string()))?;

    // Load client and verify it belongs to the company
    let client = if let Some(ref client_id) = chat.client_id {
        let client_opt = clients::Entity::find_by_id(client_id.clone())
            .one(&app_state.db)
            .await?;
        
        if let Some(ref client_model) = client_opt {
            if client_model.company_id.as_ref() != Some(&company_id_bytes.to_vec()) {
                return Err(AppError::NotFound("Chat not found".to_string()));
            }
        }
        client_opt
    } else {
        None
    };

    // Load channel
    let channel = channels::Entity::find_by_id(chat.channel_id.clone())
        .one(&app_state.db)
        .await?;

    Ok(ChatRecord {
        chat,
        client,
        channel,
    })
}

async fn load_last_message_metadata(
    app_state: &web::Data<AppState>,
    chat_ids: &[String],
) -> Result<HashMap<String, DateTimeUtc>, AppError> {
    if chat_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let metas = messages::Entity::find()
        .filter(messages::Column::ChatId.is_in(chat_ids.iter().cloned().collect::<Vec<_>>()))
        .select_only()
        .column(messages::Column::ChatId)
        .column_as(
            Expr::col(messages::Column::CreatedAt).max(),
            "last_created_at",
        )
        .group_by(messages::Column::ChatId)
        .into_model::<LastMessageMeta>()
        .all(&app_state.db)
        .await?;

    Ok(metas
        .into_iter()
        .filter_map(|meta| meta.last_created_at.map(|ts| (meta.chat_id, ts)))
        .collect())
}

async fn load_inbound_counts(
    app_state: &web::Data<AppState>,
    chat_ids: &[String],
) -> Result<HashMap<String, i64>, AppError> {
    if chat_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let counts = messages::Entity::find()
        .filter(messages::Column::ChatId.is_in(chat_ids.iter().cloned().collect::<Vec<_>>()))
        .filter(messages::Column::IsInbound.eq(1))
        .select_only()
        .column(messages::Column::ChatId)
        .column_as(Expr::col(messages::Column::Id).count(), "unread_count")
        .group_by(messages::Column::ChatId)
        .into_model::<InboundCountMeta>()
        .all(&app_state.db)
        .await?;

    Ok(counts
        .into_iter()
        .map(|meta| (meta.chat_id, meta.unread_count))
        .collect())
}

async fn load_users(
    app_state: &web::Data<AppState>,
    user_ids: &HashSet<Vec<u8>>,
) -> Result<HashMap<Vec<u8>, users::Model>, AppError> {
    if user_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let models = users::Entity::find()
        .filter(users::Column::Id.is_in(user_ids.iter().cloned().collect::<Vec<_>>()))
        .all(&app_state.db)
        .await?;

    Ok(models
        .into_iter()
        .map(|user| (user.id.clone(), user))
        .collect())
}

fn build_chat_preview(
    record: &ChatRecord,
    unread_count: i64,
    user_map: &HashMap<Vec<u8>, users::Model>,
) -> Result<ChatPreview, AppError> {
    let chat_id = record.chat.id.clone();
    let channel_summary = ChannelSummary {
        id: uuid_bytes_to_string(&record.chat.channel_id)?,
        transport: record
            .channel
            .as_ref()
            .map(|channel| channel.r#type.clone()),
    };

    let client_summary = if let Some(client) = &record.client {
        Some(ClientSummary {
            id: uuid_bytes_to_string(&client.id)?,
            name: client.full_name.clone(),
            image_url: None,
        })
    } else {
        None
    };

    let assignee_summary = build_assignee_summary(record.client.as_ref(), user_map)?;

    let chat_info = ChatInfoSummary {
        name: record.chat.name.clone(),
        image_url: None,
    };

    let preview = ChatPreview {
        id: chat_id,
        unread_count,
        channel: channel_summary,
        last_message: None,
        chat_info,
        client: client_summary,
        assignee: assignee_summary,
    };

    Ok(preview)
}

fn build_assignee_summary(
    client: Option<&clients::Model>,
    user_map: &HashMap<Vec<u8>, users::Model>,
) -> Result<Option<AssigneeSummary>, AppError> {
    if let Some(client) = client {
        if let Some(user) = user_map.get(&client.responsible_user_id) {
            let id_string = uuid_bytes_to_string(&user.id)?;
            let role = user.role.clone().unwrap_or_else(|| "employee".to_string());

            let summary = AssigneeSummary {
                id: id_string,
                name: user.name.clone().unwrap_or_else(|| "Unknown".to_string()),
                image_url: None,
                role: role.clone(),
            };

            return Ok(Some(summary));
        }
    }

    Ok(None)
}

fn assigned_to_bot(preview: &ChatPreview) -> bool {
    preview
        .assignee
        .as_ref()
        .map(|assignee| assignee.role.eq_ignore_ascii_case("bot"))
        .unwrap_or(false)
}

fn normalize_pagination(
    offset: Option<u64>,
    count: Option<u64>,
) -> Result<(u64, Option<u64>), AppError> {
    let offset = offset.unwrap_or(0);
    if let Some(count) = count {
        if count == 0 {
            return Err(AppError::InvalidInput(
                "count must be greater than zero".to_string(),
            ));
        }
    }
    Ok((offset, count))
}

fn slice_previews(
    previews: Vec<(ChatPreview, Option<DateTimeUtc>)>,
    offset: u64,
    count: Option<u64>,
) -> Vec<(ChatPreview, Option<DateTimeUtc>)> {
    let start = offset as usize;
    if start >= previews.len() {
        return Vec::new();
    }

    let end = count
        .map(|c| usize::min(start + c as usize, previews.len()))
        .unwrap_or(previews.len());
    previews.into_iter().skip(start).take(end - start).collect()
}

async fn load_last_messages(
    app_state: &web::Data<AppState>,
    chat_ids: &[String],
) -> Result<(HashMap<String, messages::Model>, HashSet<Vec<u8>>), AppError> {
    if chat_ids.is_empty() {
        return Ok((HashMap::new(), HashSet::new()));
    }

    let mut result = HashMap::new();
    let mut author_ids = HashSet::new();

    for chat_id in chat_ids {
        if let Some(message) = messages::Entity::find()
            .filter(messages::Column::ChatId.eq(chat_id.clone()))
            .order_by_desc(messages::Column::CreatedAt)
            .one(&app_state.db)
            .await?
        {
            if let Some(author) = &message.author_user_id {
                author_ids.insert(author.clone());
            }
            result.insert(chat_id.clone(), message);
        }
    }

    Ok((result, author_ids))
}

fn resolve_message_sender(
    message: &messages::Model,
    author_map: &HashMap<Vec<u8>, users::Model>,
    user_map: &HashMap<Vec<u8>, users::Model>,
    client: Option<&ClientSummary>,
) -> MessageSender {
    if let Some(author_bytes) = &message.author_user_id {
        if let Some(user) = author_map
            .get(author_bytes)
            .or_else(|| user_map.get(author_bytes))
        {
            return MessageSender {
                name: user.name.clone().unwrap_or_else(|| "Unknown".to_string()),
                image_url: None,
            };
        }
    }

    if let Some(client) = client {
        return MessageSender {
            name: client.name.clone(),
            image_url: client.image_url.clone(),
        };
    }

    MessageSender {
        name: "Unknown".to_string(),
        image_url: None,
    }
}

fn build_message_view(
    message: &messages::Model,
    sender: MessageSender,
) -> Result<MessageView, AppError> {
    let message_id = uuid_bytes_to_string(&message.id)?;
    let created_at: DateTime<Utc> = message.created_at.into();

    let content = parse_message_content(&message.content);
    let is_inbound = option_i8_to_bool(message.is_inbound).unwrap_or(false);

    Ok(MessageView {
        id: message_id,
        content,
        sender,
        is_inbound,
        created_at: created_at.to_rfc3339(),
    })
}

fn parse_message_content(value: &JsonValue) -> Vec<MessageContentItem> {
    match value {
        JsonValue::Array(items) => items
            .iter()
            .filter_map(|item| {
                let r#type = item.get("type")?.as_str()?.to_string();
                let content = item.get("content")?.as_str()?.to_string();
                Some(MessageContentItem { r#type, content })
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn extract_outgoing_content(
    message: &OutgoingMessage,
) -> Result<(Option<String>, Option<String>), AppError> {
    let mut text: Option<String> = None;
    let mut media: Option<String> = None;

    for item in &message.content {
        match item.r#type.as_str() {
            "text" => {
                let new_text = item.content.trim();
                if new_text.is_empty() {
                    continue;
                }
                match &mut text {
                    Some(existing) => {
                        existing.push_str("\n");
                        existing.push_str(new_text);
                    }
                    None => text = Some(new_text.to_string()),
                }
            }
            "image" => {
                if media.is_some() {
                    return Err(AppError::InvalidInput(
                        "Only a single image attachment is supported".to_string(),
                    ));
                }
                media = Some(item.content.clone());
            }
            other => {
                return Err(AppError::InvalidInput(format!(
                    "Unsupported message part type: {}",
                    other
                )));
            }
        }
    }

    if text.is_none() && media.is_none() {
        return Err(AppError::InvalidInput(
            "Message must contain at least text or an image".to_string(),
        ));
    }

    Ok((text, media))
}

fn uuid_to_vec(id: &str) -> Result<Vec<u8>, AppError> {
    let uuid = Uuid::parse_str(id)
        .map_err(|_| AppError::InvalidInput("Invalid UUID provided".to_string()))?;
    Ok(uuid_to_bytes(&uuid))
}
