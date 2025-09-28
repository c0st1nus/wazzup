use std::ops::Deref;

use actix_web::{HttpRequest, web};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

use crate::{
    app_state::AppState,
    database::models::{companies, company_users, users},
    errors::AppError,
};

#[derive(Clone)]
pub struct EmployeeContext {
    pub company_uuid: Uuid,
    pub user_uuid: Uuid,
    pub company: companies::Model,
    pub user: users::Model,
    pub membership_role: Option<String>,
    pub company_id_bytes: Vec<u8>,
    pub user_id_bytes: Vec<u8>,
}

impl EmployeeContext {
    pub fn role(&self) -> Option<&str> {
        self.membership_role
            .as_deref()
            .or(self.user.role.as_deref())
    }

    pub fn role_lowercase(&self) -> Option<String> {
        self.role().map(|r| r.trim().to_ascii_lowercase())
    }

    pub fn is_admin(&self) -> bool {
        self.role_lowercase()
            .map(|role| role == "admin")
            .unwrap_or(false)
    }

    pub fn is_employee_role(&self) -> bool {
        matches!(
            self.role_lowercase().as_deref(),
            Some("admin") | Some("manager") | Some("employee")
        )
    }

    pub fn is_same_user(&self, other: &[u8]) -> bool {
        self.user_id_bytes == other
    }

    pub fn require_company_api_key(&self) -> Result<String, AppError> {
        self.company
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
            .ok_or_else(|| {
                AppError::InvalidInput(
                    "Wazzup API key is not configured for the company".to_string(),
                )
            })
    }
}

#[derive(Clone)]
pub struct AdminContext {
    pub company_uuid: Uuid,
    pub user_uuid: Uuid,
    pub company_id_bytes: Vec<u8>,
    pub user_id_bytes: Vec<u8>,
    pub api_key: String,
    employee: EmployeeContext,
}

impl Deref for AdminContext {
    type Target = EmployeeContext;

    fn deref(&self) -> &Self::Target {
        &self.employee
    }
}

impl AdminContext {
    pub fn employee(&self) -> &EmployeeContext {
        &self.employee
    }
}

pub fn option_i8_to_bool(flag: Option<i8>) -> Option<bool> {
    flag.map(|value| value != 0)
}

pub fn bytes_to_uuid(bytes: &[u8]) -> Option<Uuid> {
    Uuid::from_slice(bytes).ok()
}

pub fn uuid_bytes_to_string(bytes: &[u8]) -> Result<String, AppError> {
    bytes_to_uuid(bytes)
        .map(|uuid| uuid.to_string())
        .ok_or_else(|| AppError::Internal)
}

fn parse_uuid_cookie(req: &HttpRequest, name: &str) -> Result<Uuid, AppError> {
    let cookie = req
        .cookie(name)
        .ok_or_else(|| AppError::Unauthorized(format!("Missing `{}` cookie", name)))?;

    Uuid::parse_str(cookie.value())
        .map_err(|_| AppError::Unauthorized(format!("Invalid `{}` cookie", name)))
}

pub async fn resolve_employee_context(
    req: &HttpRequest,
    app_state: &web::Data<AppState>,
) -> Result<EmployeeContext, AppError> {
    let _session_cookie = req
        .cookie("session_id")
        .ok_or_else(|| AppError::Unauthorized("Missing `session_id` cookie".to_string()))?;

    let user_uuid = parse_uuid_cookie(req, "user_id")?;
    let company_uuid = parse_uuid_cookie(req, "company_id")?;

    let company_id_bytes = crate::api::helpers::uuid_to_bytes(&company_uuid);
    let user_id_bytes = crate::api::helpers::uuid_to_bytes(&user_uuid);

    let company = companies::Entity::find_by_id(company_id_bytes.clone())
        .one(&app_state.db)
        .await?
        .ok_or_else(|| AppError::Unauthorized("Company not found".to_string()))?;

    let membership = company_users::Entity::find()
        .filter(company_users::Column::CompanyId.eq(company_id_bytes.clone()))
        .filter(company_users::Column::UserId.eq(user_id_bytes.clone()))
        .one(&app_state.db)
        .await?
        .ok_or_else(|| AppError::Unauthorized("User is not attached to the company".to_string()))?;

    let user = users::Entity::find_by_id(user_id_bytes.clone())
        .one(&app_state.db)
        .await?
        .ok_or_else(|| AppError::Unauthorized("User not found".to_string()))?;

    Ok(EmployeeContext {
        company_uuid,
        user_uuid,
        company,
        user,
        membership_role: membership.role.clone(),
        company_id_bytes,
        user_id_bytes,
    })
}

pub fn ensure_employee_access(ctx: &EmployeeContext) -> Result<(), AppError> {
    if ctx.is_employee_role() {
        Ok(())
    } else {
        Err(AppError::Forbidden(
            "Employee role required for this operation".to_string(),
        ))
    }
}

pub fn ensure_admin_access(ctx: &EmployeeContext) -> Result<(), AppError> {
    if ctx.is_admin() {
        Ok(())
    } else {
        Err(AppError::Forbidden("Admin role required".to_string()))
    }
}

pub async fn resolve_admin_context(
    req: &HttpRequest,
    app_state: &web::Data<AppState>,
) -> Result<AdminContext, AppError> {
    let employee = resolve_employee_context(req, app_state).await?;
    ensure_employee_access(&employee)?;
    ensure_admin_access(&employee)?;

    let api_key = employee.require_company_api_key()?;

    Ok(AdminContext {
        company_uuid: employee.company_uuid,
        user_uuid: employee.user_uuid,
        company_id_bytes: employee.company_id_bytes.clone(),
        user_id_bytes: employee.user_id_bytes.clone(),
        api_key,
        employee,
    })
}

pub use crate::api::helpers::uuid_to_bytes;
