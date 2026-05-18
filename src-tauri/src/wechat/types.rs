use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot_agent: Option<String>,
}

pub const MESSAGE_TYPE_NONE: i32 = 0;
pub const MESSAGE_TYPE_USER: i32 = 1;
pub const MESSAGE_TYPE_BOT: i32 = 2;

pub const MESSAGE_ITEM_TYPE_NONE: i32 = 0;
pub const MESSAGE_ITEM_TYPE_TEXT: i32 = 1;
pub const MESSAGE_ITEM_TYPE_IMAGE: i32 = 2;
pub const MESSAGE_ITEM_TYPE_VOICE: i32 = 3;
pub const MESSAGE_ITEM_TYPE_FILE: i32 = 4;
pub const MESSAGE_ITEM_TYPE_VIDEO: i32 = 5;

pub const MESSAGE_STATE_NEW: i32 = 0;
pub const MESSAGE_STATE_GENERATING: i32 = 1;
pub const MESSAGE_STATE_FINISH: i32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdnMedia {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encrypt_query_param: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aes_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encrypt_type: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<CdnMedia>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_media: Option<CdnMedia>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aeskey: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mid_size: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_size: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_height: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_width: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hd_size: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<CdnMedia>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encode_type: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bits_per_sample: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playtime: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<CdnMedia>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub md5: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub len: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<CdnMedia>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_size: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub play_length: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_md5: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_media: Option<CdnMedia>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_size: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_height: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_width: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_item: Option<Box<MessageItem>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_time_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_time_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_completed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub msg_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ref_msg: Option<RefMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_item: Option<TextItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_item: Option<ImageItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice_item: Option<VoiceItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_item: Option<FileItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_item: Option<VideoItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeixinMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seq: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_time_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_time_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete_time_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_type: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_state: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_list: Option<Vec<MessageItem>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetUpdatesReq {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sync_buf: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub get_updates_buf: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_info: Option<BaseInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetUpdatesResp {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ret: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errcode: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errmsg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub msgs: Option<Vec<WeixinMessage>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sync_buf: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub get_updates_buf: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub longpolling_timeout_ms: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMessageReq {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub msg: Option<WeixinMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_info: Option<BaseInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMessageResp {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ret: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errmsg: Option<String>,
}

pub const TYPING_STATUS_TYPING: i32 = 1;
pub const TYPING_STATUS_CANCEL: i32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendTypingReq {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ilink_user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub typing_ticket: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_info: Option<BaseInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendTypingResp {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ret: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errmsg: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetConfigReq {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ilink_user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_info: Option<BaseInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetConfigResp {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ret: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errmsg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub typing_ticket: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotifyStopReq {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_info: Option<BaseInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotifyStopResp {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ret: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errmsg: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotifyStartReq {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_info: Option<BaseInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotifyStartResp {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ret: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errmsg: Option<String>,
}
