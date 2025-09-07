use actix_web::{get, web, HttpResponse};
use chrono::{DateTime, Utc, Offset};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    errors::AppError,
    app_state::AppState,
};

/// Структура для работы с временными зонами в API
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TimezoneInfo {
    /// Название временной зоны (например, "Europe/Moscow")
    pub name: String,
    /// Смещение от UTC в секундах
    pub offset_seconds: i32,
    /// Аббревиатура временной зоны (например, "MSK")
    pub abbreviation: String,
}

/// Утилиты для работы с временными зонами
struct TimezoneUtils;

impl TimezoneUtils {
    /// Конвертирует UTC время в указанную временную зону
    fn utc_to_timezone(utc_time: DateTime<Utc>, timezone: Tz) -> DateTime<Tz> {
        utc_time.with_timezone(&timezone)
    }

    /// Получает информацию о временной зоне
    fn get_timezone_info(timezone: Tz, at_time: DateTime<Utc>) -> TimezoneInfo {
        let tz_time = Self::utc_to_timezone(at_time, timezone);
        let offset = tz_time.offset().fix().local_minus_utc();
        
        TimezoneInfo {
            name: timezone.to_string(),
            offset_seconds: offset,
            abbreviation: tz_time.format("%Z").to_string(),
        }
    }

    /// Форматирует время с учетом временной зоны для API
    fn format_time_with_timezone(utc_time: DateTime<Utc>, timezone: Tz) -> String {
        let tz_time = Self::utc_to_timezone(utc_time, timezone);
        tz_time.format("%Y-%m-%d %H:%M:%S %Z").to_string()
    }
}

#[utoipa::path(
    get,
    path = "/api/timezone/current",
    tag = "Timezone", 
    responses(
        (status = 200, description = "Current server timezone info", body = TimezoneInfo)
    )
)]
#[get("/current")]
async fn get_current_timezone(app_state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    let server_tz = app_state.config.get_timezone()
        .map_err(|e| AppError::InvalidInput(format!("Invalid server timezone: {}", e)))?;
    
    let now = Utc::now();
    let timezone_info = TimezoneUtils::get_timezone_info(server_tz, now);

    Ok(HttpResponse::Ok().json(timezone_info))
}

#[utoipa::path(
    get,
    path = "/api/timezone/current_time",
    tag = "Timezone", 
    responses(
        (status = 200, description = "Current server time in configured timezone", body = String)
    )
)]
#[get("/current_time")]
async fn get_current_time(app_state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    let server_tz = app_state.config.get_timezone()
        .map_err(|e| AppError::InvalidInput(format!("Invalid server timezone: {}", e)))?;
    
    let now_utc = Utc::now();
    let formatted_time = TimezoneUtils::format_time_with_timezone(now_utc, server_tz);

    Ok(HttpResponse::Ok().json(formatted_time))
}

// Функция для регистрации всех маршрутов этого модуля
pub fn init_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/timezone")
            .service(get_current_timezone)
            .service(get_current_time)
    );
}
