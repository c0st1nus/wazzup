use actix_web::{web, App, HttpServer, middleware};
use sea_orm::{Database, DatabaseConnection};
use std::env;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;
use dotenvy::dotenv;

mod api;
mod config;
mod database;
mod errors;
mod services;

use crate::config::Config;
use crate::api::{channels, chats, companies, messages, users, webhooks, contacts};
use crate::database::{client::models as client_models, main::models as main_models};
use crate::services::wazzup_api;

pub struct AppState {
    db: DatabaseConnection,
    config: Config,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv().ok();
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let config = Config::from_env().expect("Failed to load configuration");
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let db = Database::connect(&db_url)
        .await
        .expect("Failed to connect to database");

    #[derive(OpenApi)]
    #[openapi(
        paths(
            // Companies
            companies::get_companies,
            companies::get_company_by_id,
            companies::create_company,
            companies::update_company,
            companies::delete_company,
            // Channels
            channels::get_channels,
            channels::delete_channel,
            channels::generate_wrapped_iframe_link,
            channels::handle_channel_added,
            // Chats
            chats::get_chats,
            chats::get_chat_details,
            chats::get_chat_messages,
            // Messages
            messages::send_message,
            messages::get_messages,
            messages::get_unread_count,
            // Users
            users::get_users,
            users::create_user,
            users::get_settings,
            users::update_settings,
            // Webhooks
            webhooks::handle_webhook,
            webhooks::connect_webhooks,
            webhooks::test_webhook,
        ),
        components(
            schemas(
                // --- Models ---
                main_models::Model, // Company
                client_models::user::Model, // User

                // --- DTOs & API Structs ---
                companies::CreateCompanyDto,
                companies::UpdateCompanyDto,
                users::CreateUserDto,
                channels::WrappedIframeLinkResponse,
                channels::ChannelAddedNotification,
                webhooks::ConnectWebhooksResponse,
                
                // --- Chats API Structs ---
                chats::ChatResponse,
                chats::MessageInfo,
                chats::ChatListResponse,
                chats::ChatDetailsResponse,
                
                // --- Wazzup API Structs ---
                wazzup_api::ChannelListResponse,
                wazzup_api::ChannelInfo,
                wazzup_api::GenerateIframeLinkRequest,
                wazzup_api::SendMessageRequest,
                wazzup_api::SendMessageResponse,
                wazzup_api::MessageListResponse,
                wazzup_api::Message,
                wazzup_api::UnreadCountResponse,
                wazzup_api::UserSettings,
                wazzup_api::UserRole,
                wazzup_api::UpdateUserSettingsRequest
            )
        ),
        tags(
            (name = "Companies", description = "Company management endpoints"),
            (name = "Channels", description = "Channel management endpoints"),
            (name = "Chats", description = "Chat management endpoints (local data only)"),
            (name = "Messages", description = "Message sending and retrieval endpoints"),
            (name = "Users", description = "User management endpoints"),
            (name = "Webhooks", description = "Endpoints for receiving Wazzup webhooks")
        )
    )]
    struct ApiDoc;

    let host = config.host.clone();
    let port = config.port;

    log::info!("Starting server at http://{}:{}", host, port);
    log::info!("Swagger UI available at http://{}:{}/swagger-ui/", host, port);

    HttpServer::new(move || {
        App::new()
            .wrap(middleware::NormalizePath::always())
            .app_data(web::Data::new(AppState {
                db: db.clone(),
                config: config.clone(),
            }))
            .service(
                web::scope("/api")
                    .configure(companies::init_routes)
                    .configure(channels::init_routes)
                    .configure(chats::init_routes)
                    .configure(messages::init_routes)
                    .configure(users::init_routes)
                    .configure(contacts::init_routes)
                    .configure(webhooks::init_routes)
            )
            .service(
                SwaggerUi::new("/swagger-ui/{_:.*}")
                    .url("/api-docs/openapi.json", ApiDoc::openapi()),
            )
    })
    .bind((host, port))?
    .run()
    .await
}