use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};

#[derive(Serialize, ToSchema, Clone)]
pub struct WrappedIframeLinkResponse {
    pub link: String,
}

#[derive(Deserialize, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ChannelAddedNotification {
    pub channel_id: String,
    pub state: String,
    pub timestamp: i64,
}

#[derive(Serialize, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ChannelView {
    pub deleted: bool,
    pub details: Option<Value>,
    pub guid: Option<String>,
    pub has_acecess: bool,
    pub is_inbound: Option<bool>,
    pub name: Option<String>,
    pub phone: Option<String>,
    pub state: Option<String>,
    pub tier: Option<String>,
    pub transport: Option<String>,
    pub visible: bool,
}

#[derive(Serialize, ToSchema, Clone)]
pub struct ChannelsResponse {
    pub data: Vec<ChannelView>,
}

#[derive(Serialize, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ChannelDeletionResponse {
    pub message: String,
    pub channel_id: String,
}

#[derive(Deserialize, IntoParams)]
pub struct DeleteChannelQuery {
    /// Whether to delete chats along with the channel. Default is true.
    #[serde(default = "default_delete_chats")]
    pub delete_chats: bool,
}

pub fn default_delete_chats() -> bool {
    true
}
