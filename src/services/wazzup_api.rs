use crate::errors::AppError;
use reqwest::{Client, Method};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

const WAZZUP_API_BASE_URL: &str = "https://tech.wazzup24.com";

#[derive(Clone)]
pub struct WazzupApiService {
    client: Client,
    base_url: String,
}

// Generic request helpers
impl WazzupApiService {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            base_url: WAZZUP_API_BASE_URL.to_string(),
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

        let response = match request_builder.send().await {
            Ok(resp) => resp,
            Err(e) => {
                log::error!("Failed to send request to {}: {}", url, e);
                return Err(AppError::ReqwestError(e));
            }
        };

        // Проверяем статус и собираем детальную информацию об ошибке
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error reading response body".to_string());
            log::error!("Wazzup API Error on path {}: {} - {}", path, status, error_text);
            
            return Err(AppError::InvalidInput(format!(
                "API request to {} failed with status {}: {}",
                path, status, error_text
            )));
        }

        // Если статус успешный, парсим JSON с лучшей обработкой ошибок
        match response.json::<R>().await {
            Ok(result) => Ok(result),
            Err(e) => {
                log::error!("Failed to parse JSON response from {}: {}", path, e);
                Err(AppError::ReqwestError(e))
            }
        }
    }

    // Специальный метод для PATCH вебхуков, который возвращает строку
    async fn request_patch_webhooks_string<T: Serialize>(
        &self,
        api_key: &str,
        path: &str,
        body: &T,
    ) -> Result<String, AppError> {
        let url = format!("https://api.wazzup24.com{}", path);
        log::info!("Making webhook PATCH request to: {}", url);
        log::info!("Request body: {:?}", serde_json::to_string(body).unwrap_or_default());
        
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

        log::info!("Webhook PATCH request successful, status: {}", response.status());
        let response_text = response.text().await?;
        log::info!("Webhook PATCH response body: {}", response_text);
        
        Ok(response_text)
    }

    // Специальный метод для контактов через api.wazzup24.com
    async fn request_contacts_api<T: Serialize, R: DeserializeOwned>(
        &self,
        api_key: &str,
        method: Method,
        path: &str,
        body: Option<&T>,
    ) -> Result<R, AppError> {
        let url = format!("https://api.wazzup24.com{}", path);
        let mut request_builder = self.client.request(method, &url).bearer_auth(api_key);

        if let Some(body_data) = body {
            request_builder = request_builder.json(body_data);
        }

        log::info!("Making contacts API request to: {}", url);
        if let Some(body_data) = body {
            log::info!("Request body: {:?}", serde_json::to_string(body_data).unwrap_or_default());
        }

        let response = request_builder.send().await?;

        // Проверяем статус, потом обрабатываем тело.
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error reading response body".to_string());
            log::error!("Wazzup Contacts API Error on path {}: {} - {}", path, status, error_text);
            return Err(AppError::InvalidInput(format!(
                "Contacts API request failed with status {}: {}",
                status, error_text
            )));
        }

        log::info!("Contacts API request successful, status: {}", response.status());
        
        // Получаем текст ответа для логирования
        let response_text = response.text().await?;
        log::info!("Contacts API response body: {}", response_text);
        
        // Пробуем парсить JSON из текста
        match serde_json::from_str::<R>(&response_text) {
            Ok(result) => Ok(result),
            Err(e) => {
                log::error!("Failed to parse response JSON: {}", e);
                log::error!("Response text was: {}", response_text);
                Err(AppError::InvalidInput(format!(
                    "Failed to parse API response: {}",
                    e
                )))
            }
        }
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
    
    pub async fn get_contacts(&self, api_key: &str) -> Result<WazzupContactListResponse, AppError> {
        // Default offset to 0 if not provided
        self.get_contacts_with_offset(api_key, 0).await
    }
    
    pub async fn get_contacts_with_offset(&self, api_key: &str, offset: i32) -> Result<WazzupContactListResponse, AppError> {
        let path = format!("/v3/contacts?offset={}", offset);
        self.request_contacts_api(api_key, Method::GET, &path, None::<&()>).await
    }
    
    pub async fn create_contacts(
        &self,
        api_key: &str,
        contacts: Vec<WazzupContact>,
    ) -> Result<(), AppError> {
        // API Wazzup принимает массив контактов напрямую, не в обертке
        let _: Value = self.request_contacts_api(api_key, Method::POST, "/v3/contacts", Some(&contacts)).await?;
        Ok(())
    }
    
    pub async fn create_contact(
        &self,
        api_key: &str,
        contact: &WazzupContact,
    ) -> Result<WazzupContact, AppError> {
        // Create a single contact by wrapping it in a vector
        let contacts = vec![contact.clone()];
        let _: Value = self.request_contacts_api(api_key, Method::POST, "/v3/contacts", Some(&contacts)).await?;
        // Return the contact that was passed in (API doesn't return the created contact)
        Ok(contact.clone())
    }
    
    #[allow(dead_code)]
    pub async fn get_contact(&self, api_key: &str, contact_id: &str) -> Result<WazzupContact, AppError> {
        let path = format!("/v3/contacts/{}", contact_id);
        self.request_contacts_api(api_key, Method::GET, &path, None::<&()>).await
    }
    
    pub async fn update_contact(
        &self,
        api_key: &str,
        contact_id: &str,
        contact: &WazzupContact,
    ) -> Result<WazzupContact, AppError> {
        let path = format!("/v3/contacts/{}", contact_id);
        let _: Value = self.request_contacts_api(api_key, Method::PUT, &path, Some(contact)).await?;
        // Return the contact that was passed in (API doesn't return the updated contact)
        Ok(contact.clone())
    }
    
    pub async fn delete_contact(&self, api_key: &str, contact_id: &str) -> Result<(), AppError> {
        let path = format!("/v3/contacts/{}", contact_id);
        let _: Value = self.request_contacts_api(api_key, Method::DELETE, &path, None::<&()>).await?;
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
    ) -> Result<String, AppError> {
        self.request_patch_webhooks_string(api_key, "/v3/webhooks", request).await
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

// Contacts - согласно документации Wazzup API
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct WazzupContactData {
    pub chat_type: String,  // whatsapp, telegram, etc.
    pub chat_id: String,    // ID чата в мессенджере
    pub username: Option<String>, // для Telegram
    pub phone: Option<String>,    // для Telegram
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct WazzupContact {
    pub id: String,                           // ID контакта в CRM
    pub responsible_user_id: String,          // ID ответственного пользователя  
    pub name: String,                         // Имя контакта
    pub contact_data: Vec<WazzupContactData>, // Массив контактных данных
    pub uri: Option<String>,                  // Ссылка на контакт в CRM
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct WazzupContactListResponse {
    pub count: i32,
    pub data: Vec<WazzupContact>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateWazzupContactsRequest {
    pub contacts: Vec<WazzupContact>,
}

// API-compatible aliases for the contacts API
// Deprecated type aliases - retained for backward compatibility but will be removed
#[allow(dead_code)]
pub type Contact = WazzupContact;
#[allow(dead_code)]
pub type ContactListResponse = WazzupContactListResponse;
#[allow(dead_code)]
pub type CreateContactRequest = WazzupContact;
#[allow(dead_code)]
pub type UpdateContactRequest = WazzupContact;

// Messages
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageRequest {
    pub chat_id: Option<String>,
    pub channel_id: Option<String>,
    pub sender_id: i64,
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
pub struct WebhookSubscriptions {
    #[serde(rename = "messagesAndStatuses")]
    pub messages_and_statuses: bool,
    #[serde(rename = "contactsAndDealsCreation")]
    pub contacts_and_deals_creation: bool,
    #[serde(rename = "channelsUpdates")]
    pub channels_updates: bool,
    #[serde(rename = "templateStatus")]
    pub template_status: bool,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct WebhookSubscriptionResponse {
    pub ok: bool,
}