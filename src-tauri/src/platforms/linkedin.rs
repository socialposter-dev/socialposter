use crate::core::oauth::{self, OAuthTokens};
use crate::core::platform::{
    AuthField, AuthMode, AuthorLink, FieldKind, PlatformAuthor, PlatformCapabilities,
    PlatformEntry, PostOutcome, PostPayload, SocialPlatform,
};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::time::Duration;
use url::Url;

const LINKEDIN_CALLBACK_PORT: u16 = 48913;
const LINKEDIN_API_VERSION: &str = "202501";
const LINKEDIN_SCOPES: &str = "openid profile w_member_social";

pub struct LinkedInPlatform;

#[derive(Debug, Deserialize)]
struct UserInfoResponse {
    sub: Option<String>,
}

#[derive(Debug, Deserialize)]
struct InitializeUploadEnvelope {
    value: InitializeUploadValue,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InitializeUploadValue {
    upload_url: String,
    image: String,
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
        .ok_or_else(|| format!("Missing LinkedIn {label}."))
}

async fn fetch_member_sub(client: &Client, access_token: &str) -> Result<String, String> {
    let res = client
        .get("https://api.linkedin.com/v2/userinfo")
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|err| format!("Network error connecting to LinkedIn: {err}"))?;

    let status = res.status();
    if !status.is_success() {
        let body = res.text().await.unwrap_or_default();
        return Err(format!(
            "LinkedIn authentication failed (HTTP {status}): {body}"
        ));
    }

    let info = res
        .json::<UserInfoResponse>()
        .await
        .map_err(|err| format!("Failed to parse LinkedIn userinfo response: {err}"))?;

    info.sub
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| "LinkedIn userinfo response did not include member ID (`sub`).".to_string())
}

#[async_trait]
impl SocialPlatform for LinkedInPlatform {
    fn id(&self) -> &'static str {
        "linkedin"
    }

    fn name(&self) -> &'static str {
        "LinkedIn"
    }

    fn accent_colour(&self) -> &'static str {
        "#0A66C2"
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
                description: Some("Authorise via your LinkedIn Developer App in the browser."),
                fields: vec![
                    AuthField {
                        key: "client_id",
                        label: "Client ID",
                        placeholder: "Paste your LinkedIn Client ID",
                        kind: FieldKind::Text,
                        required: true,
                        help_text: None,
                    },
                    AuthField {
                        key: "client_secret",
                        label: "Client Secret",
                        placeholder: "Paste your LinkedIn Client Secret",
                        kind: FieldKind::Secret,
                        required: true,
                        help_text: None,
                    },
                    AuthField {
                        key: "oauth_connect",
                        label: "Authorise Account",
                        placeholder: "",
                        kind: FieldKind::OAuthButton {
                            button_label: "Connect with LinkedIn",
                            target_key: "access_token",
                        },
                        required: true,
                        help_text: Some(
                            "Requires http://127.0.0.1:48913/callback in your LinkedIn App's Authorized Redirect URLs.",
                        ),
                    },
                ],
                setup_instructions: Some(
                    "### How to set up LinkedIn (Browser Login)\n\n\
                     1. Open the [LinkedIn Developer Portal](https://www.linkedin.com/developers/apps) and create an app.\n\
                     2. Under **Products**, request access to **Share on LinkedIn** and **Sign In with LinkedIn using OpenID Connect**.\n\
                     3. Under **Auth → OAuth 2.0 settings**, add `http://127.0.0.1:48913/callback` as an **Authorized redirect URL**.\n\
                     4. Copy your **Client ID** and **Client Secret** into the fields on the left, then click **Connect with LinkedIn**.",
                ),
            },
            AuthMode {
                id: "token",
                label: "Access Token",
                description: Some("Paste an OAuth 2.0 access token directly."),
                fields: vec![AuthField {
                    key: "access_token",
                    label: "Access Token",
                    placeholder: "Paste your LinkedIn OAuth access token",
                    kind: FieldKind::Secret,
                    required: true,
                    help_text: None,
                }],
                setup_instructions: Some(
                    "### How to set up LinkedIn (Access Token)\n\n\
                     1. Open the [LinkedIn OAuth 2.0 Token Generator](https://www.linkedin.com/developers/tools/oauth/token-generator) in the Developer Portal.\n\
                     2. Select your app and check the `openid`, `profile`, and `w_member_social` scopes.\n\
                     3. Generate and copy the **Access Token**, paste it on the left, and click **Save** and **Test Connection**.",
                ),
            },
        ]
    }

    fn setup_instructions(&self) -> &'static str {
        "### LinkedIn Setup\n\n\
         Use **Browser Login (OAuth)** with a LinkedIn Developer App or paste a token via **Access Token**."
    }

    fn capabilities(&self) -> PlatformCapabilities {
        PlatformCapabilities {
            max_chars: 3000,
            max_images: 9,
            max_image_bytes: Some(5_000_000),
            supports_video: false,
            requires_media: false,
            supports_threads: false,
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
        let client_secret =
            require_non_empty(inputs, "client_secret", "Client Secret")?.to_string();

        let (server, redirect_uri) = oauth::bind_loopback(Some(LINKEDIN_CALLBACK_PORT))?;
        let state = oauth::generate_state()?;

        let auth_url = Url::parse_with_params(
            "https://www.linkedin.com/oauth/v2/authorization",
            &[
                ("response_type", "code"),
                ("client_id", client_id.as_str()),
                ("redirect_uri", redirect_uri.as_str()),
                ("state", state.as_str()),
                ("scope", LINKEDIN_SCOPES),
            ],
        )
        .map_err(|err| format!("Failed to build LinkedIn authorise URL: {err}"))?;

        open::that(auth_url.as_str())
            .map_err(|err| format!("Failed to open browser for LinkedIn login: {err}"))?;

        let code = oauth::wait_for_callback_code(
            server,
            Some(state),
            Duration::from_secs(120),
            self.name(),
            self.accent_colour(),
        )
        .await?;

        let client = Client::new();
        let token_res = client
            .post("https://www.linkedin.com/oauth/v2/accessToken")
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code.as_str()),
                ("redirect_uri", redirect_uri.as_str()),
                ("client_id", client_id.as_str()),
                ("client_secret", client_secret.as_str()),
            ])
            .send()
            .await
            .map_err(|err| format!("LinkedIn token exchange request failed: {err}"))?;

        let token_status = token_res.status();
        if !token_status.is_success() {
            let body = token_res.text().await.unwrap_or_default();
            return Err(format!(
                "LinkedIn token exchange failed (HTTP {token_status}): {body}"
            ));
        }

        let tokens = token_res
            .json::<OAuthTokens>()
            .await
            .map_err(|err| format!("Failed to parse LinkedIn token response: {err}"))?;

        let mut result = HashMap::new();
        result.insert("client_id".to_string(), client_id);
        result.insert("client_secret".to_string(), client_secret);
        result.insert("access_token".to_string(), tokens.access_token);
        if let Some(refresh) = tokens.refresh_token {
            result.insert("refresh_token".to_string(), refresh);
        }
        Ok(result)
    }

    async fn verify_credentials(&self, creds: &HashMap<String, String>) -> Result<(), String> {
        let token = require_non_empty(creds, "access_token", "Access Token")?;
        let client = Client::new();
        let _ = fetch_member_sub(&client, token).await?;
        Ok(())
    }

    async fn publish(
        &self,
        post: &PostPayload,
        creds: &HashMap<String, String>,
    ) -> Result<PostOutcome, String> {
        let token = require_non_empty(creds, "access_token", "Access Token")?;
        let client = Client::new();
        let sub = fetch_member_sub(&client, token).await?;
        let author_urn = format!("urn:li:person:{sub}");

        let mut uploaded_images: Vec<(String, Option<String>)> = Vec::new();
        for attachment in post.media.iter().take(9) {
            if !attachment.mime_type.starts_with("image/") {
                return Err(format!(
                    "LinkedIn integration currently only supports image attachments (got '{}').",
                    attachment.mime_type
                ));
            }

            let bytes = attachment.read_bytes().await?;

            let init_res = client
                .post("https://api.linkedin.com/rest/images?action=initializeUpload")
                .bearer_auth(token)
                .header("LinkedIn-Version", LINKEDIN_API_VERSION)
                .header("X-Restli-Protocol-Version", "2.0.0")
                .json(&json!({
                    "initializeUploadRequest": {
                        "owner": author_urn,
                    }
                }))
                .send()
                .await
                .map_err(|err| format!("Failed to initialise LinkedIn image upload: {err}"))?;

            let init_status = init_res.status();
            if !init_status.is_success() {
                let body = init_res.text().await.unwrap_or_default();
                return Err(format!(
                    "LinkedIn initializeUpload failed (HTTP {init_status}): {body}"
                ));
            }

            let init_data = init_res
                .json::<InitializeUploadEnvelope>()
                .await
                .map_err(|err| {
                    format!("Failed to parse LinkedIn initializeUpload response: {err}")
                })?;

            let put_res = client
                .put(&init_data.value.upload_url)
                .header(reqwest::header::CONTENT_TYPE, &attachment.mime_type)
                .body(bytes)
                .send()
                .await
                .map_err(|err| format!("Failed to upload image bytes to LinkedIn: {err}"))?;

            if !put_res.status().is_success() {
                let status = put_res.status();
                let body = put_res.text().await.unwrap_or_default();
                return Err(format!(
                    "LinkedIn image upload failed (HTTP {status}): {body}"
                ));
            }

            let alt = attachment
                .alt_text
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToString::to_string);
            uploaded_images.push((init_data.value.image, alt));
        }

        let mut post_body = Map::new();
        post_body.insert("author".to_string(), Value::String(author_urn));
        post_body.insert("commentary".to_string(), Value::String(post.text.clone()));
        post_body.insert("visibility".to_string(), Value::String("PUBLIC".to_string()));
        post_body.insert(
            "distribution".to_string(),
            json!({ "feedDistribution": "MAIN_FEED" }),
        );
        post_body.insert(
            "lifecycleState".to_string(),
            Value::String("PUBLISHED".to_string()),
        );

        if uploaded_images.len() == 1 {
            let (image_urn, alt) = &uploaded_images[0];
            let mut media_obj = Map::new();
            media_obj.insert("id".to_string(), Value::String(image_urn.clone()));
            if let Some(alt_text) = alt {
                media_obj.insert("altText".to_string(), Value::String(alt_text.clone()));
            }
            post_body.insert(
                "content".to_string(),
                json!({ "media": Value::Object(media_obj) }),
            );
        } else if uploaded_images.len() > 1 {
            let images_array: Vec<Value> = uploaded_images
                .iter()
                .map(|(image_urn, alt)| {
                    let mut item = Map::new();
                    item.insert("id".to_string(), Value::String(image_urn.clone()));
                    if let Some(alt_text) = alt {
                        item.insert("altText".to_string(), Value::String(alt_text.clone()));
                    }
                    Value::Object(item)
                })
                .collect();
            post_body.insert(
                "content".to_string(),
                json!({ "multiImage": { "images": images_array } }),
            );
        }

        let publish_res = client
            .post("https://api.linkedin.com/rest/posts")
            .bearer_auth(token)
            .header("LinkedIn-Version", LINKEDIN_API_VERSION)
            .header("X-Restli-Protocol-Version", "2.0.0")
            .json(&Value::Object(post_body))
            .send()
            .await
            .map_err(|err| format!("Failed to publish LinkedIn post: {err}"))?;

        let publish_status = publish_res.status();
        if !publish_status.is_success() {
            let body = publish_res.text().await.unwrap_or_default();
            return Err(format!(
                "LinkedIn post publish failed (HTTP {publish_status}): {body}"
            ));
        }

        let post_id = publish_res
            .headers()
            .get("x-restli-id")
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(ToString::to_string);

        let post_url = post_id
            .as_deref()
            .map(|post_urn| format!("https://www.linkedin.com/feed/update/{post_urn}/"));

        Ok(PostOutcome {
            platform_id: self.id().to_string(),
            platform_name: self.name().to_string(),
            success: true,
            post_id,
            post_url,
            ..Default::default()
        })
    }

    async fn delete_post(
        &self,
        creds: &HashMap<String, String>,
        post_id: &str,
    ) -> Result<(), String> {
        let token = require_non_empty(creds, "access_token", "Access Token")?;
        let encoded_urn: String = url::form_urlencoded::byte_serialize(post_id.as_bytes()).collect();
        let client = Client::new();

        let res = client
            .delete(format!("https://api.linkedin.com/rest/posts/{encoded_urn}"))
            .bearer_auth(token)
            .header("LinkedIn-Version", LINKEDIN_API_VERSION)
            .header("X-Restli-Protocol-Version", "2.0.0")
            .send()
            .await
            .map_err(|err| format!("Network error deleting LinkedIn post: {err}"))?;

        let status = res.status();
        if status.is_success() || status.as_u16() == 404 {
            return Ok(());
        }
        let body = res.text().await.unwrap_or_default();
        Err(format!("LinkedIn delete post failed (HTTP {status}): {body}"))
    }
}

inventory::submit! {
    PlatformEntry::new(|| Box::new(LinkedInPlatform))
}
