use actix_web::{delete, get, put, web, HttpResponse};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set,
};
use sea_orm::prelude::DateTimeWithTimeZone;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    api::helpers,
    database::{
        client,
    },
    errors::AppError,
    services::wazzup_api::{self, WazzupContact, WazzupContactData},
    app_state::AppState,
    api::validation,
};

// --- DTOs (Data Transfer Objects) ---

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
    pub id: i64,
    pub full_name: String,
    pub email: String,
    pub phone: Option<String>,
    pub wazzup_chat: Option<String>,
    #[schema(value_type = String, format = DateTime)]
    pub created_at: DateTimeWithTimeZone,
    pub wazzup_contact: Option<WazzupContact>,
}

// --- Helper Functions ---

/// Получает подключение к базе данных клиента по ID компании.
async fn get_client_db_conn(
    company_id: i64,
    app_state: &web::Data<AppState>,
) -> Result<DatabaseConnection, AppError> {
    helpers::get_client_db_connection(company_id, app_state).await
}

/// Преобразует клиента из локальной БД в Wazzup контакт
fn local_client_to_wazzup_contact(
    client: &client::clients::Model,
    responsible_user_id: &str,
) -> WazzupContact {
    let mut contact_data = Vec::new();
    
    // Добавляем WhatsApp контакт если есть телефон
    if let Some(phone) = &client.phone {
        // Очищаем телефон от всех символов кроме цифр
        let clean_phone = phone.chars().filter(|c| c.is_ascii_digit()).collect::<String>();
        if !clean_phone.is_empty() {
            contact_data.push(WazzupContactData {
                chat_type: "whatsapp".to_string(),
                chat_id: clean_phone,
                username: None,
                phone: None,
            });
        }
    }
    
    WazzupContact {
        id: client.id.to_string(),
        responsible_user_id: responsible_user_id.to_string(),
        name: client.full_name.clone(),
        contact_data,
        uri: None, // TODO: можно добавить ссылку на CRM
    }
}

/// Обновляет клиента в Wazzup на основе локальных данных
async fn sync_client_to_wazzup(
    client: &client::clients::Model,
    api_key: &str,
    responsible_user_id: &str,
    wazzup_api: &wazzup_api::WazzupApiService,
) -> Result<(), AppError> {
    let wazzup_contact = local_client_to_wazzup_contact(client, responsible_user_id);
    
    // Пробуем обновить контакт в Wazzup
    match wazzup_api.update_contact(api_key, &client.id.to_string(), &wazzup_contact).await {
        Ok(_) => Ok(()),
        Err(AppError::InvalidInput(msg)) if msg.contains("404") => {
            // Если контакт не найден в Wazzup, создаем его
            log::info!("Contact {} not found in Wazzup, creating new one", client.id);
            wazzup_api.create_contact(api_key, &wazzup_contact).await?;
            Ok(())
        }
        Err(e) => Err(e),
    }
}

// --- Route Handlers ---

#[utoipa::path(
    get,
    path = "/api/contacts/{companyId}",
    tag = "Contacts",
    params(
        ("companyId" = i64, Path, description = "Company ID")
    ),
    responses(
        (status = 200, description = "List of contacts from local database with Wazzup data", body = [ContactWithWazzupData]),
        (status = 404, description = "Company not found")
    )
)]
#[get("/{companyId}")]
async fn get_contacts(
    app_state: web::Data<AppState>,
    path: web::Path<i64>,
) -> Result<HttpResponse, AppError> {
    let company_id = path.into_inner();
    let client_db = get_client_db_conn(company_id, &app_state).await?;
    let api_key = helpers::get_company_api_key(company_id, &app_state.db).await?;
    let wazzup_api = wazzup_api::WazzupApiService::new();

    // Получаем клиентов из локальной БД
    let local_clients = client::clients::Entity::find().all(&client_db).await?;
    
    // Получаем контакты из Wazzup
    let wazzup_response = wazzup_api.get_contacts(&api_key).await.unwrap_or_else(|e| {
        log::warn!("Failed to get contacts from Wazzup: {}", e);
        wazzup_api::WazzupContactListResponse {
            count: 0,
            data: vec![],
        }
    });
    
    // Создаем хэш-карту для быстрого поиска Wazzup контактов по ID
    let wazzup_contacts_map: std::collections::HashMap<String, WazzupContact> = wazzup_response
        .data
        .into_iter()
        .map(|contact| (contact.id.clone(), contact))
        .collect();
    
    // Объединяем данные
    let contacts_with_wazzup: Vec<ContactWithWazzupData> = local_clients
        .into_iter()
        .map(|client| {
            let wazzup_contact = wazzup_contacts_map.get(&client.id.to_string()).cloned();
            ContactWithWazzupData {
                id: client.id,
                full_name: client.full_name,
                email: client.email,
                phone: client.phone,
                wazzup_chat: client.wazzup_chat,
                created_at: client.created_at,
                wazzup_contact,
            }
        })
        .collect();

    Ok(HttpResponse::Ok().json(contacts_with_wazzup))
}

#[utoipa::path(
    get,
    path = "/api/contacts/{companyId}/{contactId}",
    tag = "Contacts",
    params(
        ("companyId" = i64, Path, description = "Company ID"),
        ("contactId" = i64, Path, description = "Contact ID")
    ),
    responses(
        (status = 200, description = "Contact details from local database with Wazzup data", body = ContactWithWazzupData),
        (status = 404, description = "Company or contact not found")
    )
)]
#[get("/{companyId}/{contactId}")]
async fn get_contact_by_id(
    app_state: web::Data<AppState>,
    path: web::Path<(i64, i64)>,
) -> Result<HttpResponse, AppError> {
    let (company_id, contact_id) = path.into_inner();
    let client_db = get_client_db_conn(company_id, &app_state).await?;
    let api_key = helpers::get_company_api_key(company_id, &app_state.db).await?;
    let wazzup_api = wazzup_api::WazzupApiService::new();

    // Получаем клиента из локальной БД
    let local_client = client::clients::Entity::find_by_id(contact_id)
        .one(&client_db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Contact {} not found", contact_id)))?;
    
    // Пробуем получить контакт из Wazzup
    let wazzup_contact = wazzup_api
        .get_contact(&api_key, &contact_id.to_string())
        .await
        .ok();
    
    let contact_with_wazzup = ContactWithWazzupData {
        id: local_client.id,
        full_name: local_client.full_name,
        email: local_client.email,
        phone: local_client.phone,
        wazzup_chat: local_client.wazzup_chat,
        created_at: local_client.created_at,
        wazzup_contact,
    };

    Ok(HttpResponse::Ok().json(contact_with_wazzup))
}
#[utoipa::path(
    put,
    path = "/api/contacts/{companyId}/{contactId}",
    tag = "Contacts",
    params(
        ("companyId" = i64, Path, description = "Company ID"),
        ("contactId" = i64, Path, description = "Contact ID")
    ),
    request_body = UpdateContactDto,
    responses(
        (status = 200, description = "Contact updated successfully in both local database and Wazzup", body = ContactWithWazzupData),
        (status = 404, description = "Company or contact not found")
    )
)]
#[put("/{companyId}/{contactId}")]
async fn update_contact(
    app_state: web::Data<AppState>,
    path: web::Path<(i64, i64)>,
    body: web::Json<UpdateContactDto>,
) -> Result<HttpResponse, AppError> {
    let (company_id, contact_id) = path.into_inner();
    let client_db = get_client_db_conn(company_id, &app_state).await?;
    let api_key = helpers::get_company_api_key(company_id, &app_state.db).await?;
    let wazzup_api = wazzup_api::WazzupApiService::new();
    let update_data = body.into_inner();

    // Находим существующий контакт
    let existing_client = client::clients::Entity::find_by_id(contact_id)
        .one(&client_db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Contact {} not found", contact_id)))?;

    // Проверяем уникальность email, если он изменился
    if existing_client.email != update_data.email {
        let email_exists = client::clients::Entity::find()
            .filter(client::clients::Column::Email.eq(&update_data.email))
            .filter(client::clients::Column::Id.ne(contact_id))
            .one(&client_db)
            .await?;

        if email_exists.is_some() {
            return Err(AppError::InvalidInput(format!(
                "Contact with email {} already exists",
                update_data.email
            )));
        }
    }

    // Обновляем клиента в локальной БД
    let mut active_client: client::clients::ActiveModel = existing_client.clone().into();
    if !validation::ensure_max_len(&update_data.full_name, 200) { return Err(AppError::InvalidInput("Full name too long".into())); }
    if !validation::validate_email_opt(&update_data.email) { return Err(AppError::InvalidInput("Invalid email format".into())); }
    let sanitized_phone = match update_data.phone {
        Some(ref p) => validation::sanitize_phone(p).ok_or_else(|| AppError::InvalidInput("Invalid phone".into()))?,
        None => String::new(),
    };
    active_client.full_name = Set(update_data.full_name);
    active_client.email = Set(update_data.email);
    active_client.phone = if sanitized_phone.is_empty() { Set(None) } else { Set(Some(sanitized_phone)) };
    active_client.wazzup_chat = Set(update_data.wazzup_chat);

    let updated_client = active_client.update(&client_db).await?;

    // Синхронизируем с Wazzup (используем "1" как default responsible_user_id)
    if let Err(e) = sync_client_to_wazzup(&updated_client, &api_key, "1", &wazzup_api).await {
        log::warn!("Failed to sync contact {} to Wazzup: {}", contact_id, e);
    }

    // Получаем обновленную информацию из Wazzup
    let wazzup_contact = wazzup_api
        .get_contact(&api_key, &contact_id.to_string())
        .await
        .ok();

    let contact_with_wazzup = ContactWithWazzupData {
        id: updated_client.id,
        full_name: updated_client.full_name,
        email: updated_client.email,
        phone: updated_client.phone,
        wazzup_chat: updated_client.wazzup_chat,
        created_at: updated_client.created_at,
        wazzup_contact,
    };

    Ok(HttpResponse::Ok().json(contact_with_wazzup))
}

#[utoipa::path(
    delete,
    path = "/api/contacts/{companyId}/{contactId}",
    tag = "Contacts",
    params(
        ("companyId" = i64, Path, description = "Company ID"),
        ("contactId" = i64, Path, description = "Contact ID")
    ),
    responses(
        (status = 204, description = "Contact deleted successfully from both local database and Wazzup"),
        (status = 404, description = "Company or contact not found")
    )
)]
#[delete("/{companyId}/{contactId}")]
async fn delete_contact(
    app_state: web::Data<AppState>,
    path: web::Path<(i64, i64)>,
) -> Result<HttpResponse, AppError> {
    let (company_id, contact_id) = path.into_inner();
    let client_db = get_client_db_conn(company_id, &app_state).await?;
    let api_key = helpers::get_company_api_key(company_id, &app_state.db).await?;
    let wazzup_api = wazzup_api::WazzupApiService::new();

    // Проверяем что контакт существует
    let existing_client = client::clients::Entity::find_by_id(contact_id)
        .one(&client_db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Contact {} not found", contact_id)))?;

    // Сначала удаляем из Wazzup
    if let Err(e) = wazzup_api.delete_contact(&api_key, &contact_id.to_string()).await {
        log::warn!("Failed to delete contact {} from Wazzup: {}", contact_id, e);
        // Продолжаем удаление из локальной БД даже если не удалось удалить из Wazzup
    }

    // Удаляем из локальной БД
    let active_client: client::clients::ActiveModel = existing_client.into();
    active_client.delete(&client_db).await?;

    Ok(HttpResponse::NoContent().finish())
}

// Функция для регистрации всех маршрутов этого модуля
pub fn init_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/contacts")
            .service(get_contacts)
            .service(get_contact_by_id)
            .service(update_contact)
            .service(delete_contact),
    );
}
