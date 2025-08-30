use crate::errors::AppError;
use reqwest::{Client, Method};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

#[derive(Clone)]
pub struct WazzupApiService {
    client: Client,
    base_url: String,
}

// Generic request helpers
impl WazzupApiService {
    pub fn new(base_url: String) -> Self {
        Self {
            client: Client::new(),
            base_url,
        }
    }

    async fn request<T: Serialize, R: DeserializeOwned>(
        &self,
        api_key: &str,
        method: Method,
        path: &str,
        body: Option<&T>,
    ) -> Result<R, AppError> {
        let url = format!("{}{}", self.base_url, path);
        let mut request_builder = self.client.request(method, &url).bearer_auth(api_key);

        if let Some(body_data) = body {
            request_builder = request_builder.json(body_data);
        }

        let response = request_builder.send().await?;

        // ИСПРАВЛЕНИЕ: Сначала проверяем статус, потом обрабатываем тело.
        if !response.status().is_success() {
            // Теперь мы можем безопасно потребить `response`, чтобы получить текст ошибки.
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error reading response body".to_string());
            log::error!("Wazzup API Error on path {}: {} - {}", path, status, error_text);
            // Создаем ошибку вручную, так как response уже использован.
            return Err(AppError::InvalidInput(format!(
                "API request failed with status {}: {}",
                status, error_text
            )));
        }

        // Если статус успешный, парсим JSON.
        let result = response.json::<R>().await?;
        Ok(result)
    }

    // Специальный метод для PATCH вебхуков, так как у него другой base_url
    async fn request_patch_webhooks<T: Serialize, R: DeserializeOwned>(
        &self,
        api_key: &str,
        path: &str,
        body: &T,
    ) -> Result<R, AppError> {
        let url = format!("https://api.wazzup24.com{}", path);
        let response = self.client.patch(&url)
            .bearer_auth(api_key)
            .json(body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error reading response body".to_string());
             log::error!("Wazzup API Error on webhook patch: {} - {}", status, error_text);
            return Err(AppError::InvalidInput(format!(
                "API request failed with status {}: {}",
                status, error_text
            )));
        }

        let result = response.json::<R>().await?;
        Ok(result)
    }
}

// API method implementations
impl WazzupApiService {
    pub async fn generate_channel_iframe_link(
        &self,
        api_key: &str,
        request: &GenerateIframeLinkRequest,
    ) -> Result<GenerateIframeLinkResponse, AppError> {
        self.request(api_key, Method::POST, "/iframe/generate-channels-link", Some(request)).await
    }

    pub async fn get_channels(&self, api_key: &str) -> Result<ChannelListResponse, AppError> {
        self.request(api_key, Method::GET, "/channels/list", None::<&()>).await
    }
    
    pub async fn reinitialize_channel(&self, api_key: &str, transport: &str, channel_id: &str) -> Result<(), AppError> {
        let path = format!("/channels/{}/{}/reinit", transport, channel_id);
        let _: Value = self.request(api_key, Method::POST, &path, None::<&()>).await?;
        Ok(())
    }

    pub async fn delete_channel(&self, api_key: &str, transport: &str, channel_id: &str, delete_chats: bool) -> Result<(), AppError> {
        let path = format!("/channels/{}/{}", transport, channel_id);
        let body = serde_json::json!({ "deleteChats": delete_chats });
        let _: Value = self.request(api_key, Method::DELETE, &path, Some(&body)).await?;
        Ok(())
    }

    // --- Users/Settings ---
    
    pub async fn get_user_settings(&self, api_key: &str) -> Result<UserSettings, AppError> {
        self.request(api_key, Method::GET, "/settings", None::<&()>).await
    }

    pub async fn update_user_settings(
        &self,
        api_key: &str,
        request: &UpdateUserSettingsRequest,
    ) -> Result<(), AppError> {
        let _: Value = self.request(api_key, Method::PATCH, "/settings", Some(request)).await?;
        Ok(())
    }

    // --- Contacts ---
    
    pub async fn get_contacts(&self, api_key: &str) -> Result<ContactListResponse, AppError> {
        self.request(api_key, Method::GET, "/contacts", None::<&()>).await
    }
    
    pub async fn create_contact(
        &self,
        api_key: &str,
        request: &CreateContactRequest,
    ) -> Result<Contact, AppError> {
        self.request(api_key, Method::POST, "/contacts", Some(request)).await
    }
    
    pub async fn update_contact(
        &self,
        api_key: &str,
        contact_id: &str,
        request: &UpdateContactRequest,
    ) -> Result<Contact, AppError> {
        let path = format!("/contacts/{}", contact_id);
        self.request(api_key, Method::PUT, &path, Some(request)).await
    }
    
    pub async fn delete_contact(&self, api_key: &str, contact_id: &str) -> Result<(), AppError> {
        let path = format!("/contacts/{}", contact_id);
        let _: Value = self.request(api_key, Method::DELETE, &path, None::<&()>).await?;
        Ok(())
    }
    
    // --- Messages ---
    
    pub async fn send_message(
        &self,
        api_key: &str,
        request: &SendMessageRequest,
    ) -> Result<SendMessageResponse, AppError> {
        self.request(api_key, Method::POST, "/messages", Some(request)).await
    }

    pub async fn get_messages(&self, api_key: &str, chat_id: &str) -> Result<MessageListResponse, AppError> {
        let path = format!("/messages?chatId={}", chat_id);
        self.request(api_key, Method::GET, &path, None::<&()>).await
    }
    
    pub async fn get_unread_count(&self, api_key: &str) -> Result<UnreadCountResponse, AppError> {
        self.request(api_key, Method::GET, "/messages/unread-count", None::<&()>).await
    }

    // --- Webhooks ---

    pub async fn connect_webhooks(
        &self,
        api_key: &str,
        request: &WebhookSubscriptionRequest,
    ) -> Result<WebhookSubscriptionResponse, AppError> {
        self.request_patch_webhooks(api_key, "/v3/webhooks", request).await
    }
}


// --- Request & Response Structs ---

// Channels
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GenerateIframeLinkRequest {
    pub transport: Option<String>,
    pub channel_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GenerateIframeLinkResponse {
    pub link: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ChannelInfo {
    pub deleted: bool,
    pub details: Option<Value>, // Can be detailed later if needed
    pub guid: Option<String>,
    pub has_access: bool,
    pub name: Option<String>,
    pub phone: Option<String>,
    pub state: Option<String>,
    pub transport: Option<String>,
    pub visible: bool,
    pub tier: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct ChannelListResponse {
    pub channels: Option<Vec<ChannelInfo>>,
    pub count: i32,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PairingCodeRequest {
    pub pairing_phone: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PairingCodeResponse {
    pub pairing_phone: Option<String>,
    pub pairing_code: Option<String>,
}

// Users/Settings
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserSettings {
    pub push_input_output_message_events_for_managers: bool,
    pub user_roles: Option<Vec<UserRole>>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserRole {
    pub channel_id: Option<String>,
    pub user_id: Option<String>,
    pub role: Option<String>,
    pub allow_get_new_clients: bool,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateUserSettingsRequest {
    pub push_input_output_message_events_for_managers: Option<bool>,
    pub user_roles: Option<Vec<UserRole>>,
}

// Contacts
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Contact {
    pub id: Option<String>,
    pub name: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ContactListResponse {
    pub contacts: Option<Vec<Contact>>,
    pub count: i32,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateContactRequest {
    pub name: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateContactRequest {
    pub name: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
}

// Messages
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageRequest {
    pub chat_id: Option<String>,
    pub channel_id: Option<String>,
    pub text: Option<String>,
    pub content_type: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SendMessageResponse {
    #[serde(rename = "messageId")]
    pub message_id: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Message {
     pub id: Option<String>,
     pub chat_id: Option<String>,
     pub channel_id: Option<String>,
     pub text: Option<String>,
     pub content_type: Option<String>,
     pub created_at: Option<chrono::DateTime<chrono::Utc>>,
     pub direction: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct MessageListResponse {
    pub messages: Option<Vec<Message>>,
    pub count: i32,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UnreadCountResponse {
    pub unread_count: i32,
}


// Webhooks
#[derive(Debug, Serialize, Deserialize)]
pub struct WebhookSubscriptionRequest {
    #[serde(rename = "webhooksUri")]
    pub webhooks_uri: String,
    pub subscriptions: WebhookSubscriptions,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WebhookSubscriptions {
    pub messages_and_statuses: bool,
    pub contacts_and_deals_creation: bool,
    pub channels_updates: bool,
    pub template_status: bool,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct WebhookSubscriptionResponse {
    pub ok: bool,
}