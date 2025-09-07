use actix_web::{get, post, web, HttpResponse};
use sea_orm::{EntityTrait, ColumnTrait, QueryFilter, QueryOrder, Set, ActiveModelTrait, TransactionTrait, PaginatorTrait};
use sea_orm::prelude::DateTimeWithTimeZone;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use crate::{
    errors::AppError,
    database::client::{clients::{Entity as Client}, wazzup_transfers, users},
    api::helpers,
    api::validation,
    app_state::AppState,
};

// --- API Response Structures ---

#[derive(Debug, Serialize, ToSchema)]
pub struct ClientResponse {
    pub id: i64,
    pub full_name: String,
    pub email: String,
    pub phone: Option<String>,
    pub wazzup_chat: Option<String>,
    pub responsible_user_id: Option<i64>,
    pub responsible_user_name: Option<String>,
    #[schema(value_type = String, format = DateTime)]
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ClientListResponse {
    pub clients: Vec<ClientResponse>,
    pub total: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ClientQuery {
    pub page: Option<u64>,
    pub limit: Option<u64>,
    pub search: Option<String>,
}

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

// --- Route Handlers ---

#[utoipa::path(
    get,
    path = "/api/clients/{companyId}",
    tag = "Clients",
    params(
        ("companyId" = i64, Path, description = "Company ID"),
        ("page" = Option<u64>, Query, description = "Page number (default: 1)"),
        ("limit" = Option<u64>, Query, description = "Items per page (default: 20)"),
        ("search" = Option<String>, Query, description = "Search by name, email or phone")
    ),
    responses(
        (status = 200, description = "List of clients", body = ClientListResponse),
        (status = 404, description = "Company not found")
    )
)]
#[get("/{companyId}")]
async fn get_clients(
    app_state: web::Data<AppState>,
    path: web::Path<i64>,
    query: web::Query<ClientQuery>,
) -> Result<HttpResponse, AppError> {
    let company_id = path.into_inner();
    let page = query.page.unwrap_or(1);
    let limit = query.limit.unwrap_or(20);
    if page == 0 || limit == 0 || limit > 200 {
        return Err(AppError::InvalidInput("Invalid pagination parameters".into()));
    }
    let search = query.search.as_ref();
    
    // Получаем подключение к клиентской базе данных
    let client_db = helpers::get_client_db_connection(company_id, &app_state).await?;
    
    // Создаем базовый запрос
    let mut query_builder = Client::find();
    
    // Добавляем поиск если указан
    if let Some(search_term) = search {
        use sea_orm::{QueryFilter, Condition};
        let mut search_condition = Condition::any()
            .add(crate::database::client::clients::Column::FullName.contains(search_term))
            .add(crate::database::client::clients::Column::Email.contains(search_term));
        
        // Если в поисковом запросе есть цифры, добавляем поиск по телефону
        let phone_search: String = search_term.chars().filter(|c| c.is_numeric()).collect();
        if !phone_search.is_empty() {
            search_condition = search_condition.add(crate::database::client::clients::Column::Phone.contains(&phone_search));
        }
        
        query_builder = query_builder.filter(search_condition);
    }
    
    // Получаем общее количество
    let total = query_builder.clone().count(&client_db).await? as i64;
    
    // Применяем пагинацию и сортировку
    let clients = query_builder
        .order_by_desc(crate::database::client::clients::Column::CreatedAt)
        .paginate(&client_db, limit)
        .fetch_page(page - 1)
        .await?;
    
    // Преобразуем в ответ API с информацией об ответственных пользователях
    let mut client_responses = Vec::new();
    for client in clients {
        let responsible_user_name = if let Some(user_id) = client.responsible_user_id {
            users::Entity::find_by_id(user_id)
                .one(&client_db)
                .await?
                .map(|u| u.name)
        } else {
            None
        };
        
        // Простейшая защита длины
        if !validation::ensure_max_len(&client.full_name, 200) {
            continue; // пропускаем аномально длинные записи
        }
        client_responses.push(ClientResponse {
            id: client.id,
            full_name: client.full_name,
            email: client.email,
            phone: client.phone,
            wazzup_chat: client.wazzup_chat,
            responsible_user_id: client.responsible_user_id,
            responsible_user_name,
            created_at: client.created_at.into(),
        });
    }
    
    let response = ClientListResponse {
        clients: client_responses,
        total,
    };

    Ok(HttpResponse::Ok().json(response))
}

#[utoipa::path(
    get,
    path = "/api/clients/{companyId}/{clientId}",
    tag = "Clients",
    params(
        ("companyId" = i64, Path, description = "Company ID"),
        ("clientId" = i64, Path, description = "Client ID")
    ),
    responses(
        (status = 200, description = "Client details", body = ClientResponse),
        (status = 404, description = "Company or client not found")
    )
)]
#[get("/{companyId}/{clientId}")]
async fn get_client(
    app_state: web::Data<AppState>,
    path: web::Path<(i64, i64)>,
) -> Result<HttpResponse, AppError> {
    let (company_id, client_id) = path.into_inner();
    
    // Получаем подключение к клиентской базе данных
    let client_db = helpers::get_client_db_connection(company_id, &app_state).await?;
    
    // Находим клиента
    let client = Client::find_by_id(client_id)
        .one(&client_db)
        .await?
        .ok_or_else(|| AppError::NotFound("Client not found".to_string()))?;
    
    // Получаем информацию об ответственном пользователе
    let responsible_user_name = if let Some(user_id) = client.responsible_user_id {
        users::Entity::find_by_id(user_id)
            .one(&client_db)
            .await?
            .map(|u| u.name)
    } else {
        None
    };
    
    let response = ClientResponse {
        id: client.id,
        full_name: client.full_name,
        email: client.email,
        phone: client.phone,
        wazzup_chat: client.wazzup_chat,
        responsible_user_id: client.responsible_user_id,
        responsible_user_name,
        created_at: client.created_at,
    };

    Ok(HttpResponse::Ok().json(response))
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
    let target_user = users::Entity::find_by_id(request.to_user_id)
        .one(&app_state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Target user not found".to_string()))?;
    
    if target_user.role == "quality_controll" {
        return Err(AppError::Forbidden("Cannot transfer to quality_controll user".to_string()));
    }
    
    // Находим клиента по chat_id
    let client = Client::find()
        .filter(crate::database::client::clients::Column::WazzupChat.eq(&request.chat_id))
        .one(&app_state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Client with this chat not found".to_string()))?;
    
    // Получаем информацию о том, кто делает перевод
    let from_user = users::Entity::find_by_id(request.from_user_id)
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
        .filter(crate::database::client::clients::Column::WazzupChat.eq(chat_id))
        .one(&txn)
        .await?
        .ok_or_else(|| AppError::NotFound("Client with this chat not found".to_string()))?;
    
    // Проверяем, нужно ли создавать запись в transfers
    let need_transfer_record = if let Some(current_responsible_id) = old_responsible_id {
        // Проверяем последний transfer для этого чата
        let last_transfer = wazzup_transfers::Entity::find()
            .filter(wazzup_transfers::Column::ChatId.eq(chat_id))
            .order_by_desc(wazzup_transfers::Column::CreatedAt)
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
    let mut client_active: crate::database::client::clients::ActiveModel = client.into();
    client_active.responsible_user_id = Set(Some(new_responsible_id));
    client_active.update(&txn).await?;
    
    let mut transfer_id = 0i64;
    
    // Создаем запись в wazzup_transfers если нужно
    if need_transfer_record {
        let transfer = wazzup_transfers::ActiveModel {
            id: sea_orm::NotSet,
            chat_id: Set(chat_id.to_string()),
            from_user_id: Set(old_responsible_id.unwrap_or(0)), // 0 если не было ответственного
            to_user_id: Set(new_responsible_id),
            message_id: Set(message_id),
            created_at: Set(chrono::Utc::now().into()),
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
            .service(get_clients)
            .service(get_client)
            .service(transfer_client)
    );
}
