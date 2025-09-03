// src/api/http/types.rs
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct AuthRequest {
    pub public_key: String,
    pub episode_id: Option<u64>, // For joining existing episodes
}

#[derive(Serialize)]
pub struct AuthResponse {
    #[serde(serialize_with = "serialize_u64_as_string")]
    pub episode_id: u64,
    pub organizer_public_key: String,
    pub participant_kaspa_address: String,
    pub transaction_id: Option<String>,
    pub status: String,
}

#[derive(Deserialize)]
pub struct ChallengeRequest {
    #[serde(deserialize_with = "deserialize_u64_flexible")]
    pub episode_id: u64,
    // Make optional to avoid 422 when omitted; fallback to episode owner
    pub public_key: Option<String>,
}

fn deserialize_u64_flexible<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{Error as DeError, Unexpected};
    use serde_json::Value;
    let v = Value::deserialize(deserializer)?;
    match v {
        Value::Number(n) => n.as_u64().ok_or_else(|| DeError::invalid_type(Unexpected::Other("non-u64 number"), &"u64")),
        Value::String(s) => s.parse::<u64>().map_err(|_| DeError::invalid_value(Unexpected::Str(&s), &"u64 string")),
        other => Err(DeError::invalid_type(Unexpected::Other(other.to_string().as_str()), &"u64 or string")),
    }
}

#[derive(Serialize)]
pub struct ChallengeResponse {
    #[serde(serialize_with = "serialize_u64_as_string")]
    pub episode_id: u64,
    pub nonce: String,
    pub transaction_id: Option<String>,
    pub status: String,
}

#[derive(Deserialize)]
pub struct VerifyRequest {
    pub episode_id: u64,
    pub signature: String,
    pub nonce: String,
}

#[derive(Serialize)]
pub struct VerifyResponse {
    #[serde(serialize_with = "serialize_u64_as_string")]
    pub episode_id: u64,
    pub authenticated: bool,
    pub status: String,
    pub transaction_id: Option<String>,
}

#[derive(Serialize)]
pub struct EpisodeStatus {
    #[serde(serialize_with = "serialize_u64_as_string")]
    pub episode_id: u64,
    pub authenticated: bool,
    pub status: String,
}

#[derive(Deserialize)]
pub struct RevokeSessionRequest {
    pub episode_id: u64,
    pub session_token: String,
    pub signature: String,
}

#[derive(Serialize)]
pub struct RevokeSessionResponse {
    #[serde(serialize_with = "serialize_u64_as_string")]
    pub episode_id: u64,
    pub transaction_id: String,
    pub status: String,
}

// Comment-related types
#[derive(Deserialize)]
pub struct SubmitCommentRequest {
    pub episode_message: Vec<u8>,
}

// Simple comment submission request (from frontend)
#[derive(Deserialize)]
pub struct SimpleCommentRequest {
    pub episode_id: u64,
    pub text: String,
    pub session_token: String,
}

#[derive(Serialize)]
pub struct SubmitCommentResponse {
    #[serde(serialize_with = "serialize_u64_as_string")]
    pub episode_id: u64,
    #[serde(serialize_with = "serialize_u64_as_string")]
    pub comment_id: u64,
    pub transaction_id: Option<String>,
    pub status: String,
}

#[derive(Deserialize)]
pub struct GetCommentsRequest {
    pub episode_id: u64,
    pub session_token: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct CommentData {
    #[serde(serialize_with = "serialize_u64_as_string")]
    pub id: u64,
    pub text: String,
    pub author: String,
    #[serde(serialize_with = "serialize_u64_as_string")]
    pub timestamp: u64,
    // Removed author_type field - simplified structure
}

#[derive(Serialize)]
pub struct GetCommentsResponse {
    #[serde(serialize_with = "serialize_u64_as_string")]
    pub episode_id: u64,
    pub comments: Vec<CommentData>,
    pub status: String,
}

#[derive(Serialize, Clone)]
pub struct EpisodeInfo {
    pub episode_id: u64,
    pub room_code: String,
    pub creator_public_key: String,
    pub is_authenticated: bool,
}

fn serialize_u64_as_string<S>(value: &u64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&value.to_string())
}

#[derive(Serialize)]
pub struct ListEpisodesResponse {
    pub episodes: Vec<EpisodeInfo>,
}
