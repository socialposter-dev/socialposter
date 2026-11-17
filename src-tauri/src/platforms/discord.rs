use crate::core::platform::{
    AuthField, AuthMode, AuthorLink, FieldKind, MediaAttachment, PlatformAuthor,
    PlatformCapabilities, PlatformEntry, PostOutcome, PostPayload, SocialPlatform,
};
use async_trait::async_trait;
use reqwest::multipart::{Form, Part};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::HashMap;

pub struct DiscordPlatform;

#[derive(Debug, Deserialize)]
struct DiscordMessageResponse {
    id: String,
    channel_id: String,
    guild_id: Option<String>,
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
        .ok_or_else(|| format!("Missing Discord {label}."))
}

fn require_webhook_url(creds: &HashMap<String, String>) -> Result<&str, String> {
    let url = require_non_empty(creds, "webhook_url", "Webhook URL")?;
    if !url.starts_with("https://") {
        return Err("Discord Webhook URL must start with https://.".to_string());
    }
    Ok(url)
}

async fn attach_files_to_form(
    mut form: Form,
    media: &[MediaAttachment],
) -> Result<Form, String> {
    for (idx, attachment) in media.iter().take(10).enumerate() {
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
            .map_err(|err| {
                format!("Invalid media MIME type '{}': {err}", attachment.mime_type)
            })?;

        form = form.part(format!("files[{idx}]"), file_part);
    }
    Ok(form)
}

fn build_attachments_metadata(media: &[MediaAttachment]) -> Option<Vec<Value>> {
    let has_alt = media.iter().any(|m| {
        m.alt_text
            .as_deref()
            .map(str::trim)
            .is_some_and(|s| !s.is_empty())
    });
    if !has_alt {
        return None;
    }

    let items = media
        .iter()
        .take(10)
        .enumerate()
        .map(|(idx, attachment)| {
            let filename = attachment
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("attachment");
            let mut obj = Map::new();
            obj.insert("id".to_string(), json!(idx));
            obj.insert("filename".to_string(), Value::String(filename.to_string()));
            if let Some(alt) = attachment
                .alt_text
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                obj.insert("description".to_string(), Value::String(alt.to_string()));
            }
            Value::Object(obj)
        })
        .collect();

    Some(items)
}

#[async_trait]
impl SocialPlatform for DiscordPlatform {
    fn id(&self) -> &'static str {
        "discord"
    }

    fn name(&self) -> &'static str {
        "Discord"
    }

    fn accent_colour(&self) -> &'static str {
        "#5865F2"
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
                id: "webhook",
                label: "Webhook",
                description: Some("Post directly to a single channel with an incoming webhook URL."),
                fields: vec![
                    AuthField {
                        key: "webhook_url",
                        label: "Webhook URL",
                        placeholder: "https://discord.com/api/webhooks/...",
                        kind: FieldKind::Url,
                        required: true,
                        help_text: None,
                    },
                    AuthField {
                        key: "username",
                        label: "Override Username",
                        placeholder: "Optional display name",
                        kind: FieldKind::Text,
                        required: false,
                        help_text: Some("Overrides the webhook's default display name for each post."),
                    },
                    AuthField {
                        key: "avatar_url",
                        label: "Override Avatar URL",
                        placeholder: "https://example.com/avatar.png",
                        kind: FieldKind::Url,
                        required: false,
                        help_text: Some("Optional HTTPS image URL for the webhook avatar."),
                    },
                ],
                setup_instructions: Some(
                    "### How to set up Discord (Webhook)\n\n\
                     1. Open Discord and go to the server where you want to post.\n\
                     2. Open **Server Settings → Integrations → Webhooks** (or edit a specific channel's **Integrations**).\n\
                     3. Click **New Webhook**, choose the target channel, and click **Copy Webhook URL**.\n\
                     4. Paste the Webhook URL on the left (and optionally set a custom username or avatar URL), then click **Save** and **Test Connection**.",
                ),
            },
            AuthMode {
                id: "bot",
                label: "Discord Bot",
                description: Some("Post to any channel using a Discord Bot Token and Channel ID."),
                fields: vec![
                    AuthField {
                        key: "bot_token",
                        label: "Bot Token",
                        placeholder: "Paste your Discord Bot Token",
                        kind: FieldKind::Secret,
                        required: true,
                        help_text: Some("Found in the Discord Developer Portal under Bot → Reset Token."),
                    },
                    AuthField {
                        key: "channel_id",
                        label: "Channel ID",
                        placeholder: "123456789012345678",
                        kind: FieldKind::Text,
                        required: true,
                        help_text: Some("Enable Developer Mode in Discord, right-click the channel, and choose Copy Channel ID."),
                    },
                ],
                setup_instructions: Some(
                    "### How to set up Discord (Bot)\n\n\
                     1. Open the [Discord Developer Portal](https://discord.com/developers/applications) and create (or select) an application.\n\
                     2. Go to **Bot**, copy or reset the **Token**, and paste it into **Bot Token**.\n\
                     3. Invite the bot to your server with **Send Messages** and **Attach Files** permissions.\n\
                     4. In Discord, enable **User Settings → Advanced → Developer Mode**, right-click the target channel, and select **Copy Channel ID**.\n\
                     5. Click **Save** and **Test Connection**.",
                ),
            },
        ]
    }

    fn setup_instructions(&self) -> &'static str {
        "### Discord Setup\n\n\
         Choose **Webhook** for quick single-channel posting or **Discord Bot** to post via a bot token and channel ID."
    }

    fn capabilities(&self) -> PlatformCapabilities {
        PlatformCapabilities {
            max_chars: 2000,
            max_images: 10,
            max_image_bytes: Some(8_000_000),
            supports_video: true,
            requires_media: false,
            supports_threads: false,
            supports_delete: true,
        }
    }

    async fn verify_credentials(&self, creds: &HashMap<String, String>) -> Result<(), String> {
        let mode = creds.get("__auth_mode").map(String::as_str).unwrap_or("webhook");
        let client = Client::new();

        if mode == "bot" {
            let bot_token = require_non_empty(creds, "bot_token", "Bot Token")?;
            let channel_id = require_non_empty(creds, "channel_id", "Channel ID")?;

            let res = client
                .get(format!("https://discord.com/api/v10/channels/{channel_id}"))
                .header(reqwest::header::AUTHORIZATION, format!("Bot {bot_token}"))
                .send()
                .await
                .map_err(|err| format!("Network error connecting to Discord: {err}"))?;

            let status = res.status();
            if status.is_success() {
                Ok(())
            } else {
                let body = res.text().await.unwrap_or_default();
                Err(format!("Discord Bot verification failed (HTTP {status}): {body}"))
            }
        } else {
            let webhook_url = require_webhook_url(creds)?;
            let res = client
                .get(webhook_url)
                .send()
                .await
                .map_err(|err| format!("Network error connecting to Discord: {err}"))?;

            let status = res.status();
            if status.is_success() {
                Ok(())
            } else {
                let body = res.text().await.unwrap_or_default();
                Err(format!(
                    "Discord webhook verification failed (HTTP {status}): {body}"
                ))
            }
        }
    }

    async fn publish(
        &self,
        post: &PostPayload,
        creds: &HashMap<String, String>,
    ) -> Result<PostOutcome, String> {
        let mode = creds.get("__auth_mode").map(String::as_str).unwrap_or("webhook");
        let client = Client::new();

        let res = if mode == "bot" {
            let bot_token = require_non_empty(creds, "bot_token", "Bot Token")?;
            let channel_id = require_non_empty(creds, "channel_id", "Channel ID")?;
            let endpoint = format!("https://discord.com/api/v10/channels/{channel_id}/messages");
            let auth_header = format!("Bot {bot_token}");

            if post.media.is_empty() {
                client
                    .post(&endpoint)
                    .header(reqwest::header::AUTHORIZATION, auth_header)
                    .json(&json!({ "content": post.text }))
                    .send()
                    .await
                    .map_err(|err| format!("Failed to send Discord Bot message: {err}"))?
            } else {
                let mut body_obj = Map::new();
                body_obj.insert("content".to_string(), Value::String(post.text.clone()));
                if let Some(attachments) = build_attachments_metadata(&post.media) {
                    body_obj.insert("attachments".to_string(), Value::Array(attachments));
                }
                let payload_json = Value::Object(body_obj).to_string();
                let form = attach_files_to_form(
                    Form::new().text("payload_json", payload_json),
                    &post.media,
                )
                .await?;

                client
                    .post(&endpoint)
                    .header(reqwest::header::AUTHORIZATION, auth_header)
                    .multipart(form)
                    .send()
                    .await
                    .map_err(|err| format!("Failed to send Discord Bot message: {err}"))?
            }
        } else {
            let webhook_url = require_webhook_url(creds)?;
            let post_endpoint = if webhook_url.contains('?') {
                format!("{webhook_url}&wait=true")
            } else {
                format!("{webhook_url}?wait=true")
            };

            let mut body_obj = Map::new();
            body_obj.insert("content".to_string(), Value::String(post.text.clone()));
            if let Some(username) = creds
                .get("username")
                .map(String::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                body_obj.insert("username".to_string(), Value::String(username.to_string()));
            }
            if let Some(avatar_url) = creds
                .get("avatar_url")
                .map(String::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                body_obj.insert("avatar_url".to_string(), Value::String(avatar_url.to_string()));
            }
            if let Some(attachments) = build_attachments_metadata(&post.media) {
                body_obj.insert("attachments".to_string(), Value::Array(attachments));
            }

            let payload_json = Value::Object(body_obj).to_string();
            let form = attach_files_to_form(
                Form::new().text("payload_json", payload_json),
                &post.media,
            )
            .await?;

            client
                .post(&post_endpoint)
                .multipart(form)
                .send()
                .await
                .map_err(|err| format!("Failed to send Discord webhook message: {err}"))?
        };

        let status = res.status();
        if !status.is_success() {
            let body = res.text().await.unwrap_or_default();
            return Err(format!("Discord publish failed (HTTP {status}): {body}"));
        }

        let parsed_msg = res.json::<DiscordMessageResponse>().await.ok();

        let post_id = parsed_msg.as_ref().map(|msg| {
            if mode == "bot" {
                format!("{}/{}", msg.channel_id, msg.id)
            } else {
                format!("webhook:{}", msg.id)
            }
        });

        let post_url = parsed_msg.and_then(|msg| {
            msg.guild_id.map(|guild_id| {
                format!(
                    "https://discord.com/channels/{guild_id}/{}/{}",
                    msg.channel_id, msg.id
                )
            })
        });

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
        let client = Client::new();

        let res = if let Some(webhook_msg_id) = post_id.strip_prefix("webhook:") {
            let webhook_url = require_webhook_url(creds)?;
            let (base_url, query_opt) = match webhook_url.split_once('?') {
                Some((base, query)) => (base.trim_end_matches('/'), Some(query)),
                None => (webhook_url.trim_end_matches('/'), None),
            };
            let thread_query = query_opt.and_then(|q| {
                q.split('&').find(|kv| kv.starts_with("thread_id="))
            });
            let delete_url = match thread_query {
                Some(tq) => format!("{base_url}/messages/{webhook_msg_id}?{tq}"),
                None => format!("{base_url}/messages/{webhook_msg_id}"),
            };

            client
                .delete(&delete_url)
                .send()
                .await
                .map_err(|err| format!("Network error deleting Discord webhook message: {err}"))?
        } else if let Some((channel_id, message_id)) = post_id.split_once('/') {
            let bot_token = require_non_empty(creds, "bot_token", "Bot Token")?;
            client
                .delete(format!(
                    "https://discord.com/api/v10/channels/{channel_id}/messages/{message_id}"
                ))
                .header(reqwest::header::AUTHORIZATION, format!("Bot {bot_token}"))
                .send()
                .await
                .map_err(|err| format!("Network error deleting Discord Bot message: {err}"))?
        } else {
            return Err(format!("Invalid Discord post ID '{post_id}'."));
        };

        let status = res.status();
        if status.is_success() || status.as_u16() == 404 {
            return Ok(());
        }
        let body = res.text().await.unwrap_or_default();
        Err(format!("Discord delete message failed (HTTP {status}): {body}"))
    }
}

inventory::submit! {
    PlatformEntry::new(|| Box::new(DiscordPlatform))
}
