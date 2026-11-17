use crate::core::platform::{
    AuthField, AuthMode, AuthorLink, FieldKind, PlatformAuthor, PlatformCapabilities,
    PlatformEntry, PostOutcome, PostPayload, SocialPlatform,
};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::HashMap;

pub struct SlackPlatform;

#[derive(Debug, Deserialize)]
struct SlackApiResponse {
    ok: bool,
    error: Option<String>,
    upload_url: Option<String>,
    file_id: Option<String>,
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
        .ok_or_else(|| format!("Missing Slack {label}."))
}

fn require_webhook_url(creds: &HashMap<String, String>) -> Result<&str, String> {
    let url = require_non_empty(creds, "webhook_url", "Webhook URL")?;
    if !url.starts_with("https://hooks.slack.com/services/") {
        return Err(
            "Slack Webhook URL must start with https://hooks.slack.com/services/.".to_string(),
        );
    }
    Ok(url)
}

#[async_trait]
impl SocialPlatform for SlackPlatform {
    fn id(&self) -> &'static str {
        "slack"
    }

    fn name(&self) -> &'static str {
        "Slack"
    }

    fn accent_colour(&self) -> &'static str {
        "#E01E5A"
    }

    fn version(&self) -> &'static str {
        "0.1.1"
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
                id: "webhook",
                label: "Incoming Webhook",
                description: Some("Post text messages directly to a single Slack channel."),
                fields: vec![AuthField {
                    key: "webhook_url",
                    label: "Webhook URL",
                    placeholder: "https://hooks.slack.com/services/...",
                    kind: FieldKind::Secret,
                    required: true,
                    help_text: None,
                }],
                setup_instructions: Some(
                    "### How to set up Slack (Incoming Webhook)\n\n\
                     1. Open [api.slack.com/apps](https://api.slack.com/apps) and click **Create New App → From scratch**.\n\
                     2. Select **Incoming Webhooks** in the sidebar and toggle **Activate Incoming Webhooks** on.\n\
                     3. Click **Add New Webhook to Workspace**, pick a channel, and copy the **Webhook URL**.\n\
                     4. Paste the URL on the left, then click **Save** and **Test Connection**.",
                ),
            },
            AuthMode {
                id: "bot",
                label: "Slack Bot",
                description: Some("Post text and upload images or videos using a Slack Bot Token."),
                fields: vec![
                    AuthField {
                        key: "bot_token",
                        label: "Bot Token",
                        placeholder: "xoxb-...",
                        kind: FieldKind::Secret,
                        required: true,
                        help_text: None,
                    },
                    AuthField {
                        key: "channel_id",
                        label: "Channel ID",
                        placeholder: "C0123456789",
                        kind: FieldKind::Text,
                        required: true,
                        help_text: Some(
                            "Open the channel details in Slack and copy the Channel ID at the bottom.",
                        ),
                    },
                ],
                setup_instructions: Some(
                    "### How to set up Slack (Bot Token)\n\n\
                     1. Open [api.slack.com/apps](https://api.slack.com/apps) and select or create your app.\n\
                     2. Go to **OAuth & Permissions → Scopes → Bot Token Scopes** and add `chat:write` and `files:write`.\n\
                     3. Click **Install to Workspace** and copy the **Bot User OAuth Token** (`xoxb-...`).\n\
                     4. Invite the bot to your target channel (`/invite @YourBot`) and copy the **Channel ID** from the channel details.\n\
                     5. Paste both values on the left, then click **Save** and **Test Connection**.",
                ),
            },
        ]
    }

    fn setup_instructions(&self) -> &'static str {
        "### Slack Setup\n\n\
         Choose **Incoming Webhook** for text-only posting or **Slack Bot** for text and media attachments."
    }

    fn capabilities(&self) -> PlatformCapabilities {
        PlatformCapabilities {
            max_chars: 4000,
            max_images: 10,
            max_image_bytes: Some(10_000_000),
            supports_video: true,
            requires_media: false,
            supports_threads: false,
            supports_delete: false,
        }
    }

    async fn verify_credentials(&self, creds: &HashMap<String, String>) -> Result<(), String> {
        let mode = creds.get("__auth_mode").map(String::as_str).unwrap_or("webhook");

        if mode == "bot" {
            let bot_token = require_non_empty(creds, "bot_token", "Bot Token")?;
            let _ = require_non_empty(creds, "channel_id", "Channel ID")?;
            let client = Client::new();

            let res = client
                .post("https://slack.com/api/auth.test")
                .bearer_auth(bot_token)
                .send()
                .await
                .map_err(|err| format!("Network error connecting to Slack: {err}"))?;

            let status = res.status();
            let parsed: SlackApiResponse = res
                .json()
                .await
                .map_err(|err| format!("Invalid Slack auth.test response (HTTP {status}): {err}"))?;

            if status.is_success() && parsed.ok {
                Ok(())
            } else {
                Err(format!(
                    "Slack Bot verification failed: {}",
                    parsed.error.unwrap_or_else(|| format!("HTTP {status}"))
                ))
            }
        } else {
            let _ = require_webhook_url(creds)?;
            Ok(())
        }
    }

    async fn publish(
        &self,
        post: &PostPayload,
        creds: &HashMap<String, String>,
    ) -> Result<PostOutcome, String> {
        let mode = creds.get("__auth_mode").map(String::as_str).unwrap_or("webhook");
        let client = Client::new();

        if mode == "bot" {
            let bot_token = require_non_empty(creds, "bot_token", "Bot Token")?;
            let channel_id = require_non_empty(creds, "channel_id", "Channel ID")?;

            if post.media.is_empty() {
                let res = client
                    .post("https://slack.com/api/chat.postMessage")
                    .bearer_auth(bot_token)
                    .json(&json!({
                        "channel": channel_id,
                        "text": post.text,
                    }))
                    .send()
                    .await
                    .map_err(|err| format!("Failed to send Slack message: {err}"))?;

                let status = res.status();
                let parsed: SlackApiResponse = res
                    .json()
                    .await
                    .map_err(|err| format!("Invalid Slack response (HTTP {status}): {err}"))?;

                if !status.is_success() || !parsed.ok {
                    return Err(format!(
                        "Slack chat.postMessage failed: {}",
                        parsed.error.unwrap_or_else(|| format!("HTTP {status}"))
                    ));
                }
            } else {
                let mut uploaded_files: Vec<Value> = Vec::new();

                for attachment in post.media.iter().take(10) {
                    let bytes = attachment.read_bytes().await?;

                    let filename = attachment
                        .path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("attachment")
                        .to_string();
                    let len_str = bytes.len().to_string();

                    let url_res = client
                        .get("https://slack.com/api/files.getUploadURLExternal")
                        .bearer_auth(bot_token)
                        .query(&[("filename", filename.as_str()), ("length", len_str.as_str())])
                        .send()
                        .await
                        .map_err(|err| format!("Failed to request Slack upload URL: {err}"))?;

                    let url_status = url_res.status();
                    let url_parsed: SlackApiResponse = url_res.json().await.map_err(|err| {
                        format!("Invalid Slack getUploadURLExternal response (HTTP {url_status}): {err}")
                    })?;

                    if !url_status.is_success() || !url_parsed.ok {
                        return Err(format!(
                            "Slack getUploadURLExternal failed: {}",
                            url_parsed
                                .error
                                .unwrap_or_else(|| format!("HTTP {url_status}"))
                        ));
                    }

                    let upload_url = url_parsed
                        .upload_url
                        .ok_or_else(|| "Slack response missing upload_url.".to_string())?;
                    let file_id = url_parsed
                        .file_id
                        .ok_or_else(|| "Slack response missing file_id.".to_string())?;

                    let put_res = client
                        .post(&upload_url)
                        .body(bytes)
                        .send()
                        .await
                        .map_err(|err| format!("Failed to upload file bytes to Slack: {err}"))?;

                    if !put_res.status().is_success() {
                        let status = put_res.status();
                        let body = put_res.text().await.unwrap_or_default();
                        return Err(format!(
                            "Slack file upload failed (HTTP {status}): {body}"
                        ));
                    }

                    let title = attachment
                        .alt_text
                        .as_deref()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .unwrap_or(filename.as_str())
                        .to_string();

                    uploaded_files.push(json!({
                        "id": file_id,
                        "title": title,
                    }));
                }

                let mut complete_payload = Map::new();
                complete_payload.insert("files".to_string(), Value::Array(uploaded_files));
                complete_payload.insert(
                    "channel_id".to_string(),
                    Value::String(channel_id.to_string()),
                );
                if !post.text.trim().is_empty() {
                    complete_payload.insert(
                        "initial_comment".to_string(),
                        Value::String(post.text.clone()),
                    );
                }

                let complete_res = client
                    .post("https://slack.com/api/files.completeUploadExternal")
                    .bearer_auth(bot_token)
                    .json(&Value::Object(complete_payload))
                    .send()
                    .await
                    .map_err(|err| format!("Failed to complete Slack file upload: {err}"))?;

                let complete_status = complete_res.status();
                let complete_parsed: SlackApiResponse =
                    complete_res.json().await.map_err(|err| {
                        format!(
                            "Invalid Slack completeUploadExternal response (HTTP {complete_status}): {err}"
                        )
                    })?;

                if !complete_status.is_success() || !complete_parsed.ok {
                    return Err(format!(
                        "Slack completeUploadExternal failed: {}",
                        complete_parsed
                            .error
                            .unwrap_or_else(|| format!("HTTP {complete_status}"))
                    ));
                }
            }
        } else {
            if !post.media.is_empty() {
                return Err(
                    "Slack Incoming Webhooks only support text posts. Switch to Slack Bot mode to attach images or videos."
                        .to_string(),
                );
            }

            let webhook_url = require_webhook_url(creds)?;
            let res = client
                .post(webhook_url)
                .json(&json!({ "text": post.text }))
                .send()
                .await
                .map_err(|err| format!("Failed to post to Slack webhook: {err}"))?;

            let status = res.status();
            if !status.is_success() {
                let body = res.text().await.unwrap_or_default();
                return Err(format!("Slack webhook publish failed (HTTP {status}): {body}"));
            }
        }

        Ok(PostOutcome {
            platform_id: self.id().to_string(),
            platform_name: self.name().to_string(),
            success: true,
            ..Default::default()
        })
    }
}

inventory::submit! {
    PlatformEntry::new(|| Box::new(SlackPlatform))
}
