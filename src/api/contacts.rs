use actix_web::{delete, get, post, put, web, HttpResponse};

use crate::{
    api::helpers,
    errors::AppError,
    services::wazzup_api::{self, CreateContactRequest, UpdateContactRequest},
    AppState,
};

// --- Route Handlers ---

#[utoipa::path(
    get,
    path = "/api/contacts/{companyId}",
    tag = "Contacts",
    params(
        ("companyId" = i64, Path, description = "Company ID")
    ),
    responses(
        (status = 200, description = "List of contacts from Wazzup", body = wazzup_api::ContactListResponse),
        (status = 404, description = "Company not found")
    )
)]
#[get("/{companyId}")]
async fn get_contacts(
    app_state: web::Data<AppState>,
    path: web::Path<i64>,
) -> Result<HttpResponse, AppError> {
    let company_id = path.into_inner();
    let api_key = helpers::get_company_api_key(company_id, &app_state.db).await?;
    let wazzup_api = wazzup_api::WazzupApiService::new();

    let response = wazzup_api.get_contacts(&api_key).await?;
    Ok(HttpResponse::Ok().json(response))
}

#[utoipa::path(
    post,
    path = "/api/contacts/{companyId}",
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
#[post("/{companyId}")]
async fn create_contact(
    app_state: web::Data<AppState>,
    path: web::Path<i64>,
    body: web::Json<CreateContactRequest>,
) -> Result<HttpResponse, AppError> {
    let company_id = path.into_inner();
    let api_key = helpers::get_company_api_key(company_id, &app_state.db).await?;
    let wazzup_api = wazzup_api::WazzupApiService::new();

    let response = wazzup_api.create_contact(&api_key, &body.into_inner()).await?;
    Ok(HttpResponse::Created().json(response))
}

#[utoipa::path(
    put,
    path = "/api/contacts/{companyId}/{contactId}",
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
#[put("/{companyId}/{contactId}")]
async fn update_contact(
    app_state: web::Data<AppState>,
    path: web::Path<(i64, String)>,
    body: web::Json<UpdateContactRequest>,
) -> Result<HttpResponse, AppError> {
    let (company_id, contact_id) = path.into_inner();
    let api_key = helpers::get_company_api_key(company_id, &app_state.db).await?;
    let wazzup_api = wazzup_api::WazzupApiService::new();

    let response = wazzup_api
        .update_contact(&api_key, &contact_id, &body.into_inner())
        .await?;
    Ok(HttpResponse::Ok().json(response))
}

#[utoipa::path(
    delete,
    path = "/api/contacts/{companyId}/{contactId}",
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
#[delete("/{companyId}/{contactId}")]
async fn delete_contact(
    app_state: web::Data<AppState>,
    path: web::Path<(i64, String)>,
) -> Result<HttpResponse, AppError> {
    let (company_id, contact_id) = path.into_inner();
    let api_key = helpers::get_company_api_key(company_id, &app_state.db).await?;
    let wazzup_api = wazzup_api::WazzupApiService::new();

    wazzup_api.delete_contact(&api_key, &contact_id).await?;
    Ok(HttpResponse::NoContent().finish())
}

// Функция для регистрации всех маршрутов этого модуля
pub fn init_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/contacts")
            .service(get_contacts)
            .service(create_contact)
            .service(update_contact)
            .service(delete_contact),
    );
}