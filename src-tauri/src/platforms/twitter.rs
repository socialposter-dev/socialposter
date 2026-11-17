use crate::core::oauth::{self, OAuthTokens};
use crate::core::platform::{
    AuthField, AuthMode, AuthorLink, FieldKind, PlatformAuthor, PlatformCapabilities,
    PlatformEntry, PostOutcome, PostPayload, SocialPlatform,
};
use crate::core::vault;
use async_trait::async_trait;
use reqwest::multipart::{Form, Part};
use reqwest::{Client, RequestBuilder, StatusCode};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::time::Duration;
use url::Url;

const X_CALLBACK_PORT: u16 = 48914;
const X_SCOPES: &str = "tweet.read tweet.write users.read media.write offline.access";
const MAX_IMAGE_BYTES: usize = 5 * 1024 * 1024;
const MAX_GIF_BYTES: usize = 15 * 1024 * 1024;
const MAX_VIDEO_BYTES: usize = 512 * 1024 * 1024;
const CHUNK_SIZE_BYTES: usize = 5 * 1024 * 1024;

pub struct TwitterPlatform;

#[derive(Debug, Deserialize)]
struct XDataEnvelope<T> {
    data: Option<T>,
}

#[derive(Debug, Deserialize)]
struct XProcessingError {
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct XProcessingInfo {
    state: String,
    check_after_secs: Option<u64>,
    error: Option<XProcessingError>,
}

#[derive(Debug, Deserialize)]
struct XMediaUploadData {
    id: String,
    processing_info: Option<XProcessingInfo>,
}

#[derive(Debug, Deserialize)]
struct XTweetData {
    id: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MediaKind {
    Image,
    Gif,
    Video,
}

impl MediaKind {
    fn category(self) -> &'static str {
        match self {
            MediaKind::Image => "tweet_image",
            MediaKind::Gif => "tweet_gif",
            MediaKind::Video => "tweet_video",
        }
    }
}

fn require_non_empty<'a>(
    creds: &'a HashMap<String, String>,
    key: &str,
    label: &str,
) -> Result<&'a str, String> {
    creds
        .get(key)
        .map(String::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("Missing X / Twitter {label}."))
}

async fn exchange_token_request(
    client: &Client,
    client_id: &str,
    client_secret: Option<&str>,
    params: &[(&str, &str)],
) -> Result<OAuthTokens, String> {
    let mut req = client.post("https://api.x.com/2/oauth2/token");
    let mut form_params: Vec<(&str, &str)> = params.to_vec();

    if let Some(secret) = client_secret.map(str::trim).filter(|s| !s.is_empty()) {
        req = req.basic_auth(client_id, Some(secret));
    } else {
        form_params.push(("client_id", client_id));
    }

    let res = req
        .form(&form_params)
        .send()
        .await
        .map_err(|err| format!("X OAuth token request failed: {err}"))?;

    let status = res.status();
    if !status.is_success() {
        let body = res.text().await.unwrap_or_default();
        return Err(format!("X OAuth token exchange failed (HTTP {status}): {body}"));
    }

    res.json::<OAuthTokens>()
        .await
        .map_err(|err| format!("Failed to parse X OAuth token response: {err}"))
}

async fn refresh_access_token(
    client: &Client,
    creds: &mut HashMap<String, String>,
) -> Result<String, String> {
    let refresh_token = creds
        .get("refresh_token")
        .map(String::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "Access token expired and no refresh token is stored.".to_string())?
        .to_string();

    let client_id = creds
        .get("client_id")
        .map(String::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "Cannot refresh X token without stored Client ID.".to_string())?
        .to_string();

    let client_secret = creds.get("client_secret").cloned();

    let tokens = exchange_token_request(
        client,
        &client_id,
        client_secret.as_deref(),
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token.as_str()),
        ],
    )
    .await?;

    let new_access = tokens.access_token.clone();
    let new_refresh = tokens.refresh_token.clone();
    creds.insert("access_token".to_string(), new_access.clone());
    if let Some(ref rt) = new_refresh {
        creds.insert("refresh_token".to_string(), rt.clone());
    }

    let profile_id = creds
        .get(vault::PROFILE_ID_KEY)
        .map(String::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or("default")
        .to_string();

    let access_to_save = new_access.clone();
    let refresh_to_save = new_refresh.clone();
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        vault::save_secret(&profile_id, "twitter", "access_token", &access_to_save)?;
        if let Some(rt) = refresh_to_save {
            vault::save_secret(&profile_id, "twitter", "refresh_token", &rt)?;
        }
        Ok(())
    })
    .await
    .map_err(|err| format!("Failed to persist refreshed X token: {err}"))??;

    Ok(new_access)
}

async fn send_with_auto_refresh<F>(
    client: &Client,
    creds: &mut HashMap<String, String>,
    build: F,
) -> Result<reqwest::Response, String>
where
    F: Fn(&str) -> Result<RequestBuilder, String>,
{
    let token = require_non_empty(creds, "access_token", "Access Token")?.to_string();
    let res = build(&token)?
        .send()
        .await
        .map_err(|err| format!("Network error connecting to X: {err}"))?;

    if res.status() == StatusCode::UNAUTHORIZED
        && creds
            .get("refresh_token")
            .is_some_and(|rt| !rt.trim().is_empty())
    {
        let refreshed = refresh_access_token(client, creds).await?;
        let retry_res = build(&refreshed)?
            .send()
            .await
            .map_err(|err| format!("Network error connecting to X: {err}"))?;
        return Ok(retry_res);
    }

    Ok(res)
}

fn classify_media(post: &PostPayload) -> Result<Vec<MediaKind>, String> {
    if post.media.is_empty() {
        return Ok(Vec::new());
    }

    let mut kinds = Vec::with_capacity(post.media.len());
    for attachment in &post.media {
        let kind = if attachment.mime_type == "image/gif" {
            MediaKind::Gif
        } else if attachment.mime_type.starts_with("video/") {
            MediaKind::Video
        } else if attachment.mime_type.starts_with("image/") {
            MediaKind::Image
        } else {
            return Err(format!(
                "Unsupported X media type '{}'.",
                attachment.mime_type
            ));
        };
        kinds.push(kind);
    }

    let has_video = kinds.contains(&MediaKind::Video);
    let has_gif = kinds.contains(&MediaKind::Gif);
    let has_image = kinds.contains(&MediaKind::Image);

    if has_video && (has_image || has_gif || kinds.len() > 1) {
        return Err("X allows only 1 video per post and cannot mix video with images.".to_string());
    }
    if has_gif && (has_image || kinds.len() > 1) {
        return Err("X allows only 1 GIF per post and cannot mix GIFs with images.".to_string());
    }
    if kinds.len() > 4 {
        return Err("X allows at most 4 images per post.".to_string());
    }

    Ok(kinds)
}

#[async_trait]
impl SocialPlatform for TwitterPlatform {
    fn id(&self) -> &'static str {
        "twitter"
    }

    fn name(&self) -> &'static str {
        "X / Twitter"
    }

    fn accent_colour(&self) -> &'static str {
        "#0F1419"
    }

    fn version(&self) -> &'static str {
        "0.1.4"
    }

    fn compatible_app_version(&self) -> &'static str {
        ">=0.2.0, <1.0.0"
    }

    fn authors(&self) -> Vec<PlatformAuthor> {
        vec![PlatformAuthor {
            name: "SocialPoster Developers",
            links: vec![AuthorLink {
                label: "GitHub",
                url: "https://github.com/socialposter-dev/socialposter",
            }],
        }]
    }

    fn auth_modes(&self) -> Vec<AuthMode> {
        vec![
            AuthMode {
                id: "oauth",
                label: "Browser Login (OAuth 2.0)",
                description: Some("Sign in with X using OAuth 2.0 PKCE."),
                fields: vec![
                    AuthField {
                        key: "client_id",
                        label: "Client ID",
                        placeholder: "Paste your X OAuth 2.0 Client ID",
                        kind: FieldKind::Text,
                        required: true,
                        help_text: None,
                    },
                    AuthField {
                        key: "client_secret",
                        label: "Client Secret",
                        placeholder: "Optional (for confidential app clients)",
                        kind: FieldKind::Secret,
                        required: false,
                        help_text: Some(
                            "Leave blank for Native App / public PKCE clients.",
                        ),
                    },
                    AuthField {
                        key: "oauth_connect",
                        label: "Authorise Account",
                        placeholder: "",
                        kind: FieldKind::OAuthButton {
                            button_label: "Connect with X",
                            target_key: "access_token",
                        },
                        required: true,
                        help_text: Some(
                            "Requires http://127.0.0.1:48914/callback in your X App's Callback URIs.",
                        ),
                    },
                ],
                setup_instructions: Some(
                    "### How to set up X / Twitter (OAuth 2.0)\n\n\
                     1. Open the [X Developer Portal](https://developer.x.com/en/portal/dashboard) and select or create a Project & App.\n\
                     2. Under **User authentication settings**, enable **OAuth 2.0** (Native App or Web App) with **Read and write** permissions.\n\
                     3. Set the **Callback URI** to `http://127.0.0.1:48914/callback`.\n\
                     4. Copy your **Client ID** (and **Client Secret** if using a confidential Web App), paste them on the left, and click **Connect with X**.",
                ),
            },
            AuthMode {
                id: "token",
                label: "Access Token",
                description: Some("Paste a user-context OAuth 2.0 Bearer token."),
                fields: vec![AuthField {
                    key: "access_token",
                    label: "Access Token",
                    placeholder: "Paste your OAuth 2.0 User Access Token",
                    kind: FieldKind::Secret,
                    required: true,
                    help_text: None,
                }],
                setup_instructions: Some(
                    "### How to set up X / Twitter (Access Token)\n\n\
                     1. Generate a user-context OAuth 2.0 Access Token with `tweet.read`, `tweet.write`, `users.read`, and `media.write` scopes.\n\
                     2. Paste the token on the left, then click **Save** and **Test Connection**.",
                ),
            },
        ]
    }

    fn setup_instructions(&self) -> &'static str {
        "### X / Twitter Setup\n\n\
         Use **Browser Login (OAuth 2.0)** with your X Developer App or paste a user-context OAuth 2.0 token."
    }

    fn capabilities(&self) -> PlatformCapabilities {
        PlatformCapabilities {
            max_chars: 280,
            max_images: 4,
            max_image_bytes: Some(5_000_000),
            supports_video: true,
            requires_media: false,
            supports_threads: true,
            supports_delete: true,
        }
    }

    async fn start_oauth(
        &self,
        mode_id: &str,
        inputs: &HashMap<String, String>,
    ) -> Result<HashMap<String, String>, String> {
        if mode_id != "oauth" {
            return Err(format!("OAuth is not supported for mode '{mode_id}'."));
        }

        let client_id = require_non_empty(inputs, "client_id", "Client ID")?.to_string();
        let client_secret = inputs
            .get("client_secret")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let (server, redirect_uri) = oauth::bind_loopback(Some(X_CALLBACK_PORT))?;
        let pkce = oauth::generate_pkce()?;
        let state = oauth::generate_state()?;

        let auth_url = Url::parse_with_params(
            "https://x.com/i/oauth2/authorize",
            &[
                ("response_type", "code"),
                ("client_id", client_id.as_str()),
                ("redirect_uri", redirect_uri.as_str()),
                ("scope", X_SCOPES),
                ("state", state.as_str()),
                ("code_challenge", pkce.code_challenge.as_str()),
                ("code_challenge_method", "S256"),
            ],
        )
        .map_err(|err| format!("Failed to build X authorise URL: {err}"))?;

        open::that(auth_url.as_str())
            .map_err(|err| format!("Failed to open browser for X login: {err}"))?;

        let code = oauth::wait_for_callback_code(
            server,
            Some(state),
            Duration::from_secs(120),
            self.name(),
            self.accent_colour(),
        )
        .await?;

        let client = Client::new();
        let tokens = exchange_token_request(
            &client,
            &client_id,
            client_secret.as_deref(),
            &[
                ("grant_type", "authorization_code"),
                ("code", code.as_str()),
                ("redirect_uri", redirect_uri.as_str()),
                ("code_verifier", pkce.code_verifier.as_str()),
            ],
        )
        .await?;

        let mut result = HashMap::new();
        result.insert("client_id".to_string(), client_id);
        if let Some(secret) = client_secret {
            result.insert("client_secret".to_string(), secret);
        }
        result.insert("access_token".to_string(), tokens.access_token);
        if let Some(refresh) = tokens.refresh_token {
            result.insert("refresh_token".to_string(), refresh);
        }
        Ok(result)
    }

    async fn verify_credentials(&self, creds: &HashMap<String, String>) -> Result<(), String> {
        let mut working_creds = creds.clone();
        let client = Client::new();

        let res = send_with_auto_refresh(&client, &mut working_creds, |token| {
            Ok(client
                .get("https://api.x.com/2/users/me")
                .bearer_auth(token))
        })
        .await?;

        let status = res.status();
        if status.is_success() {
            Ok(())
        } else {
            let body = res.text().await.unwrap_or_default();
            Err(format!("X verification failed (HTTP {status}): {body}"))
        }
    }

    async fn publish(
        &self,
        post: &PostPayload,
        creds: &HashMap<String, String>,
    ) -> Result<PostOutcome, String> {
        let kinds = classify_media(post)?;
        let mut working_creds = creds.clone();
        let client = Client::new();
        let mut media_ids: Vec<String> = Vec::new();

        for (attachment, kind) in post.media.iter().zip(kinds) {
            let bytes = attachment.read_bytes().await?;

            let max_bytes = match kind {
                MediaKind::Image => MAX_IMAGE_BYTES,
                MediaKind::Gif => MAX_GIF_BYTES,
                MediaKind::Video => MAX_VIDEO_BYTES,
            };
            if bytes.len() > max_bytes {
                return Err(format!(
                    "Attachment '{}' exceeds X's {} MB limit.",
                    attachment.path.display(),
                    max_bytes / (1024 * 1024)
                ));
            }

            let init_res = send_with_auto_refresh(&client, &mut working_creds, |token| {
                Ok(client
                    .post("https://api.x.com/2/media/upload/initialize")
                    .bearer_auth(token)
                    .json(&json!({
                        "media_type": attachment.mime_type,
                        "total_bytes": bytes.len(),
                        "media_category": kind.category(),
                    })))
            })
            .await?;

            let init_status = init_res.status();
            if !init_status.is_success() {
                let body = init_res.text().await.unwrap_or_default();
                return Err(format!(
                    "X media initialize failed (HTTP {init_status}): {body}"
                ));
            }

            let init_data = init_res
                .json::<XDataEnvelope<XMediaUploadData>>()
                .await
                .map_err(|err| format!("Failed to parse X media initialize response: {err}"))?
                .data
                .ok_or_else(|| "X media initialize response missing data.".to_string())?;
            let media_id = init_data.id;

            for (segment_index, chunk) in bytes.chunks(CHUNK_SIZE_BYTES).enumerate() {
                let chunk_vec = chunk.to_vec();
                let seg_str = segment_index.to_string();
                let mime = attachment.mime_type.clone();
                let url = format!("https://api.x.com/2/media/upload/{media_id}/append");

                let append_res = send_with_auto_refresh(&client, &mut working_creds, |token| {
                    let part = Part::bytes(chunk_vec.clone())
                        .file_name("media")
                        .mime_str(&mime)
                        .map_err(|err| format!("Invalid media MIME type '{mime}': {err}"))?;
                    let form = Form::new()
                        .text("segment_index", seg_str.clone())
                        .part("media", part);
                    Ok(client.post(&url).bearer_auth(token).multipart(form))
                })
                .await?;

                let append_status = append_res.status();
                if !append_status.is_success() {
                    let body = append_res.text().await.unwrap_or_default();
                    return Err(format!(
                        "X media append failed (HTTP {append_status}): {body}"
                    ));
                }
            }

            let finalize_url = format!("https://api.x.com/2/media/upload/{media_id}/finalize");
            let finalize_res = send_with_auto_refresh(&client, &mut working_creds, |token| {
                Ok(client.post(&finalize_url).bearer_auth(token))
            })
            .await?;

            let finalize_status = finalize_res.status();
            if !finalize_status.is_success() {
                let body = finalize_res.text().await.unwrap_or_default();
                return Err(format!(
                    "X media finalize failed (HTTP {finalize_status}): {body}"
                ));
            }

            let finalize_data = finalize_res
                .json::<XDataEnvelope<XMediaUploadData>>()
                .await
                .map_err(|err| format!("Failed to parse X media finalize response: {err}"))?
                .data;

            if let Some(mut info) = finalize_data.and_then(|d| d.processing_info) {
                for _ in 0..60 {
                    match info.state.as_str() {
                        "succeeded" => break,
                        "failed" => {
                            let reason = info
                                .error
                                .and_then(|e| e.message)
                                .unwrap_or_else(|| "Media processing failed".to_string());
                            return Err(format!("X media processing failed: {reason}"));
                        }
                        _ => {
                            let wait_secs = info.check_after_secs.unwrap_or(2).clamp(1, 30);
                            tokio::time::sleep(Duration::from_secs(wait_secs)).await;

                            let status_res =
                                send_with_auto_refresh(&client, &mut working_creds, |token| {
                                    Ok(client
                                        .get("https://api.x.com/2/media/upload")
                                        .bearer_auth(token)
                                        .query(&[
                                            ("command", "STATUS"),
                                            ("media_id", media_id.as_str()),
                                        ]))
                                })
                                .await?;

                            let poll_status = status_res.status();
                            if !poll_status.is_success() {
                                let body = status_res.text().await.unwrap_or_default();
                                return Err(format!(
                                    "X media status check failed (HTTP {poll_status}): {body}"
                                ));
                            }

                            let poll_data = status_res
                                .json::<XDataEnvelope<XMediaUploadData>>()
                                .await
                                .map_err(|err| {
                                    format!("Failed to parse X media status response: {err}")
                                })?
                                .data;

                            let Some(next_info) = poll_data.and_then(|d| d.processing_info) else {
                                break;
                            };
                            info = next_info;
                        }
                    }
                }
            }

            if let Some(alt) = attachment
                .alt_text
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                let meta_body = json!({
                    "media_id": media_id,
                    "alt_text": { "text": alt },
                });
                let meta_res = send_with_auto_refresh(&client, &mut working_creds, |token| {
                    Ok(client
                        .post("https://upload.twitter.com/1.1/media/metadata/create.json")
                        .bearer_auth(token)
                        .json(&meta_body))
                })
                .await?;

                let meta_status = meta_res.status();
                if !meta_status.is_success() {
                    let body = meta_res.text().await.unwrap_or_default();
                    return Err(format!(
                        "X media alt-text metadata failed (HTTP {meta_status}): {body}"
                    ));
                }
            }

            media_ids.push(media_id);
        }

        let mut tweet_body = Map::new();
        tweet_body.insert("text".to_string(), Value::String(post.text.clone()));
        if !media_ids.is_empty() {
            tweet_body.insert("media".to_string(), json!({ "media_ids": media_ids }));
        }
        if let Some(ref parent_id) = post.reply_parent_id {
            tweet_body.insert(
                "reply".to_string(),
                json!({ "in_reply_to_tweet_id": parent_id }),
            );
        }

        let tweet_json = Value::Object(tweet_body);
        let tweet_res = send_with_auto_refresh(&client, &mut working_creds, |token| {
            Ok(client
                .post("https://api.x.com/2/tweets")
                .bearer_auth(token)
                .json(&tweet_json))
        })
        .await?;

        let tweet_status = tweet_res.status();
        if !tweet_status.is_success() {
            let body = tweet_res.text().await.unwrap_or_default();
            return Err(format!("X post failed (HTTP {tweet_status}): {body}"));
        }

        let tweet_id = tweet_res
            .json::<XDataEnvelope<XTweetData>>()
            .await
            .map_err(|err| format!("Failed to parse X post response: {err}"))?
            .data
            .map(|d| d.id);

        let post_url = tweet_id
            .as_deref()
            .map(|id| format!("https://x.com/i/web/status/{id}"));

        Ok(PostOutcome {
            platform_id: self.id().to_string(),
            platform_name: self.name().to_string(),
            success: true,
            post_id: tweet_id,
            post_url,
            ..Default::default()
        })
    }

    async fn delete_post(
        &self,
        creds: &HashMap<String, String>,
        post_id: &str,
    ) -> Result<(), String> {
        let mut working_creds = creds.clone();
        let client = Client::new();
        let url = format!("https://api.x.com/2/tweets/{post_id}");

        let res = send_with_auto_refresh(&client, &mut working_creds, |token| {
            Ok(client.delete(&url).bearer_auth(token))
        })
        .await?;

        let status = res.status();
        if status.is_success() || status == StatusCode::NOT_FOUND {
            return Ok(());
        }
        let body = res.text().await.unwrap_or_default();
        Err(format!("X delete tweet failed (HTTP {status}): {body}"))
    }
}

inventory::submit! {
    PlatformEntry::new(|| Box::new(TwitterPlatform))
}
