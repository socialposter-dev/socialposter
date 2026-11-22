use crate::plugins::wasm_platform::WasmPlatform;
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
use chrono::Utc;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use reqwest::Client;
use reqwest::header::{ETAG, IF_NONE_MATCH};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

pub const REGISTRY_URL: &str = "https://cdn.socialposter.dev/registry.json";

/// Pinned SocialPoster Hub Ed25519 public key (base64-encoded 32 bytes).
/// Override via `SOCIALPOSTER_HUB_PUBLIC_KEY` for local registry testing.
pub const DEFAULT_HUB_PUBLIC_KEY_B64: &str = "snOcIevOJOkcz1eiHGKgxFoQmLPAxILaKjo0L0T77pc=";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RegistryAuthor {
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default, alias = "name")]
    pub display_name: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub is_official: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RegistryPluginEntry {
    #[serde(alias = "id")]
    pub slug: String,
    #[serde(alias = "name")]
    pub display_name: String,
    #[serde(default, alias = "description")]
    pub summary: Option<String>,
    #[serde(default)]
    pub author: Option<RegistryAuthor>,
    #[serde(default, alias = "brand_color", alias = "accent_colour")]
    pub brand_colour: Option<String>,
    #[serde(default, alias = "version")]
    pub latest_version: String,
    #[serde(default)]
    pub is_deprecated: bool,
    #[serde(default)]
    pub deprecated_at: Option<String>,
    #[serde(default)]
    pub deprecation_reason: Option<String>,
    #[serde(default)]
    pub manifest_url: Option<String>,
    #[serde(default, alias = "iconSvg")]
    pub icon_svg: Option<String>,
    #[serde(default)]
    pub allowed_domains: Vec<String>,
    #[serde(default, alias = "official")]
    pub is_official: bool,
    #[serde(default)]
    pub supports_threads: bool,
    #[serde(default)]
    pub supports_alt_text: bool,
    #[serde(default)]
    pub supports_delete: bool,
    #[serde(default)]
    pub compatible_app_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RegistryIndex {
    #[serde(default = "default_schema_version", alias = "version")]
    pub schema_version: u32,
    #[serde(default)]
    pub generated_at: Option<String>,
    #[serde(default)]
    pub hub_public_key: Option<String>,
    #[serde(default)]
    pub plugins: Vec<RegistryPluginEntry>,
    #[serde(default)]
    pub signature: Option<String>,
}

fn default_schema_version() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PluginVersionEntry {
    pub version: String,
    #[serde(alias = "wasm_url")]
    pub download_url: String,
    pub sha256: String,
    #[serde(
        default,
        alias = "wasm_signature",
        alias = "artefact_signature",
        alias = "signature"
    )]
    pub ed25519_signature: Option<String>,
    #[serde(default)]
    pub file_size_bytes: Option<u64>,
    #[serde(default)]
    pub release_notes: Option<String>,
    #[serde(default)]
    pub published_at: Option<String>,
    #[serde(default)]
    pub compatible_app_version: Option<String>,
    #[serde(default)]
    pub is_compatible: bool,
    #[serde(default)]
    pub compatibility_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ManifestCredentialField {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub secret: bool,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub placeholder: Option<String>,
    #[serde(default)]
    pub help_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ManifestCapabilities {
    #[serde(default)]
    pub max_characters: Option<usize>,
    #[serde(default)]
    pub max_attachments: Option<usize>,
    #[serde(default)]
    pub supported_media_types: Vec<String>,
    #[serde(default)]
    pub supports_alt_text: bool,
    #[serde(default)]
    pub supports_threads: bool,
    #[serde(default)]
    pub supports_delete: bool,
    #[serde(default)]
    pub allowed_domains: Vec<String>,
    #[serde(default)]
    pub credential_fields: Vec<ManifestCredentialField>,
    #[serde(default)]
    pub auth_modes: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ManifestIconContainer {
    #[serde(default, alias = "iconSvg")]
    pub icon_svg: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PluginManifest {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub generated_at: Option<String>,
    #[serde(default)]
    pub hub_public_key: Option<String>,
    #[serde(alias = "id")]
    pub slug: String,
    #[serde(default, alias = "name")]
    pub display_name: Option<String>,
    #[serde(default, alias = "description")]
    pub summary: Option<String>,
    #[serde(default)]
    pub author: Option<RegistryAuthor>,
    #[serde(default)]
    pub authors: Vec<RegistryAuthor>,
    #[serde(default, alias = "repo", alias = "source_url")]
    pub repository: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default, alias = "official")]
    pub is_official: bool,
    #[serde(default)]
    pub brand_colour: Option<String>,
    #[serde(default, alias = "iconSvg")]
    pub icon_svg: Option<String>,
    #[serde(default)]
    pub describe: Option<ManifestIconContainer>,
    #[serde(default)]
    pub metadata: Option<ManifestIconContainer>,
    #[serde(default)]
    pub setup_instructions: Option<String>,
    #[serde(default)]
    pub is_deprecated: bool,
    #[serde(default)]
    pub deprecated_at: Option<String>,
    #[serde(default)]
    pub deprecation_reason: Option<String>,
    #[serde(default)]
    pub capabilities: Option<ManifestCapabilities>,
    #[serde(default)]
    pub allowed_domains: Vec<String>,
    #[serde(default)]
    pub latest_version: Option<String>,
    #[serde(default)]
    pub versions: Vec<PluginVersionEntry>,
    #[serde(default)]
    pub compatible_app_version: Option<String>,
    #[serde(default)]
    pub signature: Option<String>,
}

impl PluginManifest {
    pub fn resolved_icon_svg(&self) -> Option<String> {
        self.icon_svg
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
            .or_else(|| {
                self.describe
                    .as_ref()
                    .and_then(|d| d.icon_svg.as_deref())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(ToString::to_string)
            })
            .or_else(|| {
                self.metadata
                    .as_ref()
                    .and_then(|m| m.icon_svg.as_deref())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(ToString::to_string)
            })
    }
}

pub fn is_official_plugin(explicit: bool, author: Option<&RegistryAuthor>) -> bool {
    if explicit {
        return true;
    }
    let Some(a) = author else {
        return false;
    };
    if a.is_official {
        return true;
    }
    a.slug
        .as_deref()
        .is_some_and(|s| s.eq_ignore_ascii_case("socialposter") || s.eq_ignore_ascii_case("afuxy"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifestDetail {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub author: Option<RegistryAuthor>,
    pub is_official: bool,
    pub brand_colour: String,
    pub icon_svg: Option<String>,
    pub setup_instructions: Option<String>,
    pub app_version: String,
    pub compatible_app_version: String,
    pub is_compatible: bool,
    pub compatibility_error: Option<String>,
    pub latest_version: String,
    pub installed_version: Option<String>,
    pub is_installed: bool,
    pub update_available: bool,
    pub enabled: bool,
    pub verified: bool,
    pub allowed_domains: Vec<String>,
    pub capabilities: ManifestCapabilities,
    pub versions: Vec<PluginVersionEntry>,
    pub hub_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledPluginRecord {
    pub id: String,
    pub name: String,
    pub version: String,
    pub wasm_path: String,
    pub enabled: bool,
    pub verified: bool,
    #[serde(default)]
    pub is_official: bool,
    pub source: String,
    pub sha256: String,
    pub installed_at: String,
    #[serde(default)]
    pub allowed_domains: Vec<String>,
    #[serde(default)]
    pub brand_colour: Option<String>,
    #[serde(default)]
    pub icon_svg: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryPluginView {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub latest_version: Option<String>,
    pub installed_version: Option<String>,
    pub is_installed: bool,
    pub update_available: bool,
    pub enabled: bool,
    pub verified: bool,
    #[serde(default)]
    pub is_official: bool,
    pub source: String,
    pub brand_colour: String,
    pub icon_svg: Option<String>,
    pub allowed_domains: Vec<String>,
    pub authors: Vec<String>,
    pub updated_at: Option<String>,
    pub app_version: String,
    pub compatible_app_version: String,
    pub is_compatible: bool,
    pub compatibility_error: Option<String>,
}

fn plugins_root_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|err| format!("Failed to resolve app data directory: {err}"))?
        .join("plugins");
    fs::create_dir_all(&dir).map_err(|err| format!("Failed to create plugins directory: {err}"))?;
    Ok(dir)
}

fn cache_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = plugins_root_dir(app)?.join("cache");
    fs::create_dir_all(&dir)
        .map_err(|err| format!("Failed to create plugin cache directory: {err}"))?;
    Ok(dir)
}

fn manifest_cache_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = cache_dir(app)?.join("manifests");
    fs::create_dir_all(&dir)
        .map_err(|err| format!("Failed to create manifest cache directory: {err}"))?;
    Ok(dir)
}

pub fn load_cached_manifest_icon(app: &AppHandle, plugin_id: &str) -> Option<String> {
    let safe_id: String = plugin_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let path = manifest_cache_dir(app)
        .ok()?
        .join(format!("{safe_id}.json"));
    let raw = fs::read_to_string(path).ok()?;
    let manifest: PluginManifest = serde_json::from_str(&raw).ok()?;
    manifest.resolved_icon_svg()
}

/// Resolves author attributions from a plugin's cached Hub manifest (`author`, `authors`,
/// and `repository` / `homepage` links) for plugins whose Wasm `describe()` omits `authors`.
pub fn load_cached_manifest_authors(
    app: &AppHandle,
    plugin_id: &str,
) -> Vec<crate::core::platform::PlatformAuthor> {
    use crate::core::platform::{AuthorLink, PlatformAuthor};
    use std::borrow::Cow;

    let safe_id: String = plugin_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let Ok(dir) = manifest_cache_dir(app) else {
        return Vec::new();
    };
    let path = dir.join(format!("{safe_id}.json"));
    let Ok(raw) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(manifest) = serde_json::from_str::<PluginManifest>(&raw) else {
        return Vec::new();
    };

    let mut out: Vec<PlatformAuthor> = Vec::new();
    let repo_url = manifest
        .repository
        .as_deref()
        .or(manifest.homepage.as_deref())
        .map(str::trim)
        .filter(|s| s.starts_with("https://"))
        .map(ToString::to_string);

    let mut push_author = |author: &RegistryAuthor| {
        let name = author.display_name.trim();
        if name.is_empty() || out.iter().any(|a| a.name.eq_ignore_ascii_case(name)) {
            return;
        }
        let mut links = Vec::new();
        if let Some(url) = author
            .url
            .as_deref()
            .map(str::trim)
            .filter(|s| s.starts_with("https://"))
        {
            links.push(AuthorLink {
                label: Cow::Borrowed("Website"),
                url: Cow::Owned(url.to_string()),
            });
        }
        if let Some(ref repo) = repo_url
            && !links.iter().any(|l| l.url.as_ref() == repo.as_str())
        {
            links.push(AuthorLink {
                label: Cow::Borrowed("Repository"),
                url: Cow::Owned(repo.clone()),
            });
        }
        out.push(PlatformAuthor {
            name: Cow::Owned(name.to_string()),
            links,
        });
    };

    if let Some(ref primary) = manifest.author {
        push_author(primary);
    }
    for extra in &manifest.authors {
        push_author(extra);
    }

    out
}

fn installed_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = plugins_root_dir(app)?.join("installed");
    fs::create_dir_all(&dir)
        .map_err(|err| format!("Failed to create installed plugins directory: {err}"))?;
    Ok(dir)
}

fn installed_index_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(plugins_root_dir(app)?.join("installed.json"))
}

pub fn load_installed_records(app: &AppHandle) -> Result<Vec<InstalledPluginRecord>, String> {
    let path = installed_index_path(app)?;
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(&path)
        .map_err(|err| format!("Failed to read installed plugins index: {err}"))?;
    serde_json::from_str(&raw)
        .map_err(|err| format!("Failed to parse installed plugins index: {err}"))
}

pub fn save_installed_records(
    app: &AppHandle,
    records: &[InstalledPluginRecord],
) -> Result<(), String> {
    let path = installed_index_path(app)?;
    let json = serde_json::to_string_pretty(records)
        .map_err(|err| format!("Failed to serialise installed plugins index: {err}"))?;
    fs::write(&path, json).map_err(|err| format!("Failed to write installed plugins index: {err}"))
}

fn decode_hex_or_base64(raw: &str, expected_len: usize, label: &str) -> Result<Vec<u8>, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(format!("{label} is empty."));
    }

    // Check exact hex length before decoding so 128-char hex signatures and 64-char hex keys
    // are always parsed as hex and never misrouted to Base64.
    if trimmed.len() == expected_len * 2 {
        if !trimmed.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(format!(
                "Invalid {label} hex encoding: expected {expected_len_hex} hexadecimal characters.",
                expected_len_hex = expected_len * 2
            ));
        }
        let mut out = Vec::with_capacity(expected_len);
        for i in (0..trimmed.len()).step_by(2) {
            let byte = u8::from_str_radix(&trimmed[i..i + 2], 16)
                .map_err(|err| format!("Invalid {label} hex encoding: {err}"))?;
            out.push(byte);
        }
        return Ok(out);
    }

    let b64 = STANDARD
        .decode(trimmed)
        .or_else(|_| STANDARD_NO_PAD.decode(trimmed))
        .or_else(|_| URL_SAFE.decode(trimmed))
        .or_else(|_| URL_SAFE_NO_PAD.decode(trimmed))
        .map_err(|err| format!("Invalid {label} encoding (expected hex or Base64): {err}"))?;

    if b64.len() != expected_len {
        return Err(format!(
            "{label} must be exactly {expected_len} bytes (got {}).",
            b64.len()
        ));
    }
    Ok(b64)
}

pub fn hub_public_key() -> Result<VerifyingKey, String> {
    let raw = std::env::var("SOCIALPOSTER_HUB_PUBLIC_KEY")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_HUB_PUBLIC_KEY_B64.to_string());

    let bytes = decode_hex_or_base64(&raw, 32, "Hub Ed25519 public key")?;
    let key_bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| "Hub Ed25519 public key must be exactly 32 bytes.".to_string())?;

    VerifyingKey::from_bytes(&key_bytes)
        .map_err(|err| format!("Invalid Hub Ed25519 public key: {err}"))
}

fn verify_announced_hub_key(
    announced: Option<&str>,
    pinned: &VerifyingKey,
    context: &str,
) -> Result<(), String> {
    let Some(raw) = announced.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(());
    };
    let bytes = decode_hex_or_base64(raw, 32, "announced Hub Ed25519 public key")?;
    if bytes.as_slice() != pinned.as_bytes() {
        return Err(format!(
            "{context} Hub public key does not match the pinned SocialPoster Hub key."
        ));
    }
    Ok(())
}

pub fn compute_sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn verify_ed25519_signature(
    verifying_key: &VerifyingKey,
    message: &[u8],
    signature_raw: &str,
) -> Result<(), String> {
    let sig_bytes = decode_hex_or_base64(signature_raw, 64, "Ed25519 signature")?;
    let sig_array: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| "Ed25519 signature must be exactly 64 bytes.".to_string())?;
    let signature = Signature::from_bytes(&sig_array);

    verifying_key
        .verify(message, &signature)
        .map_err(|_| "Ed25519 signature verification failed.".to_string())
}

/// Verifies both the SHA-256 digest and the Ed25519 signature of a downloaded `.wasm` artefact.
///
/// The SocialPoster Hub signs the canonical payload `"{slug}:{version}:{sha256}"` (lowercase hex).
pub fn verify_wasm_artefact(
    plugin_slug: &str,
    version: &str,
    wasm_bytes: &[u8],
    expected_sha256_hex: &str,
    signature_raw: Option<&str>,
    verifying_key: &VerifyingKey,
) -> Result<String, String> {
    let signature_trimmed = signature_raw
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            format!("Plugin '{plugin_slug}' v{version} is missing its Ed25519 signature field.")
        })?;

    let actual_digest: [u8; 32] = Sha256::digest(wasm_bytes).into();
    let actual_hex: String = actual_digest.iter().map(|b| format!("{b:02x}")).collect();
    if !actual_hex.eq_ignore_ascii_case(expected_sha256_hex.trim()) {
        return Err(format!(
            "SHA-256 digest mismatch for '{plugin_slug}' v{version} (expected {}, got {actual_hex}).",
            expected_sha256_hex.trim()
        ));
    }

    let sig_bytes =
        decode_hex_or_base64(signature_trimmed, 64, "Ed25519 signature").map_err(|err| {
            format!("Malformed Ed25519 signature for '{plugin_slug}' v{version}: {err}")
        })?;
    let sig_array: [u8; 64] = sig_bytes.try_into().map_err(|_| {
        format!("Malformed Ed25519 signature for '{plugin_slug}' v{version}: expected 64 bytes.")
    })?;
    let signature = Signature::from_bytes(&sig_array);

    // Hub canonical signing payload: "{slug}:{version}:{sha256}".
    // Also accept raw WASM bytes, SHA-256 hex, or raw 32-byte digest for backward compatibility.
    let canonical_payload = format!("{plugin_slug}:{version}:{actual_hex}");
    let sig_ok = verifying_key
        .verify(canonical_payload.as_bytes(), &signature)
        .is_ok()
        || verifying_key.verify(wasm_bytes, &signature).is_ok()
        || verifying_key
            .verify(actual_hex.as_bytes(), &signature)
            .is_ok()
        || verifying_key.verify(&actual_digest, &signature).is_ok();

    if !sig_ok {
        return Err(format!(
            "Ed25519 signature verification failed for '{plugin_slug}' v{version}: signature does not match the pinned Hub public key."
        ));
    }

    Ok(actual_hex)
}

pub async fn fetch_registry_index(app: &AppHandle) -> Result<RegistryIndex, String> {
    let cache = cache_dir(app)?;
    let cached_json_path = cache.join("registry.json");
    let cached_etag_path = cache.join("registry.etag");

    let cached_etag = fs::read_to_string(&cached_etag_path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let pinned_key = hub_public_key()?;
    let client = Client::new();
    let mut req = client.get(REGISTRY_URL);
    if let Some(ref etag) = cached_etag {
        req = req.header(IF_NONE_MATCH, etag);
    }

    match req.send().await {
        Ok(res) if res.status() == reqwest::StatusCode::NOT_MODIFIED => {
            let raw = fs::read_to_string(&cached_json_path)
                .map_err(|err| format!("Failed to read cached plugin registry: {err}"))?;
            let index: RegistryIndex = serde_json::from_str(&raw)
                .map_err(|err| format!("Failed to parse cached plugin registry: {err}"))?;
            verify_announced_hub_key(index.hub_public_key.as_deref(), &pinned_key, "Registry")?;
            Ok(index)
        }
        Ok(res) if res.status().is_success() => {
            let next_etag = res
                .headers()
                .get(ETAG)
                .and_then(|v| v.to_str().ok())
                .map(ToString::to_string);
            let raw = res
                .text()
                .await
                .map_err(|err| format!("Failed to read plugin registry response: {err}"))?;

            let index: RegistryIndex = serde_json::from_str(&raw)
                .map_err(|err| format!("Failed to parse plugin registry index: {err}"))?;

            verify_announced_hub_key(index.hub_public_key.as_deref(), &pinned_key, "Registry")?;

            if let Some(ref sig) = index.signature {
                let mut unsigned = index.clone();
                unsigned.signature = None;
                let canonical = serde_json::to_vec(&unsigned).map_err(|err| {
                    format!("Failed to serialise registry for verification: {err}")
                })?;
                verify_ed25519_signature(&pinned_key, &canonical, sig)?;
            }

            let _ = fs::write(&cached_json_path, &raw);
            if let Some(etag) = next_etag {
                let _ = fs::write(&cached_etag_path, etag);
            }
            Ok(index)
        }
        Ok(res) => {
            let status = res.status();
            if cached_json_path.is_file() {
                let raw = fs::read_to_string(&cached_json_path)
                    .map_err(|err| format!("Failed to read cached plugin registry: {err}"))?;
                let index: RegistryIndex = serde_json::from_str(&raw)
                    .map_err(|err| format!("Failed to parse cached plugin registry: {err}"))?;
                verify_announced_hub_key(index.hub_public_key.as_deref(), &pinned_key, "Registry")?;
                Ok(index)
            } else {
                Err(format!("Plugin registry returned HTTP {status}."))
            }
        }
        Err(err) => {
            if cached_json_path.is_file() {
                let raw = fs::read_to_string(&cached_json_path).map_err(|read_err| {
                    format!("Failed to read cached plugin registry: {read_err}")
                })?;
                let index: RegistryIndex = serde_json::from_str(&raw).map_err(|parse_err| {
                    format!("Failed to parse cached plugin registry: {parse_err}")
                })?;
                verify_announced_hub_key(index.hub_public_key.as_deref(), &pinned_key, "Registry")?;
                Ok(index)
            } else {
                Err(format!("Unable to reach plugin registry: {err}"))
            }
        }
    }
}

pub async fn fetch_plugin_manifest(
    app: &AppHandle,
    plugin_id: &str,
    manifest_url_hint: Option<&str>,
    expected_latest_version: Option<&str>,
) -> Result<PluginManifest, String> {
    let safe_id = plugin_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>();
    let manifests_dir = manifest_cache_dir(app)?;
    let cached_json_path = manifests_dir.join(format!("{safe_id}.json"));
    let cached_etag_path = manifests_dir.join(format!("{safe_id}.etag"));

    // Bypass stale ETag when registry.json advertises a newer version missing from the cached manifest.
    let cache_has_latest = match (
        expected_latest_version
            .map(str::trim)
            .filter(|v| !v.is_empty()),
        fs::read_to_string(&cached_json_path).ok(),
    ) {
        (Some(latest), Some(raw)) => serde_json::from_str::<PluginManifest>(&raw)
            .map(|m| m.versions.iter().any(|v| v.version == latest))
            .unwrap_or(false),
        (Some(_), None) => false,
        (None, _) => true,
    };

    let cached_etag = if cache_has_latest {
        fs::read_to_string(&cached_etag_path)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    } else {
        None
    };

    let url = manifest_url_hint
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| {
            format!("https://cdn.socialposter.dev/plugins/{plugin_id}/manifest.json")
        });

    let pinned_key = hub_public_key()?;
    let client = Client::new();
    let mut req = client.get(&url);
    if let Some(ref etag) = cached_etag {
        req = req.header(IF_NONE_MATCH, etag);
    }

    let parse_and_verify = |raw: &str| -> Result<PluginManifest, String> {
        let manifest: PluginManifest = serde_json::from_str(raw)
            .map_err(|err| format!("Failed to parse plugin manifest: {err}"))?;
        verify_announced_hub_key(
            manifest.hub_public_key.as_deref(),
            &pinned_key,
            &format!("Plugin '{plugin_id}' manifest"),
        )?;
        if let Some(ref sig) = manifest.signature {
            let mut unsigned = manifest.clone();
            unsigned.signature = None;
            let canonical = serde_json::to_vec(&unsigned)
                .map_err(|err| format!("Failed to serialise manifest for verification: {err}"))?;
            verify_ed25519_signature(&pinned_key, &canonical, sig)?;
        }
        Ok(manifest)
    };

    match req.send().await {
        Ok(res) if res.status() == reqwest::StatusCode::NOT_MODIFIED => {
            let raw = fs::read_to_string(&cached_json_path)
                .map_err(|err| format!("Failed to read cached plugin manifest: {err}"))?;
            parse_and_verify(&raw)
        }
        Ok(res) if res.status().is_success() => {
            let next_etag = res
                .headers()
                .get(ETAG)
                .and_then(|v| v.to_str().ok())
                .map(ToString::to_string);
            let raw = res
                .text()
                .await
                .map_err(|err| format!("Failed to read plugin manifest: {err}"))?;
            let manifest = parse_and_verify(&raw)?;
            let _ = fs::write(&cached_json_path, &raw);
            if let Some(etag) = next_etag {
                let _ = fs::write(&cached_etag_path, etag);
            }
            Ok(manifest)
        }
        Ok(res) => {
            let status = res.status();
            if cached_json_path.is_file() {
                let raw = fs::read_to_string(&cached_json_path)
                    .map_err(|err| format!("Failed to read cached plugin manifest: {err}"))?;
                parse_and_verify(&raw)
            } else {
                Err(format!(
                    "Failed to download plugin manifest (HTTP {status})."
                ))
            }
        }
        Err(err) => {
            if cached_json_path.is_file() {
                let raw = fs::read_to_string(&cached_json_path).map_err(|read_err| {
                    format!("Failed to read cached plugin manifest: {read_err}")
                })?;
                parse_and_verify(&raw)
            } else {
                Err(format!("Unable to reach plugin manifest: {err}"))
            }
        }
    }
}

pub async fn install_from_registry(
    app: &AppHandle,
    plugin_id: &str,
    requested_version: Option<&str>,
) -> Result<InstalledPluginRecord, String> {
    let index = fetch_registry_index(app).await?;
    let entry = index
        .plugins
        .iter()
        .find(|p| p.slug == plugin_id)
        .ok_or_else(|| format!("Plugin '{plugin_id}' was not found in the registry."))?;

    let target_version = requested_version
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or(&entry.latest_version);

    let client = Client::new();
    let pinned_key = hub_public_key()?;
    let manifest = fetch_plugin_manifest(
        app,
        plugin_id,
        entry.manifest_url.as_deref(),
        Some(&entry.latest_version),
    )
    .await?;

    let version_entry = manifest
        .versions
        .iter()
        .find(|v| v.version == target_version)
        .or_else(|| manifest.versions.first())
        .ok_or_else(|| {
            format!("Plugin '{plugin_id}' manifest contains no downloadable versions.")
        })?;

    // Enforce SemVer compatibility for the requested version before downloading the .wasm artefact.
    let version_compat_req = version_entry
        .compatible_app_version
        .as_deref()
        .or(manifest.compatible_app_version.as_deref())
        .or(entry.compatible_app_version.as_deref())
        .unwrap_or(">=1.0.0");
    let (version_is_compatible, _) = crate::core::registry::evaluate_compatibility(
        env!("CARGO_PKG_VERSION"),
        version_compat_req,
    );
    if !version_is_compatible {
        return Err(format!(
            "Cannot install {} v{}: requires SocialPoster {}, which is incompatible with v{}.",
            manifest.display_name.as_deref().unwrap_or(plugin_id),
            version_entry.version,
            version_compat_req,
            env!("CARGO_PKG_VERSION"),
        ));
    }

    let wasm_res = client
        .get(&version_entry.download_url)
        .send()
        .await
        .map_err(|err| format!("Failed to download plugin artefact: {err}"))?;
    if !wasm_res.status().is_success() {
        return Err(format!(
            "Failed to download plugin artefact (HTTP {}).",
            wasm_res.status()
        ));
    }

    let wasm_bytes = wasm_res
        .bytes()
        .await
        .map_err(|err| format!("Failed to read downloaded plugin bytes: {err}"))?;

    let verified_sha256 = verify_wasm_artefact(
        plugin_id,
        &version_entry.version,
        &wasm_bytes,
        &version_entry.sha256,
        version_entry.ed25519_signature.as_deref(),
        &pinned_key,
    )?;

    let plugin_ver_dir = installed_dir(app)?
        .join(plugin_id)
        .join(&version_entry.version);
    fs::create_dir_all(&plugin_ver_dir)
        .map_err(|err| format!("Failed to create plugin installation directory: {err}"))?;
    let dest_wasm = plugin_ver_dir.join("plugin.wasm");
    fs::write(&dest_wasm, &wasm_bytes)
        .map_err(|err| format!("Failed to write plugin artefact: {err}"))?;

    let loaded = match WasmPlatform::from_wasm_file(&dest_wasm) {
        Ok(platform) => platform,
        Err(err) => {
            let _ = fs::remove_file(&dest_wasm);
            return Err(err);
        }
    };

    let desc = loaded.describe();
    let (is_compatible, compat_err) = crate::core::registry::evaluate_compatibility(
        env!("CARGO_PKG_VERSION"),
        &desc.compatible_app_version,
    );
    if !is_compatible {
        let _ = fs::remove_file(&dest_wasm);
        return Err(compat_err.unwrap_or_else(|| {
            format!(
                "Plugin '{}' requires SocialPoster {} (current: {}).",
                desc.display_name,
                desc.compatible_app_version,
                env!("CARGO_PKG_VERSION")
            )
        }));
    }
    let allowed_domains = if !desc.allowed_domains.is_empty() {
        desc.allowed_domains.clone()
    } else if !manifest.allowed_domains.is_empty() {
        manifest.allowed_domains.clone()
    } else {
        manifest
            .capabilities
            .as_ref()
            .map(|c| c.allowed_domains.clone())
            .unwrap_or_default()
    };

    let icon_svg = desc
        .icon_svg
        .clone()
        .or_else(|| manifest.resolved_icon_svg());
    let brand_colour = Some(
        manifest
            .brand_colour
            .clone()
            .or_else(|| entry.brand_colour.clone())
            .unwrap_or_else(|| desc.brand_colour.clone()),
    );

    let is_official = is_official_plugin(
        manifest.is_official || entry.is_official,
        manifest.author.as_ref().or(entry.author.as_ref()),
    );

    let record = InstalledPluginRecord {
        id: desc.id.clone(),
        name: desc.display_name.clone(),
        version: version_entry.version.clone(),
        wasm_path: dest_wasm.to_string_lossy().to_string(),
        enabled: true,
        verified: true,
        is_official,
        source: "registry".to_string(),
        sha256: verified_sha256,
        installed_at: Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        allowed_domains,
        brand_colour,
        icon_svg,
    };

    let mut records = load_installed_records(app)?;
    records.retain(|r| r.id != record.id);
    records.push(record.clone());
    save_installed_records(app, &records)?;

    Ok(record)
}

pub fn install_local_wasm(
    app: &AppHandle,
    file_path: &Path,
) -> Result<InstalledPluginRecord, String> {
    if !file_path.is_file() {
        return Err(format!(
            "Plugin file '{}' does not exist.",
            file_path.display()
        ));
    }

    let bytes =
        fs::read(file_path).map_err(|err| format!("Failed to read local plugin file: {err}"))?;
    let sha256 = compute_sha256_hex(&bytes);

    let loaded = WasmPlatform::from_wasm_file(file_path)?;
    let desc = loaded.describe().clone();
    let (is_compatible, compat_err) = crate::core::registry::evaluate_compatibility(
        env!("CARGO_PKG_VERSION"),
        &desc.compatible_app_version,
    );
    if !is_compatible {
        return Err(compat_err.unwrap_or_else(|| {
            format!(
                "Plugin '{}' requires SocialPoster {} (current: {}).",
                desc.display_name,
                desc.compatible_app_version,
                env!("CARGO_PKG_VERSION")
            )
        }));
    }

    let plugin_ver_dir = installed_dir(app)?
        .join(&desc.id)
        .join(format!("local-{}", &sha256[..8]));
    fs::create_dir_all(&plugin_ver_dir)
        .map_err(|err| format!("Failed to create local plugin directory: {err}"))?;
    let dest_wasm = plugin_ver_dir.join("plugin.wasm");
    fs::write(&dest_wasm, bytes)
        .map_err(|err| format!("Failed to copy local plugin artefact: {err}"))?;

    let record = InstalledPluginRecord {
        id: desc.id,
        name: desc.display_name,
        version: desc.version,
        wasm_path: dest_wasm.to_string_lossy().to_string(),
        enabled: true,
        verified: false,
        is_official: false,
        source: "local".to_string(),
        sha256,
        installed_at: Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        allowed_domains: desc.allowed_domains,
        brand_colour: Some(desc.brand_colour),
        icon_svg: desc.icon_svg,
    };

    let mut records = load_installed_records(app)?;
    records.retain(|r| r.id != record.id);
    records.push(record.clone());
    save_installed_records(app, &records)?;

    Ok(record)
}

pub fn set_plugin_enabled(app: &AppHandle, plugin_id: &str, enabled: bool) -> Result<(), String> {
    let mut records = load_installed_records(app)?;
    let Some(target) = records.iter_mut().find(|r| r.id == plugin_id) else {
        return Err(format!("Plugin '{plugin_id}' is not installed."));
    };
    target.enabled = enabled;
    save_installed_records(app, &records)
}

pub fn uninstall_plugin(app: &AppHandle, plugin_id: &str) -> Result<(), String> {
    let mut records = load_installed_records(app)?;
    let before = records.len();
    records.retain(|r| r.id != plugin_id);
    if records.len() == before {
        return Err(format!("Plugin '{plugin_id}' is not installed."));
    }
    save_installed_records(app, &records)?;

    let dir = installed_dir(app)?.join(plugin_id);
    if dir.exists() {
        let _ = fs::remove_dir_all(&dir);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    #[test]
    fn parses_live_hub_registry_and_manifest_schemas() {
        let registry_json = r##"{
            "schema_version": 1,
            "generated_at": "2026-11-21T08:52:37Z",
            "hub_public_key": "snOcIevOJOkcz1eiHGKgxFoQmLPAxILaKjo0L0T77pc=",
            "plugins": [
                {
                    "slug": "discord",
                    "display_name": "Discord",
                    "summary": "Official Discord plugin",
                    "author": {
                        "slug": "socialposter",
                        "display_name": "SocialPoster",
                        "url": "https://github.com/socialposter-dev",
                        "is_official": true
                    },
                    "brand_colour": "#5865F2",
                    "latest_version": "1.0.0",
                    "is_deprecated": false,
                    "manifest_url": "https://cdn.socialposter.dev/plugins/discord/manifest.json",
                    "supports_threads": false,
                    "supports_alt_text": true,
                    "supports_delete": true
                }
            ]
        }"##;

        let index: RegistryIndex =
            serde_json::from_str(registry_json).expect("parses live registry schema");
        assert_eq!(index.plugins.len(), 1);
        assert_eq!(index.plugins[0].slug, "discord");
        assert_eq!(index.plugins[0].latest_version, "1.0.0");

        let pinned = hub_public_key().expect("decodes pinned hub key");
        verify_announced_hub_key(index.hub_public_key.as_deref(), &pinned, "Registry")
            .expect("pinned key matches live key");
    }

    #[test]
    fn verifies_canonical_hub_payload_with_hex_and_url_safe_base64() {
        let signing_key = SigningKey::from_bytes(&[7u8; 32]);
        let verifying_key = signing_key.verifying_key();
        let wasm_bytes = b"\0asm\x01\x00\x00\x00mock-plugin";
        let sha_hex = compute_sha256_hex(wasm_bytes);
        let payload = format!("discord:1.0.0:{sha_hex}");
        let sig = signing_key.sign(payload.as_bytes());
        let sig_hex: String = sig.to_bytes().iter().map(|b| format!("{b:02x}")).collect();
        let sig_url_no_pad = URL_SAFE_NO_PAD.encode(sig.to_bytes());

        let result_hex = verify_wasm_artefact(
            "discord",
            "1.0.0",
            wasm_bytes,
            &sha_hex,
            Some(&sig_hex),
            &verifying_key,
        );
        assert_eq!(result_hex.as_deref(), Ok(sha_hex.as_str()));

        let result_b64 = verify_wasm_artefact(
            "discord",
            "1.0.0",
            wasm_bytes,
            &sha_hex,
            Some(&sig_url_no_pad),
            &verifying_key,
        );
        assert_eq!(result_b64.as_deref(), Ok(sha_hex.as_str()));
    }

    #[test]
    fn decodes_64_char_hex_public_key_and_128_char_hex_signature() {
        let signing_key = SigningKey::from_bytes(&[11u8; 32]);
        let pub_hex: String = signing_key
            .verifying_key()
            .to_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let decoded_key = decode_hex_or_base64(&pub_hex, 32, "Hub Ed25519 public key")
            .expect("decodes 64-char hex key");
        assert_eq!(
            decoded_key.as_slice(),
            signing_key.verifying_key().as_bytes()
        );

        let bad_hex = "z".repeat(128);
        let hex_err = decode_hex_or_base64(&bad_hex, 64, "Ed25519 signature")
            .expect_err("rejects non-hex 128-char signature");
        assert!(hex_err.contains("hex encoding"));
    }

    #[test]
    fn reports_specific_verification_failures() {
        let signing_key = SigningKey::from_bytes(&[9u8; 32]);
        let verifying_key = signing_key.verifying_key();
        let wasm_bytes = b"\0asm\x01\x00\x00\x00authentic";
        let sha_hex = compute_sha256_hex(wasm_bytes);
        let payload = format!("discord:1.0.0:{sha_hex}");
        let sig = signing_key.sign(payload.as_bytes());
        let sig_b64 = STANDARD.encode(sig.to_bytes());

        let missing_err = verify_wasm_artefact(
            "discord",
            "1.0.0",
            wasm_bytes,
            &sha_hex,
            None,
            &verifying_key,
        )
        .expect_err("rejects missing signature");
        assert!(missing_err.contains("missing its Ed25519 signature field"));

        let malformed_err = verify_wasm_artefact(
            "discord",
            "1.0.0",
            wasm_bytes,
            &sha_hex,
            Some("not-valid-base64!!!"),
            &verifying_key,
        )
        .expect_err("rejects malformed signature");
        assert!(malformed_err.contains("Malformed Ed25519 signature"));

        let tampered = b"\0asm\x01\x00\x00\x00tampered!";
        let hash_err = verify_wasm_artefact(
            "discord",
            "1.0.0",
            tampered,
            &sha_hex,
            Some(&sig_b64),
            &verifying_key,
        )
        .expect_err("rejects tampered bytes");
        assert!(hash_err.contains("SHA-256 digest mismatch"));

        let wrong_key = SigningKey::from_bytes(&[12u8; 32]).verifying_key();
        let key_err = verify_wasm_artefact(
            "discord",
            "1.0.0",
            wasm_bytes,
            &sha_hex,
            Some(&sig_b64),
            &wrong_key,
        )
        .expect_err("rejects signature from wrong key");
        assert!(key_err.contains("does not match the pinned Hub public key"));
    }
}
