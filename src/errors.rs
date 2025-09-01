use actix_web::{http::StatusCode, HttpResponse, ResponseError};
use sea_orm::DbErr;
use thiserror::Error;

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
        }
    }

    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code()).json(serde_json::json!({
            "error": self.to_string()
        }))
    }
}