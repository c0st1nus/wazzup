use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, IntoActiveModel, QueryFilter,
    Set,
};
use uuid::Uuid;

use crate::{
    api::helpers::get_company_api_key,
    database::models::channels,
    errors::AppError,
    services::wazzup_api::ChannelListResponse,
};

pub use crate::api::context::bytes_to_uuid;
pub use crate::api::helpers::uuid_to_bytes;

pub async fn get_company_api_key_by_uuid(
    company_uuid: &Uuid,
    db: &DatabaseConnection,
) -> Result<String, AppError> {
    get_company_api_key(company_uuid, db).await
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
