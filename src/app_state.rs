use sea_orm::DatabaseConnection;
use crate::config::Config;
use crate::services::wazzup_api::WazzupApiService;
use crate::services::bot_service::BotService;

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub config: Config,
    pub wazzup_api: WazzupApiService,
    pub bot_service: BotService,
}