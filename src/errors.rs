use actix_web::{http::StatusCode, HttpResponse, ResponseError};
use sea_orm::DbErr;
use thiserror::Error;
use serde::Serialize;

/// Унифицированная структура ответа об ошибке
#[derive(Serialize)]
pub struct ErrorResponse<'a> {
    pub code: &'a str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
}

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Database error: {0}")]
    DbError(#[from] DbErr),

    #[error("Reqwest error: {0}")]
    ReqwestError(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Internal server error")]
    Internal,
}

impl ResponseError for AppError {
    fn status_code(&self) -> StatusCode {
        match self {
            AppError::DbError(_) | AppError::ReqwestError(_) | AppError::JsonError(_) | AppError::Internal => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::InvalidInput(_) => StatusCode::BAD_REQUEST,
            AppError::Forbidden(_) => StatusCode::FORBIDDEN,
        }
    }

    fn error_response(&self) -> HttpResponse {
        let code = self.code();
        let message = self.to_string();
        // trace_id можно внедрить позже через middleware (корреляция)
        let body = ErrorResponse { code, message, details: None, trace_id: None };
        HttpResponse::build(self.status_code()).json(body)
    }
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            AppError::DbError(_) => "DB_ERROR",
            AppError::ReqwestError(_) => "HTTP_ERROR",
            AppError::JsonError(_) => "JSON_ERROR",
            AppError::NotFound(_) => "NOT_FOUND",
            AppError::InvalidInput(_) => "INVALID_INPUT",
            AppError::Forbidden(_) => "FORBIDDEN",
            AppError::Internal => "INTERNAL",
        }
    }
}