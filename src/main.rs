use actix_web::{web, App, HttpServer, middleware};
use actix_files as fs;
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
use crate::api::{admin, channels, chats, clients, companies, messages, users, webhooks, contacts};
use crate::database::{client::models as client_models, main::models as main_models, pool_manager::ClientDbPoolManager};
use crate::services::wazzup_api;

pub struct AppState {
    db: DatabaseConnection,
    config: Config,
    client_db_pool: ClientDbPoolManager,
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

    // Создаем pool manager для клиентских баз данных
    let client_db_pool = ClientDbPoolManager::new(config.clone());

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
            messages::get_local_messages,
            // Clients
            clients::get_clients,
            clients::get_client,
            clients::transfer_client,
            // Users
            users::get_users,
            users::create_user,
            users::get_settings,
            users::update_settings,
            // Contacts
            contacts::get_contacts,
            contacts::get_contact_by_id,
            contacts::update_contact,
            contacts::delete_contact,
            // Webhooks
            webhooks::handle_webhook,
            webhooks::connect_webhooks,
            webhooks::test_webhook,
            // Admin
            admin::get_db_pool_stats,
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
                contacts::UpdateContactDto,
                contacts::ContactWithWazzupData,
                channels::WrappedIframeLinkResponse,
                channels::ChannelAddedNotification,
                webhooks::ConnectWebhooksResponse,
                
                // --- Clients API Structs ---
                clients::TransferClientRequest,
                clients::TransferClientResponse,
                clients::ClientResponse,
                clients::ClientListResponse,
                clients::ClientQuery,
                
                // --- Messages API Structs ---
                messages::MessageResponse,
                messages::MessageListResponse,
                
                // --- Message Types ---
                client_models::MessageType,
                
                // --- Chats API Structs ---
                chats::ChatResponse,
                chats::ResponsibleUserInfo,
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
                wazzup_api::UpdateUserSettingsRequest,
                
                // --- Admin Structs ---
                admin::DatabasePoolStats
            )
        ),
        tags(
            (name = "Admin", description = "Administrative endpoints"),
            (name = "Companies", description = "Company management endpoints"),
            (name = "Channels", description = "Channel management endpoints"),
            (name = "Chats", description = "Chat management endpoints (local data only)"),
            (name = "Clients", description = "Client management endpoints"),
            (name = "Messages", description = "Message sending and retrieval endpoints"),
            (name = "Users", description = "User management endpoints"),
            (name = "Contacts", description = "Contact management endpoints (synced with Wazzup)"),
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
            .wrap(middleware::Compress::default())
            .app_data(web::Data::new(AppState {
                db: db.clone(),
                config: config.clone(),
                client_db_pool: client_db_pool.clone(),
            }))
            .service(
                fs::Files::new("/static", "./static")
                    .show_files_listing()
                    .use_last_modified(true)
            )
            .service(
                web::scope("/api")
                    .wrap(middleware::NormalizePath::trim())
                    .configure(companies::init_routes)
                    .configure(channels::init_routes)
                    .configure(chats::init_routes)
                    .configure(clients::init_routes)
                    .configure(messages::init_routes)
                    .configure(users::init_routes)
                    .configure(contacts::init_routes)
                    .configure(webhooks::init_routes)
                    .configure(admin::init_routes)
            )
            .service(
                web::redirect("/swagger", "/swagger/"))
                .service(SwaggerUi::new("/swagger/{_:.*}")
                    .url("/api-docs/openapi.json", ApiDoc::openapi()),
            )
    })
    .workers(num_cpus::get())
    .bind((host, port))?
    .run()
    .await
}