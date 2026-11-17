use crate::core::oauth::{self, OAuthTokenConfig};
use crate::core::platform::{
    AuthField, AuthMode, AuthorLink, FieldKind, PlatformAuthor, PlatformCapabilities,
    PlatformEntry, PostOutcome, PostPayload, SocialPlatform,
};
use async_trait::async_trait;
use reqwest::multipart::{Form, Part};
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::time::Duration;
use url::Url;

const MASTODON_SCOPES: &str = "read:accounts write:statuses write:media";

pub struct MastodonPlatform;

#[derive(Debug, Deserialize)]
struct MastodonAppResponse {
    client_id: String,
    client_secret: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MediaResponse {
    id: String,
}

#[derive(Debug, Deserialize)]
struct StatusResponse {
    id: String,
    url: Option<String>,
    uri: Option<String>,
}

fn normalise_instance_url(raw_input: Option<&String>) -> Result<String, String> {
    let raw = raw_input
        .map(String::as_str)
        .unwrap_or("")
        .trim()
        .trim_end_matches('/');

    if raw.is_empty() {
        return Err("Missing Mastodon Instance URL.".to_string());
    }

    if raw.starts_with("http://") {
        return Err("Mastodon Instance URL must use https://.".to_string());
    }

    if raw.starts_with("https://") {
        Ok(raw.to_string())
    } else {
        Ok(format!("https://{raw}"))
    }
}

fn require_token(creds: &HashMap<String, String>) -> Result<&str, String> {
    creds
        .get("access_token")
        .map(String::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "Missing Mastodon Access Token.".to_string())
}

#[async_trait]
impl SocialPlatform for MastodonPlatform {
    fn id(&self) -> &'static str {
        "mastodon"
    }

    fn name(&self) -> &'static str {
        "Mastodon"
    }

    fn accent_colour(&self) -> &'static str {
        "#6364FF"
    }

    fn version(&self) -> &'static str {
        "0.1.3"
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
                label: "Browser Login (OAuth)",
                description: Some("Sign in directly on your Mastodon server via your browser."),
                fields: vec![
                    AuthField {
                        key: "instance_url",
                        label: "Instance URL",
                        placeholder: "https://mastodon.social",
                        kind: FieldKind::Url,
                        required: true,
                        help_text: None,
                    },
                    AuthField {
                        key: "oauth_connect",
                        label: "Authorise Account",
                        placeholder: "",
                        kind: FieldKind::OAuthButton {
                            button_label: "Connect with Mastodon",
                            target_key: "access_token",
                        },
                        required: true,
                        help_text: Some(
                            "Enter your Instance URL above, then click to approve SocialPoster in your browser.",
                        ),
                    },
                ],
                setup_instructions: Some(
                    "### Connect Mastodon with Browser Login\n\n\
                     1. Enter your Mastodon **Instance URL** on the left (for example, `https://mastodon.social`).\n\
                     2. Click **Connect with Mastodon** to open your server's authorisation page in your default browser.\n\
                     3. Click **Authorize** — SocialPoster will automatically receive and store your access token.\n\
                     4. Press **Test Connection** to confirm everything is ready.",
                ),
            },
            AuthMode {
                id: "token",
                label: "Manual Access Token",
                description: Some("Paste an application access token generated in Preferences → Development."),
                fields: vec![
                    AuthField {
                        key: "instance_url",
                        label: "Instance URL",
                        placeholder: "https://mastodon.social",
                        kind: FieldKind::Url,
                        required: true,
                        help_text: None,
                    },
                    AuthField {
                        key: "access_token",
                        label: "Access Token",
                        placeholder: "Paste your Mastodon access token",
                        kind: FieldKind::Secret,
                        required: true,
                        help_text: None,
                    },
                ],
                setup_instructions: None,
            },
        ]
    }

    fn setup_instructions(&self) -> &'static str {
        "### How to set up Mastodon (Manual Token)\n\n\
         1. Sign in to your Mastodon server (for example, `https://mastodon.social` or your community instance).\n\
         2. Open **Preferences → Development** and click **New Application**.\n\
         3. Name the application `SocialPoster` and enable the `read:accounts`, `write:media`, and `write:statuses` scopes.\n\
         4. Click **Submit**, open the newly created application, and copy **Your access token**.\n\
         5. Paste your **Instance URL** and **Access Token** on the left, then click **Save** and **Test Connection**."
    }

    fn capabilities(&self) -> PlatformCapabilities {
        PlatformCapabilities {
            max_chars: 500,
            max_images: 4,
            max_image_bytes: Some(8_000_000),
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

        let instance_url = normalise_instance_url(inputs.get("instance_url"))?;
        let (server, redirect_uri) = oauth::bind_loopback(None)?;

        let client = Client::new();
        let app_res = client
            .post(format!("{instance_url}/api/v1/apps"))
            .form(&[
                ("client_name", "SocialPoster"),
                ("redirect_uris", redirect_uri.as_str()),
                ("scopes", MASTODON_SCOPES),
            ])
            .send()
            .await
            .map_err(|err| format!("Failed to register OAuth app on {instance_url}: {err}"))?;

        let app_status = app_res.status();
        if !app_status.is_success() {
            let body = app_res.text().await.unwrap_or_default();
            return Err(format!(
                "Mastodon app registration failed (HTTP {app_status}): {body}"
            ));
        }

        let app = app_res
            .json::<MastodonAppResponse>()
            .await
            .map_err(|err| format!("Failed to parse Mastodon app registration: {err}"))?;

        let pkce = oauth::generate_pkce()?;
        let state = oauth::generate_state()?;

        let auth_url = Url::parse_with_params(
            &format!("{instance_url}/oauth/authorize"),
            &[
                ("client_id", app.client_id.as_str()),
                ("scope", MASTODON_SCOPES),
                ("redirect_uri", redirect_uri.as_str()),
                ("response_type", "code"),
                ("code_challenge", pkce.code_challenge.as_str()),
                ("code_challenge_method", "S256"),
                ("state", state.as_str()),
            ],
        )
        .map_err(|err| format!("Failed to build Mastodon authorise URL: {err}"))?;

        open::that(auth_url.as_str())
            .map_err(|err| format!("Failed to open browser for Mastodon login: {err}"))?;

        let code = oauth::wait_for_callback_code(
            server,
            Some(state),
            Duration::from_secs(120),
            self.name(),
            self.accent_colour(),
        )
        .await?;

        let token_config = OAuthTokenConfig {
            client_id: app.client_id,
            client_secret: app.client_secret,
            token_url: format!("{instance_url}/oauth/token"),
            redirect_uri,
            extra_params: vec![],
        };

        let tokens =
            oauth::exchange_code_for_tokens(&client, &token_config, &code, &pkce.code_verifier)
                .await?;

        let mut result = HashMap::new();
        result.insert("instance_url".to_string(), instance_url);
        result.insert("access_token".to_string(), tokens.access_token);
        if let Some(refresh) = tokens.refresh_token {
            result.insert("refresh_token".to_string(), refresh);
        }
        Ok(result)
    }

    async fn verify_credentials(&self, creds: &HashMap<String, String>) -> Result<(), String> {
        let instance = normalise_instance_url(creds.get("instance_url"))?;
        let token = require_token(creds)?;

        let client = Client::new();
        let res = client
            .get(format!("{instance}/api/v1/accounts/verify_credentials"))
            .bearer_auth(token)
            .send()
            .await
            .map_err(|err| format!("Network error connecting to Mastodon: {err}"))?;

        let status = res.status();
        if status.is_success() {
            Ok(())
        } else {
            let body = res.text().await.unwrap_or_default();
            Err(format!(
                "Mastodon credential check failed (HTTP {status}): {body}"
            ))
        }
    }

    async fn publish(
        &self,
        post: &PostPayload,
        creds: &HashMap<String, String>,
    ) -> Result<PostOutcome, String> {
        let instance = normalise_instance_url(creds.get("instance_url"))?;
        let token = require_token(creds)?;
        let client = Client::new();

        let mut media_ids: Vec<String> = Vec::new();
        for attachment in post.media.iter().take(4) {
            let bytes = attachment.read_bytes().await?;

            let filename = attachment
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("attachment")
                .to_string();

            let file_part = Part::bytes(bytes)
                .file_name(filename)
                .mime_str(&attachment.mime_type)
                .map_err(|err| format!("Invalid media MIME type '{}': {err}", attachment.mime_type))?;

            let mut form = Form::new().part("file", file_part);
            if let Some(alt) = attachment
                .alt_text
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                form = form.text("description", alt.to_string());
            }

            let media_res = client
                .post(format!("{instance}/api/v1/media"))
                .bearer_auth(token)
                .multipart(form)
                .send()
                .await
                .map_err(|err| format!("Failed to upload media to Mastodon: {err}"))?;

            let media_status = media_res.status();
            if !media_status.is_success() {
                let body = media_res.text().await.unwrap_or_default();
                return Err(format!(
                    "Mastodon media upload failed (HTTP {media_status}): {body}"
                ));
            }

            let uploaded = media_res
                .json::<MediaResponse>()
                .await
                .map_err(|err| format!("Failed to parse Mastodon media response: {err}"))?;
            media_ids.push(uploaded.id);
        }

        let mut payload = json!({
            "status": post.text,
        });
        if !media_ids.is_empty() {
            payload["media_ids"] = json!(media_ids);
        }
        if let Some(ref parent_id) = post.reply_parent_id {
            payload["in_reply_to_id"] = json!(parent_id);
        }

        let status_res = client
            .post(format!("{instance}/api/v1/statuses"))
            .bearer_auth(token)
            .json(&payload)
            .send()
            .await
            .map_err(|err| format!("Failed to publish status to Mastodon: {err}"))?;

        let http_status = status_res.status();
        if !http_status.is_success() {
            let body = status_res.text().await.unwrap_or_default();
            return Err(format!(
                "Mastodon status publish failed (HTTP {http_status}): {body}"
            ));
        }

        let created = status_res
            .json::<StatusResponse>()
            .await
            .map_err(|err| format!("Failed to parse Mastodon status response: {err}"))?;

        Ok(PostOutcome {
            platform_id: self.id().to_string(),
            platform_name: self.name().to_string(),
            success: true,
            post_id: Some(created.id),
            post_url: created.url.or(created.uri),
            ..Default::default()
        })
    }

    async fn delete_post(
        &self,
        creds: &HashMap<String, String>,
        post_id: &str,
    ) -> Result<(), String> {
        let instance_url = normalise_instance_url(creds.get("instance_url"))?;
        let access_token = creds
            .get("access_token")
            .map(String::as_str)
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .ok_or_else(|| "Missing Mastodon Access Token.".to_string())?;

        let client = Client::new();
        let res = client
            .delete(format!("{instance_url}/api/v1/statuses/{post_id}"))
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|err| format!("Network error deleting Mastodon status: {err}"))?;

        let status = res.status();
        if status.is_success() || status.as_u16() == 404 {
            return Ok(());
        }
        let body = res.text().await.unwrap_or_default();
        Err(format!("Mastodon delete status failed (HTTP {status}): {body}"))
    }
}

inventory::submit! {
    PlatformEntry::new(|| Box::new(MastodonPlatform))
}
