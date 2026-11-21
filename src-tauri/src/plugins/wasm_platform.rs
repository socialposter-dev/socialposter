use crate::core::platform::{
    AuthField, AuthMode, AuthorLink, FieldKind, PlatformAuthor, PlatformCapabilities, PostOutcome,
    PostPayload, SelectOption, SocialPlatform,
};
use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use extism::{Manifest, Plugin, Wasm};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

const SHORT_CALL_TIMEOUT: Duration = Duration::from_secs(15);
const PUBLISH_CALL_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
struct WasmAuthorLink {
    label: String,
    url: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
struct WasmAuthor {
    name: String,
    #[serde(default)]
    links: Vec<WasmAuthorLink>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
struct WasmSelectOption {
    value: String,
    label: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
struct WasmAuthField {
    key: String,
    label: String,
    #[serde(default)]
    placeholder: String,
    #[serde(default = "default_field_kind")]
    kind: String,
    #[serde(default = "default_true")]
    required: bool,
    #[serde(default)]
    help_text: Option<String>,
    #[serde(default)]
    options: Vec<WasmSelectOption>,
    #[serde(default)]
    button_label: Option<String>,
    #[serde(default)]
    target_key: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
struct WasmAuthMode {
    id: String,
    label: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    fields: Vec<WasmAuthField>,
    #[serde(default)]
    setup_instructions: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WasmDescribeOutput {
    pub id: String,
    #[serde(alias = "name")]
    pub display_name: String,
    #[serde(
        default = "default_brand_colour",
        alias = "brand_color",
        alias = "accent_colour",
        alias = "accent_color"
    )]
    pub brand_colour: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default = "default_compatible_app_version")]
    pub compatible_app_version: String,
    #[serde(default)]
    pub icon_svg: Option<String>,
    #[serde(default = "default_max_chars", alias = "max_chars")]
    pub max_characters: usize,
    #[serde(default = "default_max_attachments", alias = "max_images")]
    pub max_attachments: usize,
    #[serde(default)]
    pub max_image_bytes: Option<usize>,
    #[serde(default)]
    pub supported_media_types: Vec<String>,
    #[serde(default)]
    pub supports_alt_text: bool,
    #[serde(default)]
    pub supports_video: bool,
    #[serde(default)]
    pub requires_media: bool,
    #[serde(default)]
    pub supports_threads: bool,
    #[serde(default)]
    pub supports_delete: bool,
    #[serde(default)]
    pub allowed_domains: Vec<String>,
    #[serde(default)]
    pub setup_instructions: Option<String>,
    #[serde(default)]
    authors: Vec<WasmAuthor>,
    #[serde(default)]
    auth_modes: Vec<WasmAuthMode>,
    #[serde(default)]
    credential_fields: Vec<WasmAuthField>,
}

fn default_field_kind() -> String {
    "text".to_string()
}

fn default_true() -> bool {
    true
}

fn default_brand_colour() -> String {
    "#64748B".to_string()
}

fn default_version() -> String {
    "0.1.0".to_string()
}

fn default_compatible_app_version() -> String {
    ">=1.0.0".to_string()
}

fn default_max_chars() -> usize {
    500
}

fn default_max_attachments() -> usize {
    4
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
struct WasmMediaInput {
    filename: String,
    mime_type: String,
    alt_text: Option<String>,
    data_base64: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
struct WasmPublishInput<'a> {
    text: &'a str,
    media: Vec<WasmMediaInput>,
    credentials: &'a HashMap<String, String>,
    reply_root_id: Option<&'a str>,
    reply_parent_id: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct WasmValidateAuthOutput {
    #[serde(default = "default_true")]
    ok: bool,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct WasmPublishOutput {
    #[serde(default)]
    ok: Option<bool>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    post_id: Option<String>,
    #[serde(default)]
    post_url: Option<String>,
    #[serde(default)]
    notes: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct WasmDeleteOutput {
    #[serde(default = "default_true")]
    ok: bool,
    #[serde(default)]
    error: Option<String>,
}

fn map_auth_field(field: &WasmAuthField) -> AuthField {
    let kind = match field.kind.trim().to_ascii_lowercase().as_str() {
        "secret" | "password" => FieldKind::Secret,
        "url" => FieldKind::Url,
        "select" => FieldKind::Select {
            options: field
                .options
                .iter()
                .map(|o| SelectOption {
                    value: Cow::Owned(o.value.clone()),
                    label: Cow::Owned(o.label.clone()),
                })
                .collect(),
        },
        "oauth" | "oauth_button" => FieldKind::OAuthButton {
            button_label: Cow::Owned(
                field
                    .button_label
                    .clone()
                    .unwrap_or_else(|| "Connect".to_string()),
            ),
            target_key: Cow::Owned(
                field
                    .target_key
                    .clone()
                    .unwrap_or_else(|| field.key.clone()),
            ),
        },
        _ => FieldKind::Text,
    };

    AuthField {
        key: Cow::Owned(field.key.clone()),
        label: Cow::Owned(field.label.clone()),
        placeholder: Cow::Owned(field.placeholder.clone()),
        kind,
        required: field.required,
        help_text: field.help_text.clone().map(Cow::Owned),
    }
}

fn build_sandbox_manifest(
    wasm_path: &Path,
    allowed_domains: &[String],
    timeout: Duration,
) -> Manifest {
    let mut manifest = Manifest::new([Wasm::file(wasm_path)]).with_timeout(timeout);

    // Leave allowed_paths completely empty so the plugin cannot access the host filesystem.
    manifest.allowed_paths = None;

    if !allowed_domains.is_empty() {
        manifest = manifest.with_allowed_hosts(allowed_domains.iter().cloned());
    }

    manifest
}

fn call_wasm_export(
    wasm_path: &Path,
    allowed_domains: &[String],
    timeout: Duration,
    export_name: &str,
    input_json: &str,
) -> Result<String, String> {
    let manifest = build_sandbox_manifest(wasm_path, allowed_domains, timeout);
    let mut plugin = Plugin::new(&manifest, [], true)
        .map_err(|err| format!("Failed to initialise WebAssembly sandbox: {err}"))?;

    plugin
        .call::<&str, String>(export_name, input_json)
        .map_err(|err| format!("Plugin '{export_name}' execution failed: {err}"))
}

pub struct WasmPlatform {
    wasm_path: PathBuf,
    describe: WasmDescribeOutput,
    is_official: bool,
    setup_instructions_cached: String,
    auth_modes_cached: Vec<AuthMode>,
    authors_cached: Vec<PlatformAuthor>,
    capabilities_cached: PlatformCapabilities,
}

impl WasmPlatform {
    /// Loads a `.wasm` plugin inside a zero-network, zero-filesystem sandbox to inspect
    /// its `describe` export and cache metadata for the host registry.
    pub fn from_wasm_file(wasm_path: &Path) -> Result<Self, String> {
        let raw = call_wasm_export(wasm_path, &[], SHORT_CALL_TIMEOUT, "describe", "")?;
        let describe: WasmDescribeOutput = serde_json::from_str(&raw)
            .map_err(|err| format!("Invalid plugin describe metadata: {err}"))?;

        if describe.id.trim().is_empty() {
            return Err("Plugin describe output returned an empty id.".to_string());
        }

        let auth_modes_cached = if !describe.auth_modes.is_empty() {
            describe
                .auth_modes
                .iter()
                .map(|m| AuthMode {
                    id: Cow::Owned(m.id.clone()),
                    label: Cow::Owned(m.label.clone()),
                    description: m.description.clone().map(Cow::Owned),
                    fields: m.fields.iter().map(map_auth_field).collect(),
                    setup_instructions: m.setup_instructions.clone().map(Cow::Owned),
                })
                .collect()
        } else {
            vec![AuthMode {
                id: Cow::Borrowed("default"),
                label: Cow::Borrowed("Default"),
                description: None,
                fields: describe
                    .credential_fields
                    .iter()
                    .map(map_auth_field)
                    .collect(),
                setup_instructions: describe.setup_instructions.clone().map(Cow::Owned),
            }]
        };

        let authors_cached = describe
            .authors
            .iter()
            .map(|a| PlatformAuthor {
                name: Cow::Owned(a.name.clone()),
                links: a
                    .links
                    .iter()
                    .map(|l| AuthorLink {
                        label: Cow::Owned(l.label.clone()),
                        url: Cow::Owned(l.url.clone()),
                    })
                    .collect(),
            })
            .collect();

        let supports_video = describe.supports_video
            || describe
                .supported_media_types
                .iter()
                .any(|m| m.starts_with("video/"));

        let capabilities_cached = PlatformCapabilities {
            max_chars: describe.max_characters,
            max_images: describe.max_attachments,
            max_image_bytes: describe.max_image_bytes,
            supports_video,
            requires_media: describe.requires_media,
            supports_threads: describe.supports_threads,
            supports_delete: describe.supports_delete,
        };

        let setup_instructions_cached = describe
            .setup_instructions
            .clone()
            .unwrap_or_else(|| format!("Configure your {} credentials.", describe.display_name));

        Ok(Self {
            wasm_path: wasm_path.to_path_buf(),
            describe,
            is_official: false,
            setup_instructions_cached,
            auth_modes_cached,
            authors_cached,
            capabilities_cached,
        })
    }

    pub fn with_official(mut self, official: bool) -> Self {
        self.is_official = official;
        self
    }

    pub fn describe(&self) -> &WasmDescribeOutput {
        &self.describe
    }
}

#[async_trait]
impl SocialPlatform for WasmPlatform {
    fn id(&self) -> &str {
        &self.describe.id
    }

    fn name(&self) -> &str {
        &self.describe.display_name
    }

    fn accent_colour(&self) -> &str {
        &self.describe.brand_colour
    }

    fn version(&self) -> &str {
        &self.describe.version
    }

    fn compatible_app_version(&self) -> &str {
        &self.describe.compatible_app_version
    }

    fn is_official(&self) -> bool {
        self.is_official
    }

    fn authors(&self) -> Vec<PlatformAuthor> {
        self.authors_cached.clone()
    }

    fn auth_modes(&self) -> Vec<AuthMode> {
        self.auth_modes_cached.clone()
    }

    fn setup_instructions(&self) -> &str {
        &self.setup_instructions_cached
    }

    fn capabilities(&self) -> PlatformCapabilities {
        self.capabilities_cached
    }

    async fn verify_credentials(&self, creds: &HashMap<String, String>) -> Result<(), String> {
        let wasm_path = self.wasm_path.clone();
        let allowed_domains = self.describe.allowed_domains.clone();
        let input_json = serde_json::to_string(&serde_json::json!({
            "credentials": creds,
        }))
        .map_err(|err| format!("Failed to serialise credentials for plugin: {err}"))?;

        let raw = tokio::task::spawn_blocking(move || {
            call_wasm_export(
                &wasm_path,
                &allowed_domains,
                SHORT_CALL_TIMEOUT,
                "validate_auth",
                &input_json,
            )
        })
        .await
        .map_err(|err| format!("Plugin verification task failed: {err}"))??;

        if raw.trim().is_empty() {
            return Ok(());
        }

        let parsed: WasmValidateAuthOutput = serde_json::from_str(&raw)
            .map_err(|err| format!("Invalid validate_auth response from plugin: {err}"))?;

        if parsed.ok {
            Ok(())
        } else {
            Err(parsed
                .error
                .unwrap_or_else(|| "Credential verification failed.".to_string()))
        }
    }

    async fn publish(
        &self,
        post: &PostPayload,
        creds: &HashMap<String, String>,
    ) -> Result<PostOutcome, String> {
        let mut encoded_media = Vec::with_capacity(post.media.len());
        for attachment in &post.media {
            let bytes = attachment.read_bytes().await?;
            let filename = attachment
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("attachment")
                .to_string();
            encoded_media.push(WasmMediaInput {
                filename,
                mime_type: attachment.mime_type.clone(),
                alt_text: attachment.alt_text.clone(),
                data_base64: STANDARD.encode(bytes),
            });
        }

        let input = WasmPublishInput {
            text: &post.text,
            media: encoded_media,
            credentials: creds,
            reply_root_id: post.reply_root_id.as_deref(),
            reply_parent_id: post.reply_parent_id.as_deref(),
        };
        let input_json = serde_json::to_string(&input)
            .map_err(|err| format!("Failed to serialise publish payload for plugin: {err}"))?;

        let wasm_path = self.wasm_path.clone();
        let allowed_domains = self.describe.allowed_domains.clone();
        let raw = tokio::task::spawn_blocking(move || {
            call_wasm_export(
                &wasm_path,
                &allowed_domains,
                PUBLISH_CALL_TIMEOUT,
                "publish",
                &input_json,
            )
        })
        .await
        .map_err(|err| format!("Plugin publish task failed: {err}"))??;

        let parsed: WasmPublishOutput = serde_json::from_str(&raw)
            .map_err(|err| format!("Invalid publish response from plugin: {err}"))?;

        if parsed.ok == Some(false) {
            return Err(parsed
                .error
                .unwrap_or_else(|| "Plugin publish failed.".to_string()));
        }

        let post_ids = parsed.post_id.iter().cloned().collect();
        Ok(PostOutcome {
            platform_id: self.id().to_string(),
            platform_name: self.name().to_string(),
            success: true,
            post_id: parsed.post_id,
            post_ids,
            post_url: parsed.post_url,
            notes: parsed.notes,
            ..Default::default()
        })
    }

    async fn delete_post(
        &self,
        creds: &HashMap<String, String>,
        post_id: &str,
    ) -> Result<(), String> {
        if !self.capabilities_cached.supports_delete {
            return Err("Post deletion is not supported by this platform.".to_string());
        }

        let wasm_path = self.wasm_path.clone();
        let allowed_domains = self.describe.allowed_domains.clone();
        let input_json = serde_json::to_string(&serde_json::json!({
            "credentials": creds,
            "post_id": post_id,
        }))
        .map_err(|err| format!("Failed to serialise delete request for plugin: {err}"))?;

        let raw = tokio::task::spawn_blocking(move || {
            call_wasm_export(
                &wasm_path,
                &allowed_domains,
                SHORT_CALL_TIMEOUT,
                "delete_post",
                &input_json,
            )
        })
        .await
        .map_err(|err| format!("Plugin delete task failed: {err}"))??;

        if raw.trim().is_empty() {
            return Ok(());
        }

        let parsed: WasmDeleteOutput = serde_json::from_str(&raw)
            .map_err(|err| format!("Invalid delete_post response from plugin: {err}"))?;

        if parsed.ok {
            Ok(())
        } else {
            Err(parsed
                .error
                .unwrap_or_else(|| "Plugin post deletion failed.".to_string()))
        }
    }
}
