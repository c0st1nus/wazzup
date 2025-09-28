use actix_web::{web, App, HttpServer, middleware};
use actix_files as fs;
use sea_orm::Database;
use std::env;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;
use dotenvy::dotenv;
use actix_cors::Cors;

mod api;
mod config;
mod database;
mod errors;
mod services;
mod app_state; // ensure app_state visible to crate::* imports

use crate::config::Config;
use crate::api::{channels, chats, companies, messages, timezone, users, webhooks, contacts, clients};
use crate::database::models;
use crate::services::{wazzup_api, bot_service};
use crate::app_state::AppState;

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
            // Timezone
            timezone::get_current_timezone,
            timezone::get_current_time,
            // Contacts
            contacts::get_contacts,
            contacts::get_contact_by_id,
            contacts::update_contact,
            contacts::delete_contact,
            // Webhooks
            webhooks::handle_webhook,
            webhooks::connect_webhooks,
            webhooks::test_webhook,
        ),
        components(
            schemas(
                contacts::UpdateContactDto,
                contacts::ContactWithWazzupData,
                channels::WrappedIframeLinkResponse,
                channels::ChannelAddedNotification,
                webhooks::ConnectWebhooksResponse,
                
                // --- Chats API Structs ---
                chats::ChatResponse,
                chats::MessageInfo,
                chats::ChatListResponse,
                chats::ChatDetailsResponse,
                chats::ClientInfo,
                chats::ResponsibleUserInfo,
                
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
            (name = "Timezone", description = "Timezone conversion and utility endpoints"),
            (name = "Contacts", description = "Contact management endpoints (synced with Wazzup)"),
            (name = "Webhooks", description = "Endpoints for receiving Wazzup webhooks"),
        )
    )]
    struct ApiDoc;

    let host = config.host.clone();
    let port = config.port;

    log::info!("Starting server at http://{}:{}", host, port);
    log::info!("Swagger UI available at http://{}:{}/swagger-ui/", host, port);

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(AppState {
                db: db.clone(),
                config: config.clone(),
                wazzup_api: wazzup_api::WazzupApiService::new(),
                bot_service: bot_service::BotService::new(),
            }))
            // Глобальный лимит размера тела (1MB)
            .app_data(actix_web::web::PayloadConfig::new(config.effective_max_body_bytes()))
            .wrap(api::middleware::RequestId)
            .wrap(
                Cors::default()
                    .allow_any_origin()
                    .allow_any_method()
                    .allow_any_header()
                    .max_age(3600)
            )
            .service(
                fs::Files::new("/static", "./static")
                    .show_files_listing()
                    .use_last_modified(true)
            )
            .service(
                web::scope("/api")
                    .wrap(middleware::NormalizePath::trim())
                    .configure(channels::init_routes)
                    .configure(chats::init_routes)
                    .configure(messages::init_routes)
                    .configure(contacts::init_routes)
                    .configure(webhooks::init_routes)
                    .configure(clients::init_routes)
            )
            .service(
                web::redirect("/swagger", "/swagger/"))
                .service(SwaggerUi::new("/swagger/{_:.*}")
                    .url("/api-docs/openapi.json", ApiDoc::openapi()),
            )
    })
    .bind((host, port))?
    .run()
    .await
}