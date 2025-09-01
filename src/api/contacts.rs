use actix_web::{delete, get, post, put, web, HttpResponse};
use sea_orm::{DatabaseConnection, EntityTrait};
use crate::{
    database::main::models as main_models,
    errors::AppError,
    services::wazzup_api::{self, CreateContactRequest, UpdateContactRequest},
    AppState,
};

// --- Helper Functions ---

/// Находит компанию и возвращает ее API ключ.
async fn get_company_api_key(
    company_id: i64,
    db: &DatabaseConnection,
) -> Result<String, AppError> {
    let company = main_models::Entity::find_by_id(company_id)
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

// --- Route Handlers ---

#[utoipa::path(
    get,
    path = "/api/company/{companyId}/contacts",
    tag = "Contacts",
    params(
        ("companyId" = i64, Path, description = "Company ID")
    ),
    responses(
        (status = 200, description = "List of contacts from Wazzup", body = wazzup_api::ContactListResponse),
        (status = 404, description = "Company not found")
    )
)]
#[get("/{companyId}/contacts")]
async fn get_contacts(
    app_state: web::Data<AppState>,
    path: web::Path<i64>,
) -> Result<HttpResponse, AppError> {
    let company_id = path.into_inner();
    let api_key = get_company_api_key(company_id, &app_state.db).await?;
    let wazzup_api = wazzup_api::WazzupApiService::new();

    let response = wazzup_api.get_contacts(&api_key).await?;
    Ok(HttpResponse::Ok().json(response))
}

#[utoipa::path(
    post,
    path = "/api/company/{companyId}/contacts",
    tag = "Contacts",
    params(
        ("companyId" = i64, Path, description = "Company ID")
    ),
    request_body = wazzup_api::CreateContactRequest,
    responses(
        (status = 201, description = "Contact created successfully in Wazzup", body = wazzup_api::Contact),
        (status = 404, description = "Company not found")
    )
)]
#[post("/{companyId}/contacts")]
async fn create_contact(
    app_state: web::Data<AppState>,
    path: web::Path<i64>,
    body: web::Json<CreateContactRequest>,
) -> Result<HttpResponse, AppError> {
    let company_id = path.into_inner();
    let api_key = get_company_api_key(company_id, &app_state.db).await?;
    let wazzup_api = wazzup_api::WazzupApiService::new();

    let response = wazzup_api.create_contact(&api_key, &body.into_inner()).await?;
    Ok(HttpResponse::Created().json(response))
}

#[utoipa::path(
    put,
    path = "/api/company/{companyId}/contacts/{contactId}",
    tag = "Contacts",
    params(
        ("companyId" = i64, Path, description = "Company ID"),
        ("contactId" = String, Path, description = "Contact ID")
    ),
    request_body = wazzup_api::UpdateContactRequest,
    responses(
        (status = 200, description = "Contact updated successfully in Wazzup", body = wazzup_api::Contact),
        (status = 404, description = "Company not found")
    )
)]
#[put("/{companyId}/contacts/{contactId}")]
async fn update_contact(
    app_state: web::Data<AppState>,
    path: web::Path<(i64, String)>,
    body: web::Json<UpdateContactRequest>,
) -> Result<HttpResponse, AppError> {
    let (company_id, contact_id) = path.into_inner();
    let api_key = get_company_api_key(company_id, &app_state.db).await?;
    let wazzup_api = wazzup_api::WazzupApiService::new();

    let response = wazzup_api
        .update_contact(&api_key, &contact_id, &body.into_inner())
        .await?;
    Ok(HttpResponse::Ok().json(response))
}

#[utoipa::path(
    delete,
    path = "/api/company/{companyId}/contacts/{contactId}",
    tag = "Contacts",
    params(
        ("companyId" = i64, Path, description = "Company ID"),
        ("contactId" = String, Path, description = "Contact ID")
    ),
    responses(
        (status = 204, description = "Contact deleted successfully from Wazzup"),
        (status = 404, description = "Company not found")
    )
)]
#[delete("/{companyId}/contacts/{contactId}")]
async fn delete_contact(
    app_state: web::Data<AppState>,
    path: web::Path<(i64, String)>,
) -> Result<HttpResponse, AppError> {
    let (company_id, contact_id) = path.into_inner();
    let api_key = get_company_api_key(company_id, &app_state.db).await?;
    let wazzup_api = wazzup_api::WazzupApiService::new();

    wazzup_api.delete_contact(&api_key, &contact_id).await?;
    Ok(HttpResponse::NoContent().finish())
}

// Функция для регистрации всех маршрутов этого модуля
pub fn init_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/company")
            .service(get_contacts)
            .service(create_contact)
            .service(update_contact)
            .service(delete_contact),
    );
}