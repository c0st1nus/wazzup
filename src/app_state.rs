use crate::config::Config;
use crate::services::bot_service::BotService;
use crate::services::wazzup_api::WazzupApiService;
use sea_orm::DatabaseConnection;

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub config: Config,
    pub wazzup_api: WazzupApiService,
    pub bot_service: BotService,
}
