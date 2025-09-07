use actix_web::{delete, get, post, web, HttpRequest, HttpResponse};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, IntoActiveModel,
    QueryFilter, Set,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use url::form_urlencoded;

use crate::{
    database::{
        client::{wazzup_channels, wazzup_chats, wazzup_messages, wazzup_settings},
        main,
    },
    errors::AppError,
    services::wazzup_api::{self, ChannelListResponse, GenerateIframeLinkRequest},
    AppState,
};

// --- DTOs (Data Transfer Objects) ---
#[derive(Serialize, ToSchema, Clone)]
pub struct WrappedIframeLinkResponse {
    link: String,
}

#[derive(Deserialize, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ChannelAddedNotification {
    channel_id: String,
    state: String,
    timestamp: i64,
}

#[derive(Deserialize, IntoParams)]
struct DeleteChannelQuery {
    /// Whether to delete chats along with the channel. Default is true.
    #[serde(default = "default_delete_chats")]
    delete_chats: bool,
}

fn default_delete_chats() -> bool {
    true
}

// --- Helper Functions ---

/// Находит компанию и возвращает ее API ключ.
async fn get_company_api_key(
    company_id: i64,
    db: &DatabaseConnection,
) -> Result<String, AppError> {
    let company = main::companies::Entity::find_by_id(company_id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Company with id {} not found", company_id)))?;

    if company.wazzup_api_key.is_empty() {
        Err(AppError::InvalidInput(format!(
            "API key for company {} is not set",
            company_id
        )))
    } else {
        Ok(company.wazzup_api_key)
    }
}

/// Синхронизирует каналы из ответа API с локальной БД клиента.
async fn sync_channels_to_db(
    company_id: i64,
    channel_response: &ChannelListResponse,
    app_state: &web::Data<AppState>,
) -> Result<(), AppError> {
    // Получаем подключение к БД клиента используя pool manager
    let client_db = crate::api::helpers::get_client_db_connection(company_id, app_state).await?;

    if let Some(channels) = &channel_response.channels {
        for channel_info in channels {
            if let Some(guid) = &channel_info.guid {
                let existing_channel = wazzup_channels::Entity::find_by_id(guid.clone())
                    .one(&client_db)
                    .await?;

                if existing_channel.is_none() {
                    let new_channel = wazzup_channels::ActiveModel {
                        id: Set(guid.clone()),
                        r#type: Set(channel_info.transport.clone().unwrap_or_else(|| "unknown".to_string())),
                    };
                    new_channel.insert(&client_db).await?;
                }
            }
        }
        log::info!("Synced {} channels for company {}", channels.len(), company_id);
    }
    Ok(())
}

// --- Route Handlers ---

#[utoipa::path(
    get,
    path = "/api/channels/{companyId}",
    tag = "Channels",
    params(
        ("companyId" = i64, Path, description = "Company ID")
    ),
    responses(
        (status = 200, description = "List of channels", body = wazzup_api::ChannelListResponse),
        (status = 404, description = "Company not found")
    )
)]
#[get("/{companyId}")]
async fn get_channels(
    app_state: web::Data<AppState>,
    path: web::Path<i64>,
) -> Result<HttpResponse, AppError> {
    let company_id = path.into_inner();
    let api_key = get_company_api_key(company_id, &app_state.db).await?;
    let wazzup_api = wazzup_api::WazzupApiService::new();

    let channels_response = wazzup_api.get_channels(&api_key).await?;

    let state_clone = app_state.clone();
    let response_clone = channels_response.clone();
    actix_web::rt::spawn(async move {
        if let Err(e) = sync_channels_to_db(company_id, &response_clone, &state_clone).await {
            log::error!("Failed to sync channels for company {}: {}", company_id, e);
        }
    });

    Ok(HttpResponse::Ok().json(channels_response))
}


#[utoipa::path(
    delete,
    path = "/api/channels/{companyId}/{transport}/{channelId}",
    tag = "Channels",
    params(
        ("companyId" = i64, Path, description = "Company ID"),
        ("transport" = String, Path, description = "Channel transport"),
        ("channelId" = String, Path, description = "Channel ID"),
        DeleteChannelQuery
    ),
    responses(
        (status = 200, description = "Channel deleted successfully"),
        (status = 404, description = "Company not found")
    )
)]
#[delete("/{companyId}/{transport}/{channelId}")]
async fn delete_channel(
    app_state: web::Data<AppState>,
    path: web::Path<(i64, String, String)>,
    query: web::Query<DeleteChannelQuery>,
) -> Result<HttpResponse, AppError> {
    let (company_id, transport, channel_id) = path.into_inner();
    log::info!("Delete channel request: company_id={}, transport={}, channel_id={}, delete_chats={}", 
               company_id, transport, channel_id, query.delete_chats);
    
    let api_key = get_company_api_key(company_id, &app_state.db).await?;
    let wazzup_api = wazzup_api::WazzupApiService::new();
    
    wazzup_api.delete_channel(&api_key, &transport, &channel_id, query.delete_chats).await?;

    // Получаем подключение к БД клиента используя pool manager
    let client_db = crate::api::helpers::get_client_db_connection(company_id, &app_state).await?;

    let channel_to_delete = wazzup_channels::Entity::find_by_id(channel_id.clone()).one(&client_db).await?;

    if let Some(channel) = channel_to_delete {
        if query.delete_chats {
            let chats = wazzup_chats::Entity::find().filter(wazzup_chats::Column::ChannelId.eq(channel_id.clone())).all(&client_db).await?;
            for chat in chats {
                wazzup_messages::Entity::delete_many().filter(wazzup_messages::Column::ChatId.eq(chat.id.clone())).exec(&client_db).await?;
                chat.into_active_model().delete(&client_db).await?;
            }
        }
        wazzup_settings::Entity::delete_many().filter(wazzup_settings::Column::WazzupChannelId.eq(channel_id.clone())).exec(&client_db).await?;
        channel.into_active_model().delete(&client_db).await?;
    }
    
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "message": "Channel deleted successfully",
        "channelId": channel_id
    })))
}


#[utoipa::path(
    post,
    path = "/api/channels/{companyId}/iframe-link",
    tag = "Channels",
    params(
        ("companyId" = i64, Path, description = "Company ID")
    ),
    request_body = GenerateIframeLinkRequest,
    responses(
        (status = 200, description = "Wrapped iframe link generated", body = WrappedIframeLinkResponse),
        (status = 404, description = "Company not found")
    )
)]
#[post("/{companyId}/iframe-link")]
async fn generate_wrapped_iframe_link(
    app_state: web::Data<AppState>,
    req: HttpRequest, 
    path: web::Path<i64>,
    body: web::Json<GenerateIframeLinkRequest>,
) -> Result<HttpResponse, AppError> {
    let company_id = path.into_inner();
    let api_key = get_company_api_key(company_id, &app_state.db).await?;
    let wazzup_api = wazzup_api::WazzupApiService::new();

    let original_response = wazzup_api.generate_channel_iframe_link(&api_key, &body).await?;
    let original_link = original_response.link.ok_or_else(|| AppError::Internal)?;

    let host = req.headers().get("host").and_then(|h| h.to_str().ok()).unwrap_or("");
    let scheme = req.connection_info().scheme().to_string();
    
    let encoded_original_link: String = form_urlencoded::byte_serialize(original_link.as_bytes()).collect();

    let wrapped_link = format!(
        "{}://{}/static/channel-setup.html?companyId={}&transport={}&originalLink={}",
        scheme,
        host,
        company_id,
        body.transport.as_deref().unwrap_or(""),
        encoded_original_link
    );

    Ok(HttpResponse::Ok().json(WrappedIframeLinkResponse { link: wrapped_link }))
}


#[utoipa::path(
    post,
    path = "/api/channels/{companyId}/added/{transport}",
    tag = "Channels",
    params(
        ("companyId" = i64, Path, description = "Company ID"),
        ("transport" = String, Path, description = "Channel transport")
    ),
    request_body = ChannelAddedNotification,
    responses(
        (status = 200, description = "Notification processed")
    )
)]
#[post("/{companyId}/added/{transport}")]
async fn handle_channel_added(
    app_state: web::Data<AppState>,
    path: web::Path<(i64, String)>,
    body: web::Json<ChannelAddedNotification>,
) -> Result<HttpResponse, AppError> {
    let (company_id, transport) = path.into_inner();
    log::info!(
        "Channel added notification received for company {}, transport {}, channelId {}, state {}, timestamp {}",
        company_id, transport, body.channel_id, body.state, body.timestamp
    );

    let state_clone = app_state.clone();
    actix_web::rt::spawn(async move {
        match get_company_api_key(company_id, &state_clone.db).await {
            Ok(api_key) => {
                let wazzup_api = wazzup_api::WazzupApiService::new();
                if let Ok(response) = wazzup_api.get_channels(&api_key).await {
                    if let Err(e) = sync_channels_to_db(company_id, &response, &state_clone).await {
                         log::error!("Failed to sync channels after 'added' notification: {}", e);
                    }
                }
            }
            Err(e) => log::error!("Could not get api key for sync after 'added' notification: {}", e),
        }
    });

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "success",
        "message": "Channel addition processed successfully",
        "channelId": body.channel_id,
        "transport": transport,
        "timestamp": chrono::Utc::now()
    })))
}

// --- НОВЫЙ ЭНДПОИНТ ---
#[utoipa::path(
    post,
    path = "/api/channels/{companyId}/{transport}/{channelId}/reinit",
    tag = "Channels",
    params(
        ("companyId" = i64, Path, description = "Company ID"),
        ("transport" = String, Path, description = "Channel transport"),
        ("channelId" = String, Path, description = "Channel ID")
    ),
    responses(
        (status = 200, description = "Channel reinitialization initiated"),
        (status = 404, description = "Company not found")
    )
)]
#[post("/{companyId}/{transport}/{channelId}/reinit")]
async fn reinitialize_channel(
    app_state: web::Data<AppState>,
    path: web::Path<(i64, String, String)>,
) -> Result<HttpResponse, AppError> {
    let (company_id, transport, channel_id) = path.into_inner();
    let api_key = get_company_api_key(company_id, &app_state.db).await?;
    let wazzup_api = wazzup_api::WazzupApiService::new();

    wazzup_api.reinitialize_channel(&api_key, &transport, &channel_id).await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "message": "Channel reinitialization initiated"
    })))
}


// Функция для регистрации всех маршрутов этого модуля
pub fn init_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/channels")
            .service(get_channels)
            .service(delete_channel)
            .service(generate_wrapped_iframe_link)
            .service(handle_channel_added)
            .service(reinitialize_channel),
    );
}
