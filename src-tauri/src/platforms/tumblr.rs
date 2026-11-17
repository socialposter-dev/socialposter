use crate::core::platform::{
    AuthField, AuthMode, AuthorLink, FieldKind, PlatformAuthor, PlatformCapabilities,
    PlatformEntry, PostOutcome, PostPayload, SocialPlatform,
};
use async_trait::async_trait;
use reqwest::multipart::{Form, Part};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::HashMap;

pub struct TumblrPlatform;

#[derive(Debug, Deserialize)]
struct TumblrMeta {
    status: u16,
    msg: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TumblrEnvelope<T> {
    meta: TumblrMeta,
    response: Option<T>,
}

#[derive(Debug, Deserialize)]
struct TumblrCreatePostResponse {
    id_string: Option<String>,
    id: Option<Value>,
}

fn normalise_blog_identifier(creds: &HashMap<String, String>) -> Result<String, String> {
    let raw = creds
        .get("blog_identifier")
        .map(String::as_str)
        .unwrap_or("")
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/');

    if raw.is_empty() {
        return Err("Missing Tumblr Blog Identifier.".to_string());
    }
    Ok(raw.to_string())
}

fn require_token(creds: &HashMap<String, String>) -> Result<&str, String> {
    creds
        .get("access_token")
        .map(String::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "Missing Tumblr Access Token.".to_string())
}

#[async_trait]
impl SocialPlatform for TumblrPlatform {
    fn id(&self) -> &'static str {
        "tumblr"
    }

    fn name(&self) -> &'static str {
        "Tumblr"
    }

    fn accent_colour(&self) -> &'static str {
        "#001935"
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
            label: "OAuth Bearer Token",
            description: Some("Post to your Tumblr blog using an OAuth 2.0 Bearer token."),
            fields: vec![
                AuthField {
                    key: "blog_identifier",
                    label: "Blog Identifier",
                    placeholder: "myblog.tumblr.com",
                    kind: FieldKind::Text,
                    required: true,
                    help_text: None,
                },
                AuthField {
                    key: "access_token",
                    label: "OAuth Bearer Token",
                    placeholder: "Paste your Tumblr OAuth2 bearer token",
                    kind: FieldKind::Secret,
                    required: true,
                    help_text: None,
                },
            ],
            setup_instructions: Some(
                "### How to set up Tumblr\n\n\
                 1. Register an application at [tumblr.com/oauth/apps](https://www.tumblr.com/oauth/apps).\n\
                 2. Obtain an OAuth 2.0 **Bearer Token** with the `write` scope for your account.\n\
                 3. Enter your **Blog Identifier** (for example, `myblog.tumblr.com`) and **OAuth Bearer Token** on the left.\n\
                 4. Click **Save** and **Test Connection**.",
            ),
        }]
    }

    fn setup_instructions(&self) -> &'static str {
        "### How to set up Tumblr\n\n\
         Register an app at `tumblr.com/oauth/apps` and supply your blog identifier and OAuth 2.0 Bearer token."
    }

    fn capabilities(&self) -> PlatformCapabilities {
        PlatformCapabilities {
            max_chars: 4096,
            max_images: 10,
            max_image_bytes: Some(10_000_000),
            supports_video: false,
            requires_media: false,
            supports_threads: false,
            supports_delete: true,
        }
    }

    async fn verify_credentials(&self, creds: &HashMap<String, String>) -> Result<(), String> {
        let _ = normalise_blog_identifier(creds)?;
        let token = require_token(creds)?;
        let client = Client::new();

        let res = client
            .get("https://api.tumblr.com/v2/user/info")
            .bearer_auth(token)
            .send()
            .await
            .map_err(|err| format!("Network error connecting to Tumblr: {err}"))?;

        let status = res.status();
        let envelope: TumblrEnvelope<Value> = res
            .json()
            .await
            .map_err(|err| format!("Invalid Tumblr response (HTTP {status}): {err}"))?;

        if status.is_success() && envelope.meta.status == 200 {
            Ok(())
        } else {
            Err(format!(
                "Tumblr verification failed (HTTP {}): {}",
                envelope.meta.status,
                envelope.meta.msg.unwrap_or_else(|| "Unauthorised".to_string())
            ))
        }
    }

    async fn publish(
        &self,
        post: &PostPayload,
        creds: &HashMap<String, String>,
    ) -> Result<PostOutcome, String> {
        let blog_identifier = normalise_blog_identifier(creds)?;
        let token = require_token(creds)?;
        let client = Client::new();

        let mut content_blocks: Vec<Value> = Vec::new();
        if !post.text.trim().is_empty() {
            content_blocks.push(json!({
                "type": "text",
                "text": post.text,
            }));
        }

        let mut file_parts: Vec<(String, Part)> = Vec::new();
        for (idx, attachment) in post.media.iter().take(10).enumerate() {
            if !attachment.mime_type.starts_with("image/") {
                return Err(format!(
                    "Tumblr integration currently only supports image attachments (got '{}').",
                    attachment.mime_type
                ));
            }

            let bytes = attachment.read_bytes().await?;

            let filename = attachment
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("attachment")
                .to_string();

            let identifier = format!("media_{idx}");
            let part = Part::bytes(bytes)
                .file_name(filename)
                .mime_str(&attachment.mime_type)
                .map_err(|err| {
                    format!("Invalid media MIME type '{}': {err}", attachment.mime_type)
                })?;

            let mut image_block = Map::new();
            image_block.insert("type".to_string(), Value::String("image".to_string()));
            image_block.insert(
                "media".to_string(),
                json!([{
                    "type": attachment.mime_type,
                    "identifier": identifier,
                }]),
            );
            if let Some(alt) = attachment
                .alt_text
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                image_block.insert("alt_text".to_string(), Value::String(alt.to_string()));
            }
            content_blocks.push(Value::Object(image_block));
            file_parts.push((identifier, part));
        }

        let json_payload = json!({
            "content": content_blocks,
            "state": "published",
        })
        .to_string();

        let mut form = Form::new().text("json", json_payload);
        for (field_name, part) in file_parts {
            form = form.part(field_name, part);
        }

        let res = client
            .post(format!(
                "https://api.tumblr.com/v2/blog/{blog_identifier}/posts"
            ))
            .bearer_auth(token)
            .multipart(form)
            .send()
            .await
            .map_err(|err| format!("Failed to publish post to Tumblr: {err}"))?;

        let status = res.status();
        let envelope: TumblrEnvelope<TumblrCreatePostResponse> = res
            .json()
            .await
            .map_err(|err| format!("Failed to parse Tumblr post response (HTTP {status}): {err}"))?;

        if !status.is_success() || envelope.meta.status >= 400 {
            return Err(format!(
                "Tumblr publish failed (HTTP {}): {}",
                envelope.meta.status,
                envelope.meta.msg.unwrap_or_else(|| "Unknown error".to_string())
            ));
        }

        let post_id = envelope.response.and_then(|r| {
            r.id_string.or_else(|| {
                r.id.and_then(|val| match val {
                    Value::String(s) => Some(s),
                    Value::Number(n) => Some(n.to_string()),
                    _ => None,
                })
            })
        });

        let post_url = post_id
            .as_deref()
            .map(|id| format!("https://{blog_identifier}/post/{id}"));

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
        let blog_identifier = normalise_blog_identifier(creds)?;
        let token = require_token(creds)?;
        let client = Client::new();

        let res = client
            .post(format!(
                "https://api.tumblr.com/v2/blog/{blog_identifier}/post/delete"
            ))
            .bearer_auth(token)
            .form(&[("id", post_id)])
            .send()
            .await
            .map_err(|err| format!("Network error deleting Tumblr post: {err}"))?;

        let status = res.status();
        if status.is_success() || status.as_u16() == 404 {
            return Ok(());
        }
        let body = res.text().await.unwrap_or_default();
        Err(format!("Tumblr delete post failed (HTTP {status}): {body}"))
    }
}

inventory::submit! {
    PlatformEntry::new(|| Box::new(TumblrPlatform))
}
