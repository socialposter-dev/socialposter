use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;

pub const AUTH_KEYS_RESERVED: [&str; 3] = ["access_token", "refresh_token", "expires_at"];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorLink {
    pub label: Cow<'static, str>,
    pub url: Cow<'static, str>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformAuthor {
    pub name: Cow<'static, str>,
    pub links: Vec<AuthorLink>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectOption {
    pub value: Cow<'static, str>,
    pub label: Cow<'static, str>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum FieldKind {
    Text,
    Secret,
    Url,
    Select {
        options: Vec<SelectOption>,
    },
    OAuthButton {
        button_label: Cow<'static, str>,
        target_key: Cow<'static, str>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthField {
    pub key: Cow<'static, str>,
    pub label: Cow<'static, str>,
    pub placeholder: Cow<'static, str>,
    pub kind: FieldKind,
    pub required: bool,
    pub help_text: Option<Cow<'static, str>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthMode {
    pub id: Cow<'static, str>,
    pub label: Cow<'static, str>,
    pub description: Option<Cow<'static, str>>,
    pub fields: Vec<AuthField>,
    pub setup_instructions: Option<Cow<'static, str>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformCapabilities {
    pub max_chars: usize,
    pub max_images: usize,
    #[serde(default)]
    pub max_image_bytes: Option<usize>,
    pub supports_video: bool,
    pub requires_media: bool,
    #[serde(default)]
    pub supports_threads: bool,
    #[serde(default)]
    pub supports_delete: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformMetadata {
    pub id: String,
    pub name: String,
    pub accent_colour: String,
    pub version: String,
    pub app_version: String,
    pub compatible_app_version: String,
    pub is_compatible: bool,
    pub compatibility_error: Option<String>,
    pub is_official: bool,
    pub authors: Vec<PlatformAuthor>,
    pub auth_modes: Vec<AuthMode>,
    pub active_auth_mode: Option<String>,
    pub setup_instructions: String,
    pub capabilities: PlatformCapabilities,
    pub is_configured: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaAttachment {
    pub path: PathBuf,
    pub mime_type: String,
    #[serde(default)]
    pub alt_text: Option<String>,
    #[serde(skip)]
    pub bytes: Option<Vec<u8>>,
}

impl MediaAttachment {
    /// Returns the in-memory bytes when present (e.g. after compression),
    /// otherwise reads the original file asynchronously from disk.
    pub async fn read_bytes(&self) -> Result<Vec<u8>, String> {
        if let Some(ref data) = self.bytes {
            return Ok(data.clone());
        }
        tokio::fs::read(&self.path)
            .await
            .map_err(|err| format!("Failed to read attachment '{}': {err}", self.path.display()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostPayload {
    pub text: String,
    pub media: Vec<MediaAttachment>,
    #[serde(default)]
    pub reply_root_id: Option<String>,
    #[serde(default)]
    pub reply_parent_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostOutcome {
    pub platform_id: String,
    pub platform_name: String,
    pub success: bool,
    #[serde(default)]
    pub post_id: Option<String>,
    #[serde(default)]
    pub post_ids: Vec<String>,
    pub post_url: Option<String>,
    pub error: Option<String>,
    #[serde(default)]
    pub notes: Vec<String>,
    #[serde(default)]
    pub recalled: bool,
    #[serde(default)]
    pub recalled_at: Option<String>,
    #[serde(default)]
    pub recall_error: Option<String>,
}

#[async_trait]
pub trait SocialPlatform: Send + Sync + 'static {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn accent_colour(&self) -> &str {
        "#64748B"
    }
    /// Semantic version of this platform plugin (e.g., "0.1.0")
    fn version(&self) -> &str {
        "0.1.0"
    }
    /// SemVer requirement for compatible SocialPoster host versions (e.g., "^1.0.0")
    fn compatible_app_version(&self) -> &str {
        ">=1.0.0"
    }
    fn is_official(&self) -> bool {
        false
    }
    fn authors(&self) -> Vec<PlatformAuthor> {
        vec![]
    }
    fn auth_fields(&self) -> Vec<AuthField> {
        vec![]
    }
    fn auth_modes(&self) -> Vec<AuthMode> {
        vec![AuthMode {
            id: Cow::Borrowed("default"),
            label: Cow::Borrowed("Default"),
            description: None,
            fields: self.auth_fields(),
            setup_instructions: None,
        }]
    }
    fn setup_instructions(&self) -> &str;
    fn capabilities(&self) -> PlatformCapabilities;

    async fn verify_credentials(&self, _creds: &HashMap<String, String>) -> Result<(), String> {
        Ok(())
    }

    async fn start_oauth(
        &self,
        _mode_id: &str,
        _inputs: &HashMap<String, String>,
    ) -> Result<HashMap<String, String>, String> {
        Err("OAuth is not supported for this authentication mode.".to_string())
    }

    async fn publish(
        &self,
        post: &PostPayload,
        creds: &HashMap<String, String>,
    ) -> Result<PostOutcome, String>;

    async fn delete_post(
        &self,
        _creds: &HashMap<String, String>,
        _post_id: &str,
    ) -> Result<(), String> {
        Err("Post deletion is not supported by this platform.".into())
    }
}
