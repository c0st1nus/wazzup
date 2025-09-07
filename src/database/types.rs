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
