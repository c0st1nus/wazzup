use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

// --- Message Types ---
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum MessageType {
    Text,
    Image, 
    Video,
    Docs,
    #[serde(rename = "missed_call")]
    MissedCall,
    Audio,
}

impl std::fmt::Display for MessageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MessageType::Text => write!(f, "text"),
            MessageType::Image => write!(f, "image"),
            MessageType::Video => write!(f, "video"),
            MessageType::Docs => write!(f, "docs"),
            MessageType::MissedCall => write!(f, "missed_call"),
            MessageType::Audio => write!(f, "audio"),
        }
    }
}

impl From<String> for MessageType {
    fn from(s: String) -> Self {
        match s.as_str() {
            "text" => MessageType::Text,
            "image" => MessageType::Image,
            "video" => MessageType::Video,
            "docs" => MessageType::Docs,
            "missed_call" => MessageType::MissedCall,
            "audio" => MessageType::Audio,
            _ => MessageType::Text, // Default fallback
        }
    }
}

impl From<&str> for MessageType {
    fn from(s: &str) -> Self {
        MessageType::from(s.to_string())
    }
}

// --- Bookings ---
pub mod booking {
    use super::*;
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, DeriveEntityModel, ToSchema)]
    #[sea_orm(table_name = "bookings")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i64,
        pub code: String,
        pub service_id: i64,
        pub client_id: i64,
        #[schema(value_type = String, format = DateTime)]
        pub start_datetime: DateTimeUtc,
        #[schema(value_type = String, format = DateTime)]
        pub end_datetime: DateTimeUtc,
        pub status: String,
        pub notes: Option<String>,
        #[schema(value_type = String, format = DateTime)]
        pub created_at: DateTimeUtc,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(
            belongs_to = "super::service::Entity",
            from = "Column::ServiceId",
            to = "super::service::Column::Id"
        )]
        Service,
        #[sea_orm(
            belongs_to = "super::Entity",
            from = "Column::ClientId",
            to = "super::Column::Id"
        )]
        Client,
    }

    impl Related<super::service::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::Service.def()
        }
    }

    impl Related<super::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::Client.def()
        }
    }

    impl ActiveModelBehavior for ActiveModel {}
}

// --- Services ---
pub mod service {
    use super::*;
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, DeriveEntityModel, ToSchema)]
    #[sea_orm(table_name = "services")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i64,
        pub name: String,
        pub duration: i32,
        pub price: i32,
        pub description: Option<String>,
        pub image_path: Option<String>,
        pub is_active: bool,
        #[schema(value_type = String, format = DateTime)]
        pub created_at: DateTimeUtc,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(has_many = "super::booking::Entity")]
        Booking,
    }

    impl Related<super::booking::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::Booking.def()
        }
    }

    impl ActiveModelBehavior for ActiveModel {}
}

// --- Resources ---
pub mod resource {
    use super::*;
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, DeriveEntityModel, ToSchema)]
    #[sea_orm(table_name = "resources")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i64,
        pub name: String,
        pub r#type: String,
        pub role_id: Option<i64>,
        pub quantity: i32,
        pub image_path: Option<String>,
        #[schema(value_type = String, format = DateTime)]
        pub created_at: DateTimeUtc,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

// --- Tasks ---
pub mod task {
    use super::*;
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, DeriveEntityModel, ToSchema)]
    #[sea_orm(table_name = "tasks")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i64,
        pub name: String,
        pub project_id: i64,
        pub parent_task_id: Option<i64>,
        #[schema(value_type = String, format = DateTime)]
        pub created_at: DateTimeUtc,
        pub content: Option<serde_json::Value>,
        pub status_id: i64,
        pub previous_task_id: Option<i64>,
        pub route: Option<String>,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

// --- Locations ---
pub mod location {
    use super::*;
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, DeriveEntityModel, ToSchema)]
    #[sea_orm(table_name = "locations")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i64,
        pub name: String,
        pub address: String,
        pub phone: String,
        pub resource: i64,
        #[schema(value_type = String, format = DateTime)]
        pub created_at: DateTimeUtc,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

// --- Availability Exceptions ---
pub mod availability_exception {
    use super::*;
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, DeriveEntityModel, ToSchema)]
    #[sea_orm(table_name = "availability_exceptions")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i64,
        pub resource_id: i64,
        #[schema(value_type = String, format = DateTime)]
        pub start_datetime: DateTimeUtc,
        #[schema(value_type = String, format = DateTime)]
        pub end_datetime: DateTimeUtc,
        pub r#type: String,
        pub reason: Option<String>,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

// --- Clients ---
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, DeriveEntityModel, ToSchema)]
#[sea_orm(table_name = "clients")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub full_name: String,
    #[sea_orm(unique)]
    pub email: String,
    pub phone: Option<String>,
    pub wazzup_chat: Option<String>,
    pub responsible_user_id: Option<i64>,
    #[schema(value_type = String, format = DateTime)]
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "wazzup_chat::Entity",
        from = "Column::WazzupChat",
        to = "wazzup_chat::Column::Id"
    )]
    WazzupChat,
    #[sea_orm(
        belongs_to = "user::Entity",
        from = "Column::ResponsibleUserId",
        to = "user::Column::Id"
    )]
    ResponsibleUser,
}

impl Related<wazzup_chat::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::WazzupChat.def()
    }
}

impl Related<user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ResponsibleUser.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

// --- Users ---
pub mod user {
    use super::*;
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, DeriveEntityModel, ToSchema)]
    #[sea_orm(table_name = "users")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i64,
        pub name: String,
        #[sea_orm(unique)]
        pub login: String,
        #[sea_orm(unique)]
        pub email: String,
        pub password_hash: String,
        pub salt: String,
        pub role: String, // bot; manager; admin; quality_control
        pub resource_id: Option<i64>,
        pub location_id: Option<i64>,
        #[schema(value_type = String, format = DateTime)]
        pub created_at: DateTimeUtc,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(has_many = "super::token::Entity")]
        Token,
    }

    impl Related<super::token::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::Token.def()
        }
    }

    impl ActiveModelBehavior for ActiveModel {}
}

// --- Tokens ---
pub mod token {
    use super::*;
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, DeriveEntityModel, ToSchema)]
    #[sea_orm(table_name = "tokens")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i64,
        #[sea_orm(unique)]
        pub token_hash: String,
        pub user_id: i64,
        pub name: String,
        #[schema(value_type = String, format = DateTime)]
        pub created_at: DateTimeUtc,
        #[schema(value_type = String, format = DateTime)]
        pub last_used_at: Option<DateTimeUtc>,
        #[schema(value_type = String, format = DateTime)]
        pub expires_at: DateTimeUtc,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(
            belongs_to = "super::user::Entity",
            from = "Column::UserId",
            to = "super::user::Column::Id"
        )]
        User,
    }

    impl Related<super::user::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::User.def()
        }
    }

    impl ActiveModelBehavior for ActiveModel {}
}


// --- WazzupChannel ---
pub mod wazzup_channel {
    use super::*;
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, DeriveEntityModel, ToSchema)]
    #[sea_orm(table_name = "wazzup_channels")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: String,
        pub r#type: String,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(has_many = "super::wazzup_chat::Entity")]
        WazzupChat,
        #[sea_orm(has_many = "super::wazzup_setting::Entity")]
        WazzupSetting,
    }

    impl Related<super::wazzup_chat::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::WazzupChat.def()
        }
    }
    
    impl Related<super::wazzup_setting::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::WazzupSetting.def()
        }
    }

    impl ActiveModelBehavior for ActiveModel {}
}

// --- WazzupChat ---
pub mod wazzup_chat {
    use super::*;
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, DeriveEntityModel, ToSchema)]
    #[sea_orm(table_name = "wazzup_chats")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: String,
        pub channel_id: String,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(
            belongs_to = "super::wazzup_channel::Entity",
            from = "Column::ChannelId",
            to = "super::wazzup_channel::Column::Id"
        )]
        WazzupChannel,
        #[sea_orm(has_many = "super::wazzup_message::Entity")]
        WazzupMessage,
        #[sea_orm(has_many = "super::Entity")]
        Client,
    }
    
    impl Related<super::wazzup_channel::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::WazzupChannel.def()
        }
    }

    impl Related<super::wazzup_message::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::WazzupMessage.def()
        }
    }
    
    impl Related<super::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::Client.def()
        }
    }
    
    impl ActiveModelBehavior for ActiveModel {}
}

// --- WazzupMessage ---
pub mod wazzup_message {
    use super::*;
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, DeriveEntityModel, ToSchema)]
    #[sea_orm(table_name = "wazzup_messages")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: String,
        #[sea_orm(column_type = "JsonBinary")]
        pub content: serde_json::Value, // JSON массив элементов контента
        pub chat_id: String,
        #[schema(value_type = String, format = DateTime)]
        pub created_at: DateTimeUtc,
        // Поля для определения направления сообщения (можно оставить для совместимости)
        pub is_inbound: Option<bool>, // true - входящее, false - исходящее
        pub is_echo: Option<bool>, // из API Wazzup
        pub direction_status: Option<String>, // "inbound", "outbound", etc.
        pub author_name: Option<String>, // имя отправителя
        pub author_id: Option<String>, // ID отправителя в CRM
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(
            belongs_to = "super::wazzup_chat::Entity",
            from = "Column::ChatId",
            to = "super::wazzup_chat::Column::Id"
        )]
        WazzupChat,
    }
    
    impl Related<super::wazzup_chat::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::WazzupChat.def()
        }
    }
    
    impl ActiveModelBehavior for ActiveModel {}
}

// --- WazzupSetting ---
pub mod wazzup_setting {
    use super::*;
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, DeriveEntityModel, ToSchema)]
    #[sea_orm(table_name = "wazzup_settings")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub wazzup_user_id: String,
        #[sea_orm(primary_key, auto_increment = false)]
        pub wazzup_channel_id: String,
        pub role: String,
        pub receives_messages: bool,
    }
    
    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(
            belongs_to = "super::wazzup_channel::Entity",
            from = "Column::WazzupChannelId",
            to = "super::wazzup_channel::Column::Id"
        )]
        WazzupChannel,
    }
    
    impl Related<super::wazzup_channel::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::WazzupChannel.def()
        }
    }

    impl ActiveModelBehavior for ActiveModel {}
}

// --- Wazzup Transfers ---
pub mod wazzup_transfer {
    use super::*;
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, DeriveEntityModel, ToSchema)]
    #[sea_orm(table_name = "wazzup_transfers")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i64,
        pub chat_id: String,
        pub from_user_id: i64,
        pub to_user_id: i64,
        pub message_id: Option<String>,
        #[schema(value_type = String, format = DateTime)]
        pub created_at: DateTimeUtc,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(
            belongs_to = "super::user::Entity",
            from = "Column::FromUserId",
            to = "super::user::Column::Id"
        )]
        FromUser,
        #[sea_orm(
            belongs_to = "super::user::Entity", 
            from = "Column::ToUserId",
            to = "super::user::Column::Id"
        )]
        ToUser,
        #[sea_orm(
            belongs_to = "super::wazzup_chat::Entity",
            from = "Column::ChatId",
            to = "super::wazzup_chat::Column::Id"
        )]
        WazzupChat,
    }

    impl Related<super::wazzup_chat::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::WazzupChat.def()
        }
    }

    impl ActiveModelBehavior for ActiveModel {}
}