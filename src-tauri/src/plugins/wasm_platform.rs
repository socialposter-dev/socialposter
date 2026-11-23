use crate::core::platform::{
    AuthField, AuthMode, AuthorLink, FieldKind, PlatformAuthor, PlatformCapabilities, PostOutcome,
    PostPayload, SelectOption, SocialPlatform,
};
use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use extism::{Function, Manifest, PTR, Plugin, UserData, Wasm};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use url::Url;

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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct WasmMediaInput {
    attachment_id: String,
    filename: String,
    mime_type: String,
    size_bytes: usize,
    alt_text: Option<String>,
    data_base64: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
struct WasmPublishInput<'a> {
    text: &'a str,
    media: Vec<WasmMediaInput>,
    attachments: Vec<WasmMediaInput>,
    credentials: &'a HashMap<String, String>,
    reply_root_id: Option<&'a str>,
    reply_parent_id: Option<&'a str>,
    reply_to_id: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct WasmValidateAuthOutput {
    #[serde(default = "default_true", alias = "valid")]
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
    #[serde(default, alias = "url")]
    post_url: Option<String>,
    #[serde(default)]
    notes: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct WasmDeleteOutput {
    #[serde(default = "default_true", alias = "deleted")]
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

#[derive(Clone, Default)]
struct SandboxHostContext {
    offline: bool,
    credentials: HashMap<String, String>,
    media_chunks: HashMap<String, Vec<u8>>,
    allowed_domains: Vec<String>,
}

impl SandboxHostContext {
    fn offline() -> Self {
        Self {
            offline: true,
            ..Default::default()
        }
    }
}

#[derive(Debug, Deserialize)]
struct SpHttpRequest {
    #[serde(default = "default_get_method")]
    method: String,
    url: String,
    #[serde(default)]
    headers: HashMap<String, String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    body_base64: Option<String>,
}

fn default_get_method() -> String {
    "GET".to_string()
}

#[derive(Debug, Deserialize)]
struct SpMediaChunkRequest {
    attachment_id: String,
    #[serde(default)]
    offset: usize,
    #[serde(default)]
    length: usize,
}

#[derive(Debug, Deserialize)]
struct SpHmacSha1Request {
    key: String,
    data: String,
}

/// Pure-Rust SHA-1 (FIPS 180-4) used exclusively by `sp_hmac_sha1` for OAuth 1.0a signing.
fn sha1_digest(input: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [
        0x6745_2301,
        0xEFCD_AB89,
        0x98BA_DCFE,
        0x1032_5476,
        0xC3D2_E1F0,
    ];
    let bit_len = (input.len() as u64).wrapping_mul(8);
    let mut msg = input.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.as_chunks::<64>().0 {
        let mut w = [0u32; 80];
        for (i, word) in chunk.as_chunks::<4>().0.iter().enumerate() {
            w[i] = u32::from_be_bytes(*word);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (i, &wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }

    let mut out = [0u8; 20];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

fn hmac_sha1_bytes(key: &[u8], data: &[u8]) -> [u8; 20] {
    let mut k_block = [0u8; 64];
    if key.len() > 64 {
        let hashed = sha1_digest(key);
        k_block[..20].copy_from_slice(&hashed);
    } else {
        k_block[..key.len()].copy_from_slice(key);
    }
    let mut o_key_pad = [0x5cu8; 64];
    let mut i_key_pad = [0x36u8; 64];
    for i in 0..64 {
        o_key_pad[i] ^= k_block[i];
        i_key_pad[i] ^= k_block[i];
    }

    let mut inner = Vec::with_capacity(64 + data.len());
    inner.extend_from_slice(&i_key_pad);
    inner.extend_from_slice(data);
    let inner_hash = sha1_digest(&inner);

    let mut outer = Vec::with_capacity(64 + 20);
    outer.extend_from_slice(&o_key_pad);
    outer.extend_from_slice(&inner_hash);
    sha1_digest(&outer)
}

fn is_host_allowed(url_str: &str, allowed_domains: &[String]) -> Result<(), String> {
    let parsed = Url::parse(url_str).map_err(|err| format!("Invalid URL '{url_str}': {err}"))?;
    if parsed.scheme() != "https" {
        return Err("Only HTTPS requests are permitted by the plugin sandbox.".to_string());
    }
    let Some(host) = parsed.host_str() else {
        return Err("URL is missing a target host.".to_string());
    };
    let host_lower = host.to_ascii_lowercase();
    if host_lower == "localhost" || host_lower.ends_with(".local") {
        return Err("Localhost and private hosts are blocked by the plugin sandbox.".to_string());
    }
    let permitted = allowed_domains.iter().any(|allowed| {
        let clean = allowed.trim().to_ascii_lowercase();
        if let Some(suffix) = clean.strip_prefix("*.") {
            host_lower == suffix || host_lower.ends_with(&format!(".{suffix}"))
        } else {
            host_lower == clean
        }
    });
    if !permitted {
        return Err(format!(
            "Host '{host}' is not declared in plugin allowed_domains."
        ));
    }
    Ok(())
}

fn execute_sp_http(req_json: &str, allowed_domains: &[String]) -> String {
    let parsed: SpHttpRequest = match serde_json::from_str(req_json) {
        Ok(req) => req,
        Err(err) => {
            return serde_json::json!({
                "status": 0,
                "headers": {},
                "body": "",
                "error": format!("Invalid sp_http request JSON: {err}"),
            })
            .to_string();
        }
    };

    if let Err(err) = is_host_allowed(&parsed.url, allowed_domains) {
        return serde_json::json!({
            "status": 0,
            "headers": {},
            "body": "",
            "error": err,
        })
        .to_string();
    }

    let method = match reqwest::Method::from_bytes(parsed.method.trim().as_bytes()) {
        Ok(m) => m,
        Err(err) => {
            return serde_json::json!({
                "status": 0,
                "headers": {},
                "body": "",
                "error": format!("Invalid HTTP method: {err}"),
            })
            .to_string();
        }
    };

    let body_bytes = if let Some(b64) = parsed.body_base64 {
        match STANDARD.decode(b64.trim()) {
            Ok(bytes) => Some(bytes),
            Err(err) => {
                return serde_json::json!({
                    "status": 0,
                    "headers": {},
                    "body": "",
                    "error": format!("Invalid body_base64 encoding: {err}"),
                })
                .to_string();
            }
        }
    } else {
        parsed.body.map(String::into_bytes)
    };

    let url = parsed.url;
    let headers = parsed.headers;
    let run_async = async move {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|err| format!("Failed to initialise HTTP client: {err}"))?;
        let mut builder = client.request(method, &url);
        for (k, v) in headers {
            builder = builder.header(&k, &v);
        }
        if let Some(bytes) = body_bytes {
            builder = builder.body(bytes);
        }
        let res = builder
            .send()
            .await
            .map_err(|err| format!("HTTP request failed: {err}"))?;
        let status = res.status().as_u16();
        let mut resp_headers = HashMap::new();
        for (name, val) in res.headers() {
            if let Ok(val_str) = val.to_str() {
                resp_headers.insert(name.as_str().to_ascii_lowercase(), val_str.to_string());
            }
        }
        let body = res
            .text()
            .await
            .map_err(|err| format!("Failed to read HTTP response body: {err}"))?;
        Ok::<_, String>((status, resp_headers, body))
    };

    let result = match tokio::runtime::Handle::try_current() {
        Ok(handle) => handle.block_on(run_async),
        Err(_) => match tokio::runtime::Runtime::new() {
            Ok(rt) => rt.block_on(run_async),
            Err(err) => Err(format!("Failed to initialise async runtime: {err}")),
        },
    };

    match result {
        Ok((status, resp_headers, body)) => serde_json::json!({
            "status": status,
            "headers": resp_headers,
            "body": body,
        })
        .to_string(),
        Err(err) => serde_json::json!({
            "status": 0,
            "headers": {},
            "body": "",
            "error": err,
        })
        .to_string(),
    }
}

/// Registers all five SocialPoster `1.0.0` Plugin ABI host functions under `extism:host/user`.
fn build_host_functions(ctx: SandboxHostContext) -> Vec<Function> {
    let ud_http = UserData::new(ctx.clone());
    let sp_http = Function::new(
        "sp_http",
        [PTR],
        [PTR],
        ud_http,
        |plugin, inputs, outputs, user_data| {
            let req_json: String = plugin.memory_get_val(&inputs[0])?;
            let response = user_data
                .get()?
                .lock()
                .map(|guard| {
                    if guard.offline {
                        serde_json::json!({
                            "status": 0,
                            "headers": {},
                            "body": "",
                            "error": "Network access is disabled during describe().",
                        })
                        .to_string()
                    } else {
                        execute_sp_http(&req_json, &guard.allowed_domains)
                    }
                })
                .unwrap_or_else(|_| {
                    serde_json::json!({
                        "status": 0,
                        "headers": {},
                        "body": "",
                        "error": "Host context unavailable.",
                    })
                    .to_string()
                });
            plugin.memory_set_val(&mut outputs[0], response)?;
            Ok(())
        },
    )
    .with_namespace("extism:host/user");

    let ud_cred = UserData::new(ctx.clone());
    let sp_credential = Function::new(
        "sp_credential",
        [PTR],
        [PTR],
        ud_cred,
        |plugin, inputs, outputs, user_data| {
            let key: String = plugin.memory_get_val(&inputs[0])?;
            let value = user_data
                .get()?
                .lock()
                .map(|guard| {
                    if guard.offline {
                        String::new()
                    } else {
                        guard
                            .credentials
                            .get(key.trim())
                            .cloned()
                            .unwrap_or_default()
                    }
                })
                .unwrap_or_default();
            plugin.memory_set_val(&mut outputs[0], value)?;
            Ok(())
        },
    )
    .with_namespace("extism:host/user");

    let ud_media = UserData::new(ctx.clone());
    let sp_media_chunk = Function::new(
        "sp_media_chunk",
        [PTR],
        [PTR],
        ud_media,
        |plugin, inputs, outputs, user_data| {
            let req_json: String = plugin.memory_get_val(&inputs[0])?;
            let chunk: Vec<u8> = user_data
                .get()?
                .lock()
                .map(|guard| {
                    if guard.offline {
                        return Vec::new();
                    }
                    let Ok(req) = serde_json::from_str::<SpMediaChunkRequest>(&req_json) else {
                        return Vec::new();
                    };
                    let Some(bytes) = guard.media_chunks.get(&req.attachment_id) else {
                        return Vec::new();
                    };
                    if req.offset >= bytes.len() || req.length == 0 {
                        return Vec::new();
                    }
                    let end = req.offset.saturating_add(req.length).min(bytes.len());
                    bytes[req.offset..end].to_vec()
                })
                .unwrap_or_default();
            plugin.memory_set_val(&mut outputs[0], chunk)?;
            Ok(())
        },
    )
    .with_namespace("extism:host/user");

    let ud_hmac = UserData::new(ctx.clone());
    let sp_hmac_sha1 = Function::new(
        "sp_hmac_sha1",
        [PTR],
        [PTR],
        ud_hmac,
        |plugin, inputs, outputs, user_data| {
            let req_json: String = plugin.memory_get_val(&inputs[0])?;
            let encoded = user_data
                .get()?
                .lock()
                .map(|guard| {
                    if guard.offline {
                        return String::new();
                    }
                    let Ok(req) = serde_json::from_str::<SpHmacSha1Request>(&req_json) else {
                        return String::new();
                    };
                    let digest = hmac_sha1_bytes(req.key.as_bytes(), req.data.as_bytes());
                    STANDARD.encode(digest)
                })
                .unwrap_or_default();
            plugin.memory_set_val(&mut outputs[0], encoded)?;
            Ok(())
        },
    )
    .with_namespace("extism:host/user");

    let ud_sleep = UserData::new(ctx);
    let sp_sleep_ms = Function::new(
        "sp_sleep_ms",
        [PTR],
        [PTR],
        ud_sleep,
        |plugin, inputs, outputs, user_data| {
            let raw: String = plugin.memory_get_val(&inputs[0])?;
            let is_offline = user_data
                .get()?
                .lock()
                .map(|guard| guard.offline)
                .unwrap_or(true);
            if !is_offline && let Ok(ms) = raw.trim().parse::<u64>() {
                std::thread::sleep(Duration::from_millis(ms.min(10_000)));
            }
            plugin.memory_set_val(&mut outputs[0], "")?;
            Ok(())
        },
    )
    .with_namespace("extism:host/user");

    vec![
        sp_http,
        sp_credential,
        sp_media_chunk,
        sp_hmac_sha1,
        sp_sleep_ms,
    ]
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
    host_context: SandboxHostContext,
) -> Result<String, String> {
    let manifest = build_sandbox_manifest(wasm_path, allowed_domains, timeout);
    let functions = build_host_functions(host_context);
    let mut plugin = Plugin::new(&manifest, functions, true)
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
        let raw = call_wasm_export(
            wasm_path,
            &[],
            SHORT_CALL_TIMEOUT,
            "describe",
            "",
            SandboxHostContext::offline(),
        )?;
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

    pub fn with_fallback_authors(mut self, fallback: Vec<PlatformAuthor>) -> Self {
        if self.authors_cached.is_empty() && !fallback.is_empty() {
            self.authors_cached = fallback;
        }
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

        let host_context = SandboxHostContext {
            offline: false,
            credentials: creds.clone(),
            media_chunks: HashMap::new(),
            allowed_domains: allowed_domains.clone(),
        };

        let raw = tokio::task::spawn_blocking(move || {
            call_wasm_export(
                &wasm_path,
                &allowed_domains,
                SHORT_CALL_TIMEOUT,
                "validate_auth",
                &input_json,
                host_context,
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
        let mut media_chunks = HashMap::with_capacity(post.media.len());
        for (idx, attachment) in post.media.iter().enumerate() {
            let bytes = attachment.read_bytes().await?;
            let filename = attachment
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("attachment")
                .to_string();
            let attachment_id = format!("att-{idx}");
            let size_bytes = bytes.len();
            let data_base64 = STANDARD.encode(&bytes);
            media_chunks.insert(attachment_id.clone(), bytes);
            encoded_media.push(WasmMediaInput {
                attachment_id,
                filename,
                mime_type: attachment.mime_type.clone(),
                size_bytes,
                alt_text: attachment.alt_text.clone(),
                data_base64,
            });
        }

        let input = WasmPublishInput {
            text: &post.text,
            media: encoded_media.clone(),
            attachments: encoded_media,
            credentials: creds,
            reply_root_id: post.reply_root_id.as_deref(),
            reply_parent_id: post.reply_parent_id.as_deref(),
            reply_to_id: post.reply_parent_id.as_deref(),
        };
        let input_json = serde_json::to_string(&input)
            .map_err(|err| format!("Failed to serialise publish payload for plugin: {err}"))?;

        let wasm_path = self.wasm_path.clone();
        let allowed_domains = self.describe.allowed_domains.clone();
        let host_context = SandboxHostContext {
            offline: false,
            credentials: creds.clone(),
            media_chunks,
            allowed_domains: allowed_domains.clone(),
        };
        let raw = tokio::task::spawn_blocking(move || {
            call_wasm_export(
                &wasm_path,
                &allowed_domains,
                PUBLISH_CALL_TIMEOUT,
                "publish",
                &input_json,
                host_context,
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
        let host_context = SandboxHostContext {
            offline: false,
            credentials: creds.clone(),
            media_chunks: HashMap::new(),
            allowed_domains: allowed_domains.clone(),
        };
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
                host_context,
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
