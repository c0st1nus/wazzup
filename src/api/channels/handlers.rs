use actix_web::{delete, get, post, web, HttpRequest, HttpResponse};
use chrono::Utc;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde_json::json;
use url::form_urlencoded;
use uuid::Uuid;

use crate::{
	app_state::AppState,
	database::models::{channel_settings, channels},
	errors::AppError,
	services::wazzup_api::GenerateIframeLinkRequest,
};

use super::{
	functions::{
		get_company_api_key_by_uuid, load_user_channel_access, resolve_admin_context, sync_channels_to_db,
		uuid_to_bytes,
	},
	structures::{
		ChannelAddedNotification, ChannelDeletionResponse, ChannelView, ChannelsResponse, DeleteChannelQuery,
		WrappedIframeLinkResponse,
	},
	structures::default_delete_chats,
};

#[utoipa::path(
	get,
	path = "/api/channels",
	tag = "Channels",
	responses(
		(status = 200, description = "List of channels", body = ChannelsResponse),
		(status = 401, description = "Missing or invalid cookies"),
		(status = 403, description = "User lacks admin role"),
	)
)]
#[get("")]
pub async fn get_channels(
	app_state: web::Data<AppState>,
	req: HttpRequest,
) -> Result<HttpResponse, AppError> {
	let auth = resolve_admin_context(&req, &app_state).await?;
	let channels_response = app_state.wazzup_api.get_channels(&auth.api_key).await?;
	let accessible_channels = load_user_channel_access(&auth.user_uuid, &app_state.db).await?;

	let data = channels_response
		.channels
		.as_ref()
		.map(|list| {
			list.iter().map(|info| {
			let has_access_for_user = info
				.guid
				.as_ref()
				.and_then(|guid| Uuid::parse_str(guid).ok())
				.map(|uuid| accessible_channels.contains(&uuid))
				.unwrap_or(false);

			ChannelView {
				deleted: info.deleted,
					details: info.details.clone(),
					guid: info.guid.clone(),
					has_acecess: info.has_access || has_access_for_user,
					is_inbound: info.is_inbound,
					name: info.name.clone(),
					phone: info.phone.clone(),
					state: info.state.clone(),
					tier: info.tier.clone(),
					transport: info.transport.clone(),
					visible: info.visible,
			}
			})
			.collect::<Vec<_>>()
		})
		.unwrap_or_default();

	let response_body = ChannelsResponse { data };

	let db_clone = app_state.db.clone();
	let response_clone = channels_response.clone();
	actix_web::rt::spawn(async move {
		if let Err(err) = sync_channels_to_db(&response_clone, &db_clone).await {
			log::error!("Failed to sync channels: {}", err);
		}
	});

	Ok(HttpResponse::Ok().json(response_body))
}

#[utoipa::path(
	post,
	path = "/api/channels/iframe-link",
	tag = "Channels",
	request_body = GenerateIframeLinkRequest,
	responses(
		(status = 200, description = "Wrapped iframe link generated", body = WrappedIframeLinkResponse),
		(status = 401, description = "Missing or invalid cookies"),
		(status = 403, description = "User lacks admin role"),
	)
)]
#[post("/iframe-link")]
pub async fn generate_wrapped_iframe_link(
	app_state: web::Data<AppState>,
	req: HttpRequest,
	body: web::Json<GenerateIframeLinkRequest>,
) -> Result<HttpResponse, AppError> {
	let auth = resolve_admin_context(&req, &app_state).await?;
	let payload = body.into_inner();
	let original_response = app_state
		.wazzup_api
		.generate_channel_iframe_link(&auth.api_key, &payload)
		.await?;
	let original_link = original_response
		.link
		.ok_or_else(|| AppError::ExternalApiError("Wazzup did not return an iframe link".to_string()))?;

	let host = req
		.headers()
		.get("host")
		.and_then(|h| h.to_str().ok())
		.unwrap_or("");
	let scheme = req.connection_info().scheme().to_string();

	let encoded_original_link: String = form_urlencoded::byte_serialize(original_link.as_bytes()).collect();

	let wrapped_link = format!(
		"{}://{}/static/channel-setup.html?companyId={}&transport={}&originalLink={}",
		scheme,
		host,
		auth.company_uuid,
		payload.transport.as_deref().unwrap_or(""),
		encoded_original_link
	);

	Ok(HttpResponse::Ok().json(WrappedIframeLinkResponse { link: wrapped_link }))
}

#[utoipa::path(
	delete,
	path = "/api/channels/{channelId}",
	tag = "Channels",
	params(
		("channelId" = String, Path, description = "Channel GUID"),
		DeleteChannelQuery
	),
	responses(
		(status = 200, description = "Channel deleted successfully", body = ChannelDeletionResponse),
		(status = 401, description = "Missing or invalid cookies"),
		(status = 403, description = "User lacks admin role"),
		(status = 404, description = "Channel not found"),
	)
)]
#[delete("/{channelId}")]
pub async fn delete_channel(
	app_state: web::Data<AppState>,
	req: HttpRequest,
	path: web::Path<String>,
	query: Option<web::Query<DeleteChannelQuery>>,
) -> Result<HttpResponse, AppError> {
	let auth = resolve_admin_context(&req, &app_state).await?;
	let channel_id = path.into_inner();
	let channel_uuid = Uuid::parse_str(&channel_id)
		.map_err(|_| AppError::InvalidInput("channelId must be a valid UUID".to_string()))?;

	let delete_chats = query
		.map(|q| q.into_inner().delete_chats)
		.unwrap_or_else(default_delete_chats);

	let channel_bytes = uuid_to_bytes(&channel_uuid);
	let mut transport = channels::Entity::find_by_id(channel_bytes.clone())
		.one(&app_state.db)
		.await?
		.map(|model| model.r#type);

	if transport.is_none() {
	let response = app_state.wazzup_api.get_channels(&auth.api_key).await?;
		transport = response
			.channels
			.as_ref()
			.and_then(|list| {
				list.iter()
					.find(|item| item.guid.as_deref() == Some(channel_id.as_str()))
					.and_then(|item| item.transport.clone())
			});

		let db_clone = app_state.db.clone();
		let response_clone = response.clone();
		actix_web::rt::spawn(async move {
			if let Err(err) = sync_channels_to_db(&response_clone, &db_clone).await {
				log::error!("Failed to sync channels post-deletion lookup: {}", err);
			}
		});
	}

	let transport = transport.ok_or_else(|| AppError::NotFound("Channel transport not found".to_string()))?;

	app_state
		.wazzup_api
		.delete_channel(&auth.api_key, &transport, &channel_id, delete_chats)
		.await?;

	channel_settings::Entity::delete_many()
		.filter(channel_settings::Column::ChannelId.eq(channel_bytes.clone()))
		.exec(&app_state.db)
		.await?;
	channels::Entity::delete_by_id(channel_bytes)
		.exec(&app_state.db)
		.await?;

	Ok(HttpResponse::Ok().json(ChannelDeletionResponse {
		message: "Channel deleted successfully".to_string(),
		channel_id,
	}))
}

#[utoipa::path(
	post,
	path = "/api/channels/{companyId}/added/{transport}",
	tag = "Channels",
	params(
		("companyId" = String, Path, description = "Company UUID"),
		("transport" = String, Path, description = "Channel transport"),
	),
	request_body = ChannelAddedNotification,
	responses(
		(status = 200, description = "Notification processed"),
		(status = 404, description = "Company not found"),
	)
)]
#[post("/{companyId}/added/{transport}")]
pub async fn handle_channel_added(
	app_state: web::Data<AppState>,
	path: web::Path<(String, String)>,
	body: web::Json<ChannelAddedNotification>,
) -> Result<HttpResponse, AppError> {
	let (company_id_raw, transport) = path.into_inner();
	let company_uuid = Uuid::parse_str(&company_id_raw)
		.map_err(|_| AppError::InvalidInput("companyId must be a valid UUID".to_string()))?;

	let payload = body.into_inner();

	log::info!(
		"Channel added notification received for company {}, transport {}, channelId {}, state {}, timestamp {}",
		company_uuid,
		transport,
		payload.channel_id,
		payload.state,
		payload.timestamp
	);

	let api_key = get_company_api_key_by_uuid(&company_uuid, &app_state.db).await?;
	let api_clone = app_state.wazzup_api.clone();
	let db_clone = app_state.db.clone();

	actix_web::rt::spawn(async move {
		match api_clone.get_channels(&api_key).await {
			Ok(response) => {
				if let Err(err) = sync_channels_to_db(&response, &db_clone).await {
					log::error!("Failed to sync channels after 'added' notification: {}", err);
				}
			}
			Err(err) => {
				log::error!("Could not refresh channels after 'added' notification: {}", err);
			}
		}
	});

	Ok(HttpResponse::Ok().json(json!({
		"status": "success",
		"message": "Channel addition processed successfully",
		"channelId": payload.channel_id,
		"transport": transport,
		"timestamp": Utc::now()
	})))
}

#[utoipa::path(
	post,
	path = "/api/channels/{companyId}/{transport}/{channelId}/reinit",
	tag = "Channels",
	params(
		("companyId" = String, Path, description = "Company UUID"),
		("transport" = String, Path, description = "Channel transport"),
		("channelId" = String, Path, description = "Channel GUID"),
	),
	responses(
		(status = 200, description = "Channel reinitialization initiated"),
		(status = 404, description = "Company not found"),
	)
)]
#[post("/{companyId}/{transport}/{channelId}/reinit")]
pub async fn reinitialize_channel(
	app_state: web::Data<AppState>,
	path: web::Path<(String, String, String)>,
) -> Result<HttpResponse, AppError> {
	let (company_id_raw, transport, channel_id) = path.into_inner();
	let company_uuid = Uuid::parse_str(&company_id_raw)
		.map_err(|_| AppError::InvalidInput("companyId must be a valid UUID".to_string()))?;

	let api_key = get_company_api_key_by_uuid(&company_uuid, &app_state.db).await?;
	app_state
		.wazzup_api
		.reinitialize_channel(&api_key, &transport, &channel_id)
		.await?;

	Ok(HttpResponse::Ok().json(json!({
		"message": "Channel reinitialization initiated"
	})))
}

pub fn init_routes(cfg: &mut web::ServiceConfig) {
	cfg.service(
		web::scope("/channels")
			.service(get_channels)
			.service(generate_wrapped_iframe_link)
			.service(delete_channel)
			.service(handle_channel_added)
			.service(reinitialize_channel),
	);
}
