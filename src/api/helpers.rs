use sea_orm::{DatabaseConnection, EntityTrait};
use uuid::Uuid;

use crate::{database::models::companies, errors::AppError};

pub fn uuid_to_bytes(uuid: &Uuid) -> Vec<u8> {
    uuid.as_bytes().to_vec()
}

pub async fn get_company_api_key(
    company_uuid: &Uuid,
    db: &DatabaseConnection,
) -> Result<String, AppError> {
    let company_id_bytes = uuid_to_bytes(company_uuid);
    log::debug!(
        "Looking up company with UUID: {} (bytes: {:?})",
        company_uuid,
        company_id_bytes
    );

    let company = companies::Entity::find_by_id(company_id_bytes)
        .one(db)
        .await?
        .ok_or_else(|| {
            log::warn!("Company not found for UUID: {}", company_uuid);
            AppError::NotFound("Company not found".to_string())
        })?;

    log::debug!("Found company: {:?}", company.name);

    let api_key = company
        .wazzup_api_key
        .as_ref()
        .and_then(|key| {
            let trimmed = key.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .ok_or_else(|| AppError::InvalidInput("Company API key not configured".to_string()))?;

    Ok(api_key)
}
