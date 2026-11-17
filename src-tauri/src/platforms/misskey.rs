use crate::core::platform::{
    AuthField, AuthMode, AuthorLink, FieldKind, PlatformAuthor, PlatformCapabilities,
    PlatformEntry, PostOutcome, PostPayload, SelectOption, SocialPlatform,
};
use async_trait::async_trait;
use reqwest::multipart::{Form, Part};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::HashMap;

pub struct MisskeyPlatform;

#[derive(Debug, Deserialize)]
struct DriveFileResponse {
    id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateNoteResponse {
    created_note: CreatedNote,
}

#[derive(Debug, Deserialize)]
struct CreatedNote {
    id: String,
}

fn normalise_instance_url(creds: &HashMap<String, String>) -> Result<String, String> {
    let raw = creds
        .get("instance_url")
        .map(String::as_str)
        .unwrap_or("")
        .trim()
        .trim_end_matches('/');

    if raw.is_empty() {
        return Err("Missing Misskey / Sharkey Instance URL.".to_string());
    }
    if raw.starts_with("http://") {
        return Err("Misskey / Sharkey Instance URL must use https://.".to_string());
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
        .ok_or_else(|| "Missing Misskey / Sharkey API Token.".to_string())
}

#[async_trait]
impl SocialPlatform for MisskeyPlatform {
    fn id(&self) -> &'static str {
        "misskey"
    }

    fn name(&self) -> &'static str {
        "Misskey / Sharkey"
    }

    fn accent_colour(&self) -> &'static str {
        "#96D04A"
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
            id: "token",
            label: "API Token",
            description: Some("Connect to any Misskey, Sharkey, or Firefish server."),
            fields: vec![
                AuthField {
                    key: "instance_url",
                    label: "Instance URL",
                    placeholder: "https://misskey.io",
                    kind: FieldKind::Url,
                    required: true,
                    help_text: None,
                },
                AuthField {
                    key: "access_token",
                    label: "API Token",
                    placeholder: "Paste your Misskey API token",
                    kind: FieldKind::Secret,
                    required: true,
                    help_text: None,
                },
                AuthField {
                    key: "visibility",
                    label: "Note Visibility",
                    placeholder: "",
                    kind: FieldKind::Select {
                        options: vec![
                            SelectOption {
                                value: "public",
                                label: "Public",
                            },
                            SelectOption {
                                value: "home",
                                label: "Home",
                            },
                            SelectOption {
                                value: "followers",
                                label: "Followers",
                            },
                        ],
                    },
                    required: false,
                    help_text: None,
                },
            ],
            setup_instructions: Some(
                "### How to set up Misskey / Sharkey\n\n\
                 1. Sign in to your Misskey, Sharkey, or Firefish instance (for example, `https://misskey.io`).\n\
                 2. Open **Settings → API** and click **Generate access token**.\n\
                 3. Name the token `SocialPoster` and enable permissions for **Drive** (view/write files) and **Notes** (compose/delete notes).\n\
                 4. Copy the generated token, enter your **Instance URL** and **API Token** on the left, and click **Save** and **Test Connection**.",
            ),
        }]
    }

    fn setup_instructions(&self) -> &'static str {
        "### How to set up Misskey / Sharkey\n\n\
         Generate an API token under **Settings → API** with Drive and Notes permissions."
    }

    fn capabilities(&self) -> PlatformCapabilities {
        PlatformCapabilities {
            max_chars: 3000,
            max_images: 16,
            max_image_bytes: Some(10_000_000),
            supports_video: true,
            requires_media: false,
            supports_threads: true,
            supports_delete: true,
        }
    }

    async fn verify_credentials(&self, creds: &HashMap<String, String>) -> Result<(), String> {
        let instance_url = normalise_instance_url(creds)?;
        let token = require_token(creds)?;
        let client = Client::new();

        let res = client
            .post(format!("{instance_url}/api/i"))
            .json(&json!({ "i": token }))
            .send()
            .await
            .map_err(|err| format!("Network error connecting to Misskey: {err}"))?;

        let status = res.status();
        if status.is_success() {
            Ok(())
        } else {
            let body = res.text().await.unwrap_or_default();
            Err(format!("Misskey verification failed (HTTP {status}): {body}"))
        }
    }

    async fn publish(
        &self,
        post: &PostPayload,
        creds: &HashMap<String, String>,
    ) -> Result<PostOutcome, String> {
        let instance_url = normalise_instance_url(creds)?;
        let token = require_token(creds)?;
        let visibility = creds
            .get("visibility")
            .map(String::as_str)
            .filter(|v| matches!(*v, "public" | "home" | "followers"))
            .unwrap_or("public");

        let client = Client::new();
        let mut file_ids: Vec<String> = Vec::new();

        for attachment in post.media.iter().take(16) {
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

            let mut form = Form::new()
                .text("i", token.to_string())
                .part("file", file_part);

            if let Some(alt) = attachment
                .alt_text
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                form = form.text("comment", alt.to_string());
            }

            let upload_res = client
                .post(format!("{instance_url}/api/drive/files/create"))
                .multipart(form)
                .send()
                .await
                .map_err(|err| format!("Failed to upload media to Misskey Drive: {err}"))?;

            let upload_status = upload_res.status();
            if !upload_status.is_success() {
                let body = upload_res.text().await.unwrap_or_default();
                return Err(format!(
                    "Misskey Drive upload failed (HTTP {upload_status}): {body}"
                ));
            }

            let file = upload_res
                .json::<DriveFileResponse>()
                .await
                .map_err(|err| format!("Failed to parse Misskey Drive response: {err}"))?;
            file_ids.push(file.id);
        }

        let mut note_payload = Map::new();
        note_payload.insert("i".to_string(), Value::String(token.to_string()));
        note_payload.insert(
            "visibility".to_string(),
            Value::String(visibility.to_string()),
        );

        if !post.text.trim().is_empty() || file_ids.is_empty() {
            note_payload.insert("text".to_string(), Value::String(post.text.clone()));
        }
        if !file_ids.is_empty() {
            note_payload.insert("fileIds".to_string(), json!(file_ids));
        }
        if let Some(ref parent_id) = post.reply_parent_id {
            note_payload.insert("replyId".to_string(), Value::String(parent_id.clone()));
        }

        let note_res = client
            .post(format!("{instance_url}/api/notes/create"))
            .json(&Value::Object(note_payload))
            .send()
            .await
            .map_err(|err| format!("Failed to create Misskey note: {err}"))?;

        let note_status = note_res.status();
        if !note_status.is_success() {
            let body = note_res.text().await.unwrap_or_default();
            return Err(format!(
                "Misskey note publish failed (HTTP {note_status}): {body}"
            ));
        }

        let created = note_res
            .json::<CreateNoteResponse>()
            .await
            .map_err(|err| format!("Failed to parse Misskey note response: {err}"))?;

        let note_id = created.created_note.id;

        Ok(PostOutcome {
            platform_id: self.id().to_string(),
            platform_name: self.name().to_string(),
            success: true,
            post_id: Some(note_id.clone()),
            post_url: Some(format!("{instance_url}/notes/{note_id}")),
            ..Default::default()
        })
    }

    async fn delete_post(
        &self,
        creds: &HashMap<String, String>,
        post_id: &str,
    ) -> Result<(), String> {
        let instance_url = normalise_instance_url(creds)?;
        let token = require_token(creds)?;
        let client = Client::new();

        let res = client
            .post(format!("{instance_url}/api/notes/delete"))
            .json(&json!({
                "i": token,
                "noteId": post_id,
            }))
            .send()
            .await
            .map_err(|err| format!("Network error deleting Misskey note: {err}"))?;

        let status = res.status();
        if status.is_success() || status.as_u16() == 404 {
            return Ok(());
        }
        let body = res.text().await.unwrap_or_default();
        if body.contains("NO_SUCH_NOTE") {
            return Ok(());
        }
        Err(format!("Misskey delete note failed (HTTP {status}): {body}"))
    }
}

inventory::submit! {
    PlatformEntry::new(|| Box::new(MisskeyPlatform))
}
