use crate::core::platform::{
    AuthField, AuthMode, AuthorLink, FieldKind, MediaAttachment, PlatformAuthor,
    PlatformCapabilities, PlatformEntry, PostOutcome, PostPayload, SelectOption, SocialPlatform,
};
use async_trait::async_trait;
use reqwest::multipart::{Form, Part};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;

pub struct TelegramPlatform;

#[derive(Debug, Deserialize)]
struct TelegramResponse<T> {
    ok: bool,
    description: Option<String>,
    result: Option<T>,
}

#[derive(Debug, Deserialize)]
struct TelegramChat {
    username: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TelegramMessage {
    message_id: i64,
    chat: TelegramChat,
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
        .ok_or_else(|| format!("Missing Telegram {label}."))
}

fn build_post_url(msg: &TelegramMessage) -> Option<String> {
    msg.chat
        .username
        .as_deref()
        .filter(|u| !u.is_empty())
        .map(|username| format!("https://t.me/{username}/{}", msg.message_id))
}

async fn read_attachment_part(attachment: &MediaAttachment) -> Result<Part, String> {
    let bytes = attachment.read_bytes().await?;

    let filename = attachment
        .path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("attachment")
        .to_string();

    Part::bytes(bytes)
        .file_name(filename)
        .mime_str(&attachment.mime_type)
        .map_err(|err| format!("Invalid media MIME type '{}': {err}", attachment.mime_type))
}

#[async_trait]
impl SocialPlatform for TelegramPlatform {
    fn id(&self) -> &'static str {
        "telegram"
    }

    fn name(&self) -> &'static str {
        "Telegram"
    }

    fn accent_colour(&self) -> &'static str {
        "#26A5E4"
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
        vec![AuthMode {
            id: "bot",
            label: "Bot Token",
            description: Some("Post to a Telegram channel or group using a BotFather token."),
            fields: vec![
                AuthField {
                    key: "bot_token",
                    label: "Bot Token",
                    placeholder: "123456:ABC-DEF...",
                    kind: FieldKind::Secret,
                    required: true,
                    help_text: Some("Create a bot via @BotFather and paste the HTTP API token."),
                },
                AuthField {
                    key: "chat_id",
                    label: "Channel or Chat ID",
                    placeholder: "@yourchannel or -1001234567890",
                    kind: FieldKind::Text,
                    required: true,
                    help_text: Some(
                        "Use @channelusername for public channels or the numeric -100... ID for private groups.",
                    ),
                },
                AuthField {
                    key: "silent",
                    label: "Notification Sound",
                    placeholder: "",
                    kind: FieldKind::Select {
                        options: vec![
                            SelectOption {
                                value: "false",
                                label: "Normal notification",
                            },
                            SelectOption {
                                value: "true",
                                label: "Silent notification",
                            },
                        ],
                    },
                    required: false,
                    help_text: None,
                },
            ],
            setup_instructions: None,
        }]
    }

    fn setup_instructions(&self) -> &'static str {
        "### How to set up Telegram\n\n\
         1. Open Telegram and message [@BotFather](https://t.me/BotFather), then send `/newbot` and follow the prompts.\n\
         2. Copy the **HTTP API token** BotFather gives you and paste it into **Bot Token**.\n\
         3. Add your new bot as an **Administrator** of your target channel or group (with permission to post messages).\n\
         4. Enter your public `@channelusername` (or numeric `-100...` chat ID for private chats) into **Channel or Chat ID**.\n\
         5. Click **Save** and **Test Connection**."
    }

    fn capabilities(&self) -> PlatformCapabilities {
        PlatformCapabilities {
            max_chars: 4096,
            max_images: 10,
            max_image_bytes: Some(10_000_000),
            supports_video: true,
            requires_media: false,
            supports_threads: false,
            supports_delete: true,
        }
    }

    async fn verify_credentials(&self, creds: &HashMap<String, String>) -> Result<(), String> {
        let bot_token = require_non_empty(creds, "bot_token", "Bot Token")?;
        let chat_id = require_non_empty(creds, "chat_id", "Channel or Chat ID")?;
        let client = Client::new();

        let me_res = client
            .get(format!("https://api.telegram.org/bot{bot_token}/getMe"))
            .send()
            .await
            .map_err(|err| format!("Network error connecting to Telegram: {err}"))?;

        let me_status = me_res.status();
        let me_body: TelegramResponse<Value> = me_res
            .json()
            .await
            .map_err(|err| format!("Invalid Telegram getMe response (HTTP {me_status}): {err}"))?;

        if !me_status.is_success() || !me_body.ok {
            return Err(format!(
                "Invalid Telegram Bot Token: {}",
                me_body
                    .description
                    .unwrap_or_else(|| format!("HTTP {me_status}"))
            ));
        }

        let chat_res = client
            .get(format!("https://api.telegram.org/bot{bot_token}/getChat"))
            .query(&[("chat_id", chat_id)])
            .send()
            .await
            .map_err(|err| format!("Network error verifying Telegram chat: {err}"))?;

        let chat_status = chat_res.status();
        let chat_body: TelegramResponse<Value> = chat_res
            .json()
            .await
            .map_err(|err| format!("Invalid Telegram getChat response (HTTP {chat_status}): {err}"))?;

        if !chat_status.is_success() || !chat_body.ok {
            return Err(format!(
                "Telegram chat verification failed: {}",
                chat_body
                    .description
                    .unwrap_or_else(|| format!("HTTP {chat_status}"))
            ));
        }

        Ok(())
    }

    async fn publish(
        &self,
        post: &PostPayload,
        creds: &HashMap<String, String>,
    ) -> Result<PostOutcome, String> {
        let bot_token = require_non_empty(creds, "bot_token", "Bot Token")?;
        let chat_id = require_non_empty(creds, "chat_id", "Channel or Chat ID")?;
        let disable_notification = creds.get("silent").map(String::as_str) == Some("true");

        if !post.media.is_empty() && post.text.chars().count() > 1024 {
            return Err(
                "Telegram limits captions on media posts to 1,024 characters.".to_string(),
            );
        }

        let client = Client::new();
        let base = format!("https://api.telegram.org/bot{bot_token}");
        let trimmed_text = post.text.trim();

        let (post_id, post_url) = if post.media.is_empty() {
            let res = client
                .post(format!("{base}/sendMessage"))
                .json(&json!({
                    "chat_id": chat_id,
                    "text": post.text,
                    "disable_notification": disable_notification,
                }))
                .send()
                .await
                .map_err(|err| format!("Failed to send Telegram message: {err}"))?;

            let status = res.status();
            let parsed: TelegramResponse<TelegramMessage> = res
                .json()
                .await
                .map_err(|err| format!("Failed to parse Telegram response: {err}"))?;

            if !status.is_success() || !parsed.ok {
                return Err(format!(
                    "Telegram sendMessage failed: {}",
                    parsed
                        .description
                        .unwrap_or_else(|| format!("HTTP {status}"))
                ));
            }

            (
                parsed.result.as_ref().map(|m| m.message_id.to_string()),
                parsed.result.as_ref().and_then(build_post_url),
            )
        } else if post.media.len() == 1 {
            let attachment = &post.media[0];
            let is_video = attachment.mime_type.starts_with("video/");
            let (endpoint, field_name) = if is_video {
                ("sendVideo", "video")
            } else {
                ("sendPhoto", "photo")
            };

            let file_part = read_attachment_part(attachment).await?;
            let mut form = Form::new()
                .text("chat_id", chat_id.to_string())
                .text("disable_notification", disable_notification.to_string())
                .part(field_name, file_part);

            if !trimmed_text.is_empty() {
                form = form.text("caption", post.text.clone());
            }

            let res = client
                .post(format!("{base}/{endpoint}"))
                .multipart(form)
                .send()
                .await
                .map_err(|err| format!("Failed to send Telegram media: {err}"))?;

            let status = res.status();
            let parsed: TelegramResponse<TelegramMessage> = res
                .json()
                .await
                .map_err(|err| format!("Failed to parse Telegram media response: {err}"))?;

            if !status.is_success() || !parsed.ok {
                return Err(format!(
                    "Telegram {endpoint} failed: {}",
                    parsed
                        .description
                        .unwrap_or_else(|| format!("HTTP {status}"))
                ));
            }

            (
                parsed.result.as_ref().map(|m| m.message_id.to_string()),
                parsed.result.as_ref().and_then(build_post_url),
            )
        } else {
            let mut form = Form::new()
                .text("chat_id", chat_id.to_string())
                .text("disable_notification", disable_notification.to_string());

            let mut media_items: Vec<Value> = Vec::new();
            for (idx, attachment) in post.media.iter().take(10).enumerate() {
                let attach_key = format!("file{idx}");
                let file_part = read_attachment_part(attachment).await?;
                form = form.part(attach_key.clone(), file_part);

                let media_type = if attachment.mime_type.starts_with("video/") {
                    "video"
                } else {
                    "photo"
                };

                let mut item = json!({
                    "type": media_type,
                    "media": format!("attach://{attach_key}"),
                });
                if idx == 0 && !trimmed_text.is_empty() {
                    item["caption"] = Value::String(post.text.clone());
                }
                media_items.push(item);
            }

            form = form.text("media", Value::Array(media_items).to_string());

            let res = client
                .post(format!("{base}/sendMediaGroup"))
                .multipart(form)
                .send()
                .await
                .map_err(|err| format!("Failed to send Telegram media group: {err}"))?;

            let status = res.status();
            let parsed: TelegramResponse<Vec<TelegramMessage>> = res
                .json()
                .await
                .map_err(|err| format!("Failed to parse Telegram media group response: {err}"))?;

            if !status.is_success() || !parsed.ok {
                return Err(format!(
                    "Telegram sendMediaGroup failed: {}",
                    parsed
                        .description
                        .unwrap_or_else(|| format!("HTTP {status}"))
                ));
            }

            let first_msg = parsed.result.as_deref().and_then(|msgs| msgs.first());
            (
                first_msg.map(|m| m.message_id.to_string()),
                first_msg.and_then(build_post_url),
            )
        };

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
        let bot_token = require_non_empty(creds, "bot_token", "Bot Token")?;
        let chat_id = require_non_empty(creds, "chat_id", "Channel or Chat ID")?;
        let parsed_message_id: i64 = post_id
            .trim()
            .parse()
            .map_err(|_| format!("Invalid Telegram message ID '{post_id}'."))?;

        let client = Client::new();
        let res = client
            .post(format!(
                "https://api.telegram.org/bot{bot_token}/deleteMessage"
            ))
            .json(&json!({
                "chat_id": chat_id,
                "message_id": parsed_message_id,
            }))
            .send()
            .await
            .map_err(|err| format!("Network error deleting Telegram message: {err}"))?;

        let status = res.status();
        let parsed: TelegramResponse<Value> = res
            .json()
            .await
            .map_err(|err| format!("Invalid Telegram deleteMessage response (HTTP {status}): {err}"))?;

        if status.is_success() && parsed.ok {
            return Ok(());
        }
        let desc = parsed
            .description
            .unwrap_or_else(|| format!("HTTP {status}"));
        if desc.to_lowercase().contains("message to delete not found") {
            return Ok(());
        }
        Err(format!("Telegram deleteMessage failed: {desc}"))
    }
}

inventory::submit! {
    PlatformEntry::new(|| Box::new(TelegramPlatform))
}
