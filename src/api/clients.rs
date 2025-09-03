use actix_web::{post, web, HttpResponse};
use sea_orm::{EntityTrait, ColumnTrait, QueryFilter, QueryOrder, Set, ActiveModelTrait, TransactionTrait};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use crate::{
    errors::AppError,
    database::client::models::{Entity as Client, wazzup_transfer, user},
    AppState,
};

#[derive(Debug, Deserialize, ToSchema)]
pub struct TransferClientRequest {
    pub chat_id: String,
    pub to_user_id: i64,
    pub from_user_id: i64, // ID того, кто делает перевод
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TransferClientResponse {
    pub success: bool,
    pub message: String,
    pub transfer_id: Option<i64>,
}

#[utoipa::path(
    post,
    path = "/api/clients/transfer",
    tag = "Clients",
    request_body = TransferClientRequest,
    responses(
        (status = 200, description = "Client transferred successfully", body = TransferClientResponse),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Client or user not found")
    )
)]
#[post("/transfer")]
async fn transfer_client(
    app_state: web::Data<AppState>,
    body: web::Json<TransferClientRequest>,
) -> Result<HttpResponse, AppError> {
    let request = body.into_inner();
    
    // Проверяем, что целевой пользователь не quality_controll
    let target_user = user::Entity::find_by_id(request.to_user_id)
        .one(&app_state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Target user not found".to_string()))?;
    
    if target_user.role == "quality_controll" {
        return Err(AppError::Forbidden("Cannot transfer to quality_controll user".to_string()));
    }
    
    // Находим клиента по chat_id
    let client = Client::find()
        .filter(crate::database::client::models::Column::WazzupChat.eq(&request.chat_id))
        .one(&app_state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Client with this chat not found".to_string()))?;
    
    // Получаем информацию о том, кто делает перевод
    let from_user = user::Entity::find_by_id(request.from_user_id)
        .one(&app_state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("From user not found".to_string()))?;
    
    // Проверяем права на перевод
    if from_user.role == "admin" {
        // Админ может переводить кого угодно
    } else if from_user.role == "manager" {
        // Менеджер может переводить только если он ответственный
        if client.responsible_user_id != Some(request.from_user_id) {
            return Err(AppError::Forbidden("Only responsible manager can transfer client".to_string()));
        }
    } else {
        return Err(AppError::Forbidden("Only managers and admins can transfer clients".to_string()));
    }
    
    // Если пытаемся перевести на того же пользователя, ничего не делаем
    if client.responsible_user_id == Some(request.to_user_id) {
        return Ok(HttpResponse::Ok().json(TransferClientResponse {
            success: true,
            message: "Client is already assigned to this user".to_string(),
            transfer_id: None,
        }));
    }
    
    // Выполняем перевод ответственности
    let transfer_id = transfer_responsibility(
        &app_state.db,
        &request.chat_id,
        client.responsible_user_id,
        request.to_user_id,
        None,
    ).await?;
    
    Ok(HttpResponse::Ok().json(TransferClientResponse {
        success: true,
        message: "Client transferred successfully".to_string(),
        transfer_id: Some(transfer_id),
    }))
}

/// Общая функция для перевода ответственности
/// Возвращает ID созданной записи в wazzup_transfers
pub async fn transfer_responsibility(
    db: &sea_orm::DatabaseConnection,
    chat_id: &str,
    old_responsible_id: Option<i64>,
    new_responsible_id: i64,
    message_id: Option<String>,
) -> Result<i64, AppError> {
    let txn = db.begin().await?;
    
    // Находим клиента по chat_id
    let client = Client::find()
        .filter(crate::database::client::models::Column::WazzupChat.eq(chat_id))
        .one(&txn)
        .await?
        .ok_or_else(|| AppError::NotFound("Client with this chat not found".to_string()))?;
    
    // Проверяем, нужно ли создавать запись в transfers
    let need_transfer_record = if let Some(current_responsible_id) = old_responsible_id {
        // Проверяем последний transfer для этого чата
        let last_transfer = wazzup_transfer::Entity::find()
            .filter(wazzup_transfer::Column::ChatId.eq(chat_id))
            .order_by_desc(wazzup_transfer::Column::CreatedAt)
            .one(&txn)
            .await?;
        
        match last_transfer {
            Some(transfer) => transfer.to_user_id != new_responsible_id,
            None => current_responsible_id != new_responsible_id,
        }
    } else {
        true // Если не было ответственного, то точно нужно записать
    };
    
    // Обновляем ответственного в клиенте
    let mut client_active: crate::database::client::models::ActiveModel = client.into();
    client_active.responsible_user_id = Set(Some(new_responsible_id));
    client_active.update(&txn).await?;
    
    let mut transfer_id = 0i64;
    
    // Создаем запись в wazzup_transfers если нужно
    if need_transfer_record {
        let transfer = wazzup_transfer::ActiveModel {
            id: sea_orm::NotSet,
            chat_id: Set(chat_id.to_string()),
            from_user_id: Set(old_responsible_id.unwrap_or(0)), // 0 если не было ответственного
            to_user_id: Set(new_responsible_id),
            message_id: Set(message_id),
            created_at: Set(chrono::Utc::now()),
        };
        let inserted_transfer = transfer.insert(&txn).await?;
        transfer_id = inserted_transfer.id;
    }
    
    txn.commit().await?;
    Ok(transfer_id)
}

// Функция для регистрации всех маршрутов этого модуля
pub fn init_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/clients")
            .service(transfer_client)
    );
}
