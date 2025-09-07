use actix_web::{delete, get, post, put, web, HttpResponse};
use sea_orm::{ActiveModelTrait, EntityTrait, IntoActiveModel, Set};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{
    database::main,
    errors::AppError,
    AppState,
};

// --- DTOs (Data Transfer Objects) ---

#[derive(Deserialize, ToSchema, Clone)]
pub struct CreateCompanyDto {
    name: String,
    email: String,
    database_name: String,
    wazzup_api_key: String,
    description: Option<String>,
    phone: Option<String>,
}

#[derive(Deserialize, ToSchema, Clone)]
pub struct UpdateCompanyDto {
    name: String,
    email: String,
    description: Option<String>,
    phone: Option<String>,
    wazzup_api_key: String,
    is_active: Option<bool>,
}

// --- Route Handlers ---


#[utoipa::path(
    get,
    path = "/api/companies",
    tag = "Companies",
    responses(
        (status = 200, description = "List all companies", body = [main::companies::Model])
    )
)]
#[get("")]
async fn get_companies(data: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    let companies = main::companies::Entity::find().all(&data.db).await?;
    Ok(HttpResponse::Ok().json(companies))
}


#[utoipa::path(
    get,
    path = "/api/companies/{id}",
    tag = "Companies",
    params(
        ("id" = i64, Path, description = "Company ID")
    ),
    responses(
        (status = 200, description = "Company found", body = main::companies::Model),
        (status = 404, description = "Company not found")
    )
)]
#[get("/{id}")]
async fn get_company_by_id(
    data: web::Data<AppState>,
    path: web::Path<i64>,
) -> Result<HttpResponse, AppError> {
    let company_id = path.into_inner();
    let company = main::companies::Entity::find_by_id(company_id)
        .one(&data.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Company with id {} not found", company_id)))?;

    Ok(HttpResponse::Ok().json(company))
}


#[utoipa::path(
    post,
    path = "/api/companies",
    tag = "Companies",
    request_body = CreateCompanyDto,
    responses(
        (status = 201, description = "Company created successfully", body = main::companies::Model)
    )
)]
#[post("")]
async fn create_company(
    data: web::Data<AppState>,
    new_company_dto: web::Json<CreateCompanyDto>,
) -> Result<HttpResponse, AppError> {
    let company = main::companies::ActiveModel {
        name: Set(new_company_dto.name.clone()),
        email: Set(new_company_dto.email.clone()),
        database_name: Set(new_company_dto.database_name.clone()),
        wazzup_api_key: Set(new_company_dto.wazzup_api_key.clone()),
        description: Set(new_company_dto.description.clone()),
        phone: Set(new_company_dto.phone.clone()),
        created_at: Set(Some(chrono::Utc::now().into())),
        is_active: Set(Some(true)),
        ..Default::default()
    };

    let created_company = company.insert(&data.db).await?;
    Ok(HttpResponse::Created().json(created_company))
}


#[utoipa::path(
    put,
    path = "/api/companies/{id}",
    tag = "Companies",
    params(
        ("id" = i64, Path, description = "Company ID")
    ),
    request_body = UpdateCompanyDto,
    responses(
        (status = 200, description = "Company updated successfully", body = main::companies::Model),
        (status = 404, description = "Company not found")
    )
)]
#[put("/{id}")]
async fn update_company(
    data: web::Data<AppState>,
    path: web::Path<i64>,
    update_dto: web::Json<UpdateCompanyDto>,
) -> Result<HttpResponse, AppError> {
    let company_id = path.into_inner();
    let company_to_update = main::companies::Entity::find_by_id(company_id)
        .one(&data.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Company with id {} not found", company_id)))?;

    let mut active_model = company_to_update.into_active_model();

    active_model.name = Set(update_dto.name.clone());
    active_model.email = Set(update_dto.email.clone());
    active_model.description = Set(update_dto.description.clone());
    active_model.phone = Set(update_dto.phone.clone());
    active_model.wazzup_api_key = Set(update_dto.wazzup_api_key.clone());
    active_model.is_active = Set(update_dto.is_active);
    active_model.updated_at = Set(Some(chrono::Utc::now().into()));

    let updated_company = active_model.update(&data.db).await?;
    Ok(HttpResponse::Ok().json(updated_company))
}

#[utoipa::path(
    delete,
    path = "/api/companies/{id}",
    tag = "Companies",
    params(
        ("id" = i64, Path, description = "Company ID")
    ),
    responses(
        (status = 204, description = "Company deleted successfully"),
        (status = 404, description = "Company not found")
    )
)]
#[delete("/{id}")]
async fn delete_company(
    data: web::Data<AppState>,
    path: web::Path<i64>,
) -> Result<HttpResponse, AppError> {
    let company_id = path.into_inner();
    let company_to_delete = main::companies::Entity::find_by_id(company_id)
        .one(&data.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Company with id {} not found", company_id)))?;

    company_to_delete.into_active_model().delete(&data.db).await?;

    Ok(HttpResponse::NoContent().finish())
}


// Функция для регистрации всех маршрутов этого модуля
pub fn init_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/companies")
            .service(get_companies)
            .service(get_company_by_id)
            .service(create_company)
            .service(update_company)
            .service(delete_company)
    );
}
