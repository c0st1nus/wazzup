use std::collections::HashSet;

use actix_web::{HttpRequest, web};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, IntoActiveModel, QueryFilter,
    Set,
};
use uuid::Uuid;

use crate::{
    app_state::AppState,
    database::models::{channel_settings, channels, companies, company_users},
    errors::AppError,
    services::wazzup_api::ChannelListResponse,
};

#[derive(Clone)]
pub struct AuthContext {
    pub company_uuid: Uuid,
    pub user_uuid: Uuid,
    pub company_id_bytes: Vec<u8>,
    pub user_id_bytes: Vec<u8>,
    pub api_key: String,
}

pub fn uuid_to_bytes(uuid: &Uuid) -> Vec<u8> {
    uuid.as_bytes().to_vec()
}

pub fn bytes_to_uuid(bytes: &[u8]) -> Option<Uuid> {
    Uuid::from_slice(bytes).ok()
}

fn parse_uuid_cookie(req: &HttpRequest, name: &str) -> Result<Uuid, AppError> {
    let cookie = req
        .cookie(name)
        .ok_or_else(|| AppError::Unauthorized(format!("Missing `{}` cookie", name)))?;

    Uuid::parse_str(cookie.value())
        .map_err(|_| AppError::Unauthorized(format!("Invalid `{}` cookie", name)))
}

pub async fn get_company_api_key_by_uuid(
    company_uuid: &Uuid,
    db: &DatabaseConnection,
) -> Result<String, AppError> {
    let company_id_bytes = uuid_to_bytes(company_uuid);
    let company = companies::Entity::find_by_id(company_id_bytes)
        .one(db)
        .await?
        .ok_or_else(|| AppError::Unauthorized("Company not found".to_string()))?;

    let api_key = company
        .wazzup_api_key
        .and_then(|key| {
            let trimmed = key.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .ok_or_else(|| {
            AppError::InvalidInput("Wazzup API key is not configured for the company".to_string())
        })?;

    Ok(api_key)
}

pub async fn resolve_admin_context(
    req: &HttpRequest,
    app_state: &web::Data<AppState>,
) -> Result<AuthContext, AppError> {
    let user_uuid = parse_uuid_cookie(req, "user_id")?;
    let company_uuid = parse_uuid_cookie(req, "company_id")?;

    let company_id_bytes = uuid_to_bytes(&company_uuid);
    let user_id_bytes = uuid_to_bytes(&user_uuid);

    let membership = company_users::Entity::find()
        .filter(company_users::Column::CompanyId.eq(company_id_bytes.clone()))
        .filter(company_users::Column::UserId.eq(user_id_bytes.clone()))
        .one(&app_state.db)
        .await?
        .ok_or_else(|| AppError::Unauthorized("User is not attached to the company".to_string()))?;

    let role_ok = membership
        .role
        .as_deref()
        .map(|role| role.eq_ignore_ascii_case("admin"))
        .unwrap_or(false);

    if !role_ok {
        return Err(AppError::Forbidden("Admin role required".to_string()));
    }

    let api_key = get_company_api_key_by_uuid(&company_uuid, &app_state.db).await?;

    Ok(AuthContext {
        company_uuid,
        user_uuid,
        company_id_bytes,
        user_id_bytes,
        api_key,
    })
}

pub async fn load_user_channel_access(
    user_uuid: &Uuid,
    db: &DatabaseConnection,
) -> Result<HashSet<Uuid>, AppError> {
    let user_bytes = uuid_to_bytes(user_uuid);
    let records = channel_settings::Entity::find()
        .filter(channel_settings::Column::UserId.eq(user_bytes))
        .all(db)
        .await?;

    let mut accessible_channels = HashSet::new();

    for record in records {
        if record.receives_messages != 0 {
            if let Some(channel_uuid) = bytes_to_uuid(&record.channel_id) {
                accessible_channels.insert(channel_uuid);
            }
        }
    }

    Ok(accessible_channels)
}

pub async fn sync_channels_to_db(
    channel_response: &ChannelListResponse,
    db: &DatabaseConnection,
) -> Result<(), AppError> {
    if let Some(channels_list) = &channel_response.channels {
        for channel_info in channels_list {
            let guid = match &channel_info.guid {
                Some(guid) => guid,
                None => continue,
            };

            let uuid = match Uuid::parse_str(guid) {
                Ok(uuid) => uuid,
                Err(err) => {
                    log::warn!("Skipping channel with invalid guid {}: {}", guid, err);
                    continue;
                }
            };

            let channel_id_bytes = uuid_to_bytes(&uuid);
            if let Some(existing) = channels::Entity::find_by_id(channel_id_bytes.clone())
                .one(db)
                .await?
            {
                if let Some(transport) = &channel_info.transport {
                    let mut active = existing.into_active_model();
                    active.r#type = Set(transport.clone());
                    active.update(db).await?;
                }
            } else {
                let new_channel = channels::ActiveModel {
                    id: Set(channel_id_bytes),
                    r#type: Set(channel_info
                        .transport
                        .clone()
                        .unwrap_or_else(|| "unknown".to_string())),
                };
                new_channel.insert(db).await?;
            }
        }
    }

    Ok(())
}
