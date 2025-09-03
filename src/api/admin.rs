use actix_web::{get, web, HttpResponse};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{errors::AppError, AppState};

#[derive(Serialize, ToSchema)]
pub struct DatabasePoolStats {
    active_connections: usize,
    active_databases: Vec<String>,
}

/// Получает статистику по активным подключениям к базам данных
#[utoipa::path(
    get,
    path = "/api/admin/db-pool-stats",
    tag = "Admin",
    responses(
        (status = 200, description = "Database pool statistics", body = DatabasePoolStats)
    )
)]
#[get("/db-pool-stats")]
async fn get_db_pool_stats(app_state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    let active_connections = app_state.client_db_pool.active_connections_count().await;
    let active_databases = app_state.client_db_pool.get_active_databases().await;

    let stats = DatabasePoolStats {
        active_connections,
        active_databases,
    };

    Ok(HttpResponse::Ok().json(stats))
}

pub fn init_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/admin")
            .service(get_db_pool_stats)
    );
}
