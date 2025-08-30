use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

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
    #[sea_orm(has_many = "responsibility::Entity")]
    Responsibility,
}

impl Related<wazzup_chat::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::WazzupChat.def()
    }
}

impl Related<responsibility::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Responsibility.def()
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
        pub role: String,
        #[schema(value_type = String, format = DateTime)]
        pub created_at: DateTimeUtc,
        pub resource_id: Option<i64>,
        pub location: Option<i64>,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(has_many = "super::token::Entity")]
        Token,
        #[sea_orm(has_many = "super::responsibility::Entity")]
        Responsibility,
    }

    impl Related<super::token::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::Token.def()
        }
    }

    impl Related<super::responsibility::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::Responsibility.def()
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
        pub r#type: String,
        pub content: String,
        pub chat_id: String,
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

// --- Responsibility ---
pub mod responsibility {
    use super::*;
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, DeriveEntityModel, ToSchema)]
    #[sea_orm(table_name = "responsibilities")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub user_field: i64,
        #[sea_orm(primary_key, auto_increment = false)]
        pub client: i64,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(
            belongs_to = "super::user::Entity",
            from = "Column::UserField",
            to = "super::user::Column::Id"
        )]
        User,
        #[sea_orm(
            belongs_to = "super::Entity",
            from = "Column::Client",
            to = "super::Column::Id"
        )]
        Client,
    }

    impl Related<super::user::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::User.def()
        }
    }

    impl Related<super::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::Client.def()
        }
    }
    
    impl ActiveModelBehavior for ActiveModel {}
}