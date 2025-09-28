use std::collections::HashMap;

use actix_web::{HttpResponse, delete, get, put, web};
use sea_orm::prelude::DateTimeUtc;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    api::{self, validation},
    app_state::AppState,
    database::models::clients,
    errors::AppError,
    services::wazzup_api::{self, WazzupContact, WazzupContactData},
};

use api::chats::functions::uuid_bytes_to_string;
use api::helpers::{get_company_api_key, uuid_to_bytes};

#[derive(Deserialize, Serialize, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpdateContactDto {
    pub full_name: String,
    pub email: String,
    pub phone: Option<String>,
    pub wazzup_chat: Option<String>,
}

#[derive(Deserialize, Serialize, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ContactWithWazzupData {
    pub id: String,
    pub full_name: String,
    pub email: String,
    pub phone: Option<String>,
    pub wazzup_chat: Option<String>,
    #[schema(value_type = String, format = DateTime)]
    pub created_at: DateTimeUtc,
    pub wazzup_contact: Option<WazzupContact>,
}

fn parse_uuid(value: &str, field: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(value)
        .map_err(|_| AppError::InvalidInput(format!("{} must be a valid UUID", field)))
}

fn wazzup_chat_from_contact(contact: &WazzupContact) -> Option<String> {
    contact
        .contact_data
        .iter()
        .find(|item| item.chat_type.eq_ignore_ascii_case("whatsapp"))
        .map(|item| item.chat_id.clone())
        .or_else(|| {
            contact
                .contact_data
                .first()
                .map(|item| item.chat_id.clone())
        })
}

fn build_contact_view(
    client: clients::Model,
    wazzup_contact: Option<WazzupContact>,
) -> Result<ContactWithWazzupData, AppError> {
    let id = uuid_bytes_to_string(&client.id)?;
    let wazzup_chat = wazzup_contact
        .as_ref()
        .and_then(|contact| wazzup_chat_from_contact(contact));

    Ok(ContactWithWazzupData {
        id,
        full_name: client.full_name,
        email: client.email,
        phone: client.phone,
        wazzup_chat,
        created_at: client.created_at,
        wazzup_contact,
    })
}

fn sanitize_phone_input(phone: Option<String>) -> Result<Option<String>, AppError> {
    match phone {
        Some(ref raw) if !raw.is_empty() => validation::sanitize_phone(raw)
            .ok_or_else(|| AppError::InvalidInput("Invalid phone".into()))
            .map(Some),
        Some(_) => Ok(None),
        None => Ok(None),
    }
}

fn local_client_to_wazzup_contact(
    client: &clients::Model,
    override_chat: Option<&str>,
    responsible_user_id: &str,
) -> Result<WazzupContact, AppError> {
    let id = uuid_bytes_to_string(&client.id)?;

    let mut contact_data = Vec::new();

    if let Some(phone) = &client.phone {
        let clean_phone: String = phone.chars().filter(|c| c.is_ascii_digit()).collect();
        if !clean_phone.is_empty() {
            contact_data.push(WazzupContactData {
                chat_type: "whatsapp".to_string(),
                chat_id: clean_phone,
                username: None,
                phone: None,
            });
        }
    }

    if let Some(chat_id) = override_chat.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }) {
        contact_data.push(WazzupContactData {
            chat_type: "custom".to_string(),
            chat_id,
            username: None,
            phone: None,
        });
    }

    Ok(WazzupContact {
        id,
        responsible_user_id: responsible_user_id.to_string(),
        name: client.full_name.clone(),
        contact_data,
        uri: None,
    })
}

async fn sync_client_to_wazzup(
    client: &clients::Model,
    override_chat: Option<&str>,
    api_key: &str,
    wazzup_api: &wazzup_api::WazzupApiService,
) -> Result<(), AppError> {
    let responsible_user_id = uuid_bytes_to_string(&client.responsible_user_id)?;
    let wazzup_contact =
        local_client_to_wazzup_contact(client, override_chat, &responsible_user_id)?;

    match wazzup_api
        .update_contact(api_key, &wazzup_contact.id, &wazzup_contact)
        .await
    {
        Ok(_) => Ok(()),
        Err(AppError::InvalidInput(msg)) if msg.contains("404") => {
            log::info!(
                "Contact {} not found in Wazzup, creating a new record",
                wazzup_contact.id
            );
            wazzup_api.create_contact(api_key, &wazzup_contact).await?;
            Ok(())
        }
        Err(err) => Err(err),
    }
}

#[utoipa::path(
    get,
    path = "/api/contacts/{companyId}",
    tag = "Contacts",
    params(("companyId" = String, Path, description = "Company UUID")),
    responses(
        (status = 200, description = "List of contacts from local database with Wazzup data", body = [ContactWithWazzupData]),
        (status = 404, description = "Company not found")
    )
)]
#[get("/{companyId}")]
async fn get_contacts(
    app_state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let company_uuid = parse_uuid(&path.into_inner(), "companyId")?;
    let company_id_bytes = uuid_to_bytes(&company_uuid);
    let api_key = get_company_api_key(&company_uuid, &app_state.db).await?;

    let clients = clients::Entity::find()
        .filter(clients::Column::CompanyId.eq(company_id_bytes))
        .all(&app_state.db)
        .await?;

    let wazzup_response = app_state
        .wazzup_api
        .get_contacts(&api_key)
        .await
        .unwrap_or_else(|err| {
            log::warn!("Failed to fetch contacts from Wazzup: {}", err);
            wazzup_api::WazzupContactListResponse {
                count: 0,
                data: vec![],
            }
        });

    let wazzup_map: HashMap<String, WazzupContact> = wazzup_response
        .data
        .into_iter()
        .map(|contact| (contact.id.clone(), contact))
        .collect();

    let mut contacts = Vec::new();
    for client in clients {
        let client_id = uuid_bytes_to_string(&client.id)?;
        let wazzup_contact = wazzup_map.get(&client_id).cloned();
        contacts.push(build_contact_view(client, wazzup_contact)?);
    }

    Ok(HttpResponse::Ok().json(contacts))
}

#[utoipa::path(
    get,
    path = "/api/contacts/{companyId}/{contactId}",
    tag = "Contacts",
    params(
        ("companyId" = String, Path, description = "Company UUID"),
        ("contactId" = String, Path, description = "Contact UUID")
    ),
    responses(
        (status = 200, description = "Contact details", body = ContactWithWazzupData),
        (status = 404, description = "Company or contact not found")
    )
)]
#[get("/{companyId}/{contactId}")]
async fn get_contact_by_id(
    app_state: web::Data<AppState>,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, AppError> {
    let (company_raw, contact_raw) = path.into_inner();
    let company_uuid = parse_uuid(&company_raw, "companyId")?;
    let contact_uuid = parse_uuid(&contact_raw, "contactId")?;
    let company_id_bytes = uuid_to_bytes(&company_uuid);
    let contact_id_bytes = uuid_to_bytes(&contact_uuid);
    let api_key = get_company_api_key(&company_uuid, &app_state.db).await?;

    let client = clients::Entity::find_by_id(contact_id_bytes.clone())
        .one(&app_state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Contact not found".to_string()))?;

    if client
        .company_id
        .as_ref()
        .map(|id| id != &company_id_bytes)
        .unwrap_or(true)
    {
        return Err(AppError::NotFound(
            "Contact does not belong to the company".to_string(),
        ));
    }

    let client_id = uuid_bytes_to_string(&client.id)?;
    let wazzup_contact = app_state
        .wazzup_api
        .get_contact(&api_key, &client_id)
        .await
        .ok();

    let contact_view = build_contact_view(client, wazzup_contact)?;

    Ok(HttpResponse::Ok().json(contact_view))
}

#[utoipa::path(
    put,
    path = "/api/contacts/{companyId}/{contactId}",
    tag = "Contacts",
    params(
        ("companyId" = String, Path, description = "Company UUID"),
        ("contactId" = String, Path, description = "Contact UUID")
    ),
    request_body = UpdateContactDto,
    responses(
        (status = 200, description = "Contact updated successfully", body = ContactWithWazzupData),
        (status = 404, description = "Company or contact not found")
    )
)]
#[put("/{companyId}/{contactId}")]
async fn update_contact(
    app_state: web::Data<AppState>,
    path: web::Path<(String, String)>,
    body: web::Json<UpdateContactDto>,
) -> Result<HttpResponse, AppError> {
    let (company_raw, contact_raw) = path.into_inner();
    let company_uuid = parse_uuid(&company_raw, "companyId")?;
    let contact_uuid = parse_uuid(&contact_raw, "contactId")?;
    let company_id_bytes = uuid_to_bytes(&company_uuid);
    let contact_id_bytes = uuid_to_bytes(&contact_uuid);

    let api_key = get_company_api_key(&company_uuid, &app_state.db).await?;

    let existing_client = clients::Entity::find_by_id(contact_id_bytes.clone())
        .one(&app_state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Contact not found".to_string()))?;

    if existing_client
        .company_id
        .as_ref()
        .map(|id| id != &company_id_bytes)
        .unwrap_or(true)
    {
        return Err(AppError::NotFound(
            "Contact does not belong to the company".to_string(),
        ));
    }

    let update_data = body.into_inner();

    if !validation::ensure_max_len(&update_data.full_name, 200) {
        return Err(AppError::InvalidInput("Full name too long".into()));
    }

    if !validation::validate_email_opt(&update_data.email) {
        return Err(AppError::InvalidInput("Invalid email format".into()));
    }

    let sanitized_phone = sanitize_phone_input(update_data.phone.clone())?;

    let email_exists = clients::Entity::find()
        .filter(clients::Column::CompanyId.eq(company_id_bytes.clone()))
        .filter(clients::Column::Email.eq(update_data.email.clone()))
        .filter(clients::Column::Id.ne(contact_id_bytes.clone()))
        .one(&app_state.db)
        .await?;

    if email_exists.is_some() {
        return Err(AppError::InvalidInput(format!(
            "Contact with email {} already exists",
            update_data.email
        )));
    }

    let mut active_client: clients::ActiveModel = existing_client.clone().into();
    active_client.full_name = Set(update_data.full_name.clone());
    active_client.email = Set(update_data.email.clone());
    active_client.phone = Set(sanitized_phone);

    let updated_client = active_client.update(&app_state.db).await?;

    if let Err(err) = sync_client_to_wazzup(
        &updated_client,
        update_data.wazzup_chat.as_deref(),
        &api_key,
        &app_state.wazzup_api,
    )
    .await
    {
        log::warn!("Failed to sync contact {} to Wazzup: {}", contact_uuid, err);
    }

    let client_id = uuid_bytes_to_string(&updated_client.id)?;
    let wazzup_contact = app_state
        .wazzup_api
        .get_contact(&api_key, &client_id)
        .await
        .ok();

    let contact_view = build_contact_view(updated_client, wazzup_contact)?;

    Ok(HttpResponse::Ok().json(contact_view))
}

#[utoipa::path(
    delete,
    path = "/api/contacts/{companyId}/{contactId}",
    tag = "Contacts",
    params(
        ("companyId" = String, Path, description = "Company UUID"),
        ("contactId" = String, Path, description = "Contact UUID")
    ),
    responses(
        (status = 204, description = "Contact deleted successfully"),
        (status = 404, description = "Company or contact not found")
    )
)]
#[delete("/{companyId}/{contactId}")]
async fn delete_contact(
    app_state: web::Data<AppState>,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, AppError> {
    let (company_raw, contact_raw) = path.into_inner();
    let company_uuid = parse_uuid(&company_raw, "companyId")?;
    let contact_uuid = parse_uuid(&contact_raw, "contactId")?;
    let company_id_bytes = uuid_to_bytes(&company_uuid);
    let contact_id_bytes = uuid_to_bytes(&contact_uuid);

    let api_key = get_company_api_key(&company_uuid, &app_state.db).await?;

    let client = clients::Entity::find_by_id(contact_id_bytes.clone())
        .one(&app_state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Contact not found".to_string()))?;

    if client
        .company_id
        .as_ref()
        .map(|id| id != &company_id_bytes)
        .unwrap_or(true)
    {
        return Err(AppError::NotFound(
            "Contact does not belong to the company".to_string(),
        ));
    }

    let client_id = uuid_bytes_to_string(&client.id)?;

    if let Err(err) = app_state
        .wazzup_api
        .delete_contact(&api_key, &client_id)
        .await
    {
        log::warn!(
            "Failed to delete contact {} from Wazzup: {}",
            contact_uuid,
            err
        );
    }

    clients::Entity::delete_by_id(contact_id_bytes)
        .exec(&app_state.db)
        .await?;

    Ok(HttpResponse::NoContent().finish())
}

pub fn init_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/contacts")
            .service(get_contacts)
            .service(get_contact_by_id)
            .service(update_contact)
            .service(delete_contact),
    );
}
