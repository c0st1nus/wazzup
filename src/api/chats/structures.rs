use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MessageContentItem {
    pub r#type: String,
    pub content: String,
}

#[derive(Debug, Serialize, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MessageSender {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
}

#[derive(Debug, Serialize, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MessageView {
    pub id: String,
    pub content: Vec<MessageContentItem>,
    pub sender: MessageSender,
    pub is_inbound: bool,
    pub created_at: String,
}

#[derive(Debug, Serialize, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ChannelSummary {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
}

#[derive(Debug, Serialize, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ChatInfoSummary {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
}

#[derive(Debug, Serialize, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ClientSummary {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
}

#[derive(Debug, Serialize, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AssigneeSummary {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    pub role: String,
}

#[derive(Debug, Serialize, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ChatPreview {
    pub id: String,
    pub unread_count: i64,
    pub channel: ChannelSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_message: Option<MessageView>,
    pub chat_info: ChatInfoSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client: Option<ClientSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<AssigneeSummary>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChatPreviewList {
    pub data: Vec<ChatPreview>,
}

pub type ChatDetails = ChatPreview;

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessagesResponse {
    pub data: Vec<MessageView>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChatPreviewsQuery {
    pub offset: Option<u64>,
    pub count: Option<u64>,
    pub filter: Option<String>,
    pub bot: Option<bool>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MessagesQuery {
    pub offset: Option<u64>,
    pub count: Option<u64>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SendChatMessageRequest {
    pub message: OutgoingMessage,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OutgoingMessage {
    pub content: Vec<MessageContentItem>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SendChatMessageResponse {
    pub id: String,
    pub created_at: String,
}
