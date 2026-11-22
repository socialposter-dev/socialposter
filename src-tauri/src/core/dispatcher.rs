use crate::core::platform::{MediaAttachment, PostOutcome, PostPayload};
use crate::core::{media, registry, threading, vault};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::task::JoinSet;

const PLATFORM_PUBLISH_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformDispatchRequest {
    pub platform_id: String,
    pub text: String,
    pub media: Vec<MediaAttachment>,
}

fn failure_outcome(platform_id: String, platform_name: String, error: String) -> PostOutcome {
    PostOutcome {
        platform_id,
        platform_name,
        success: false,
        error: Some(error),
        ..Default::default()
    }
}

/// Dispatches posts concurrently across all requested platforms using `JoinSet`.
/// Each platform runs in an isolated task with its own timeout and per-platform
/// media compression; when `auto_thread` is enabled on a thread-capable platform
/// whose `max_chars` is exceeded, splits the post into a chained reply thread.
pub async fn dispatch_all(
    profile_id: &str,
    targets: Vec<PlatformDispatchRequest>,
    auto_thread: bool,
    auto_compress: bool,
) -> Vec<PostOutcome> {
    let mut set: JoinSet<PostOutcome> = JoinSet::new();
    let mut outcomes: Vec<PostOutcome> = Vec::with_capacity(targets.len());

    for target in targets {
        let Some(platform) = registry::find_platform(&target.platform_id) else {
            outcomes.push(failure_outcome(
                target.platform_id.clone(),
                target.platform_id,
                "Platform plugin is not registered.".to_string(),
            ));
            continue;
        };

        let platform_id = platform.id().to_string();
        let platform_name = platform.name().to_string();

        let (is_compatible, compatibility_error) = registry::check_compatibility(&*platform);
        if !is_compatible {
            outcomes.push(failure_outcome(
                platform_id,
                platform_name,
                compatibility_error.unwrap_or_else(|| {
                    "Platform plugin is incompatible with this version of SocialPoster.".to_string()
                }),
            ));
            continue;
        }

        let auth_modes = platform.auth_modes();

        let creds = match vault::load_platform_credentials(profile_id, &platform_id, &auth_modes) {
            Ok(creds) => creds,
            Err(err) => {
                outcomes.push(failure_outcome(platform_id, platform_name, err));
                continue;
            }
        };

        let caps = platform.capabilities();
        let max_image_bytes = caps.max_image_bytes;

        set.spawn(async move {
            let raw_media = target.media;
            let prep_result = tokio::task::spawn_blocking(move || {
                media::prepare_media_for_platform_with_options(
                    &raw_media,
                    max_image_bytes,
                    auto_compress,
                )
            })
            .await;

            let (prepared_media, mut notes) = match prep_result {
                Ok(Ok(pair)) => pair,
                Ok(Err(err)) => return failure_outcome(platform_id, platform_name, err),
                Err(err) => {
                    return failure_outcome(
                        platform_id,
                        platform_name,
                        format!("Media preparation task failed: {err}"),
                    );
                }
            };

            let should_thread =
                auto_thread && caps.supports_threads && target.text.chars().count() > caps.max_chars;

            if should_thread {
                let chunks = threading::split_into_thread(&target.text, caps.max_chars, true);
                let total = chunks.len();

                if total > 1 {
                    let mut chunk_iter = chunks.into_iter();
                    let first_text = chunk_iter.next().unwrap_or_default();
                    let first_payload = PostPayload {
                        text: first_text,
                        media: prepared_media,
                        reply_root_id: None,
                        reply_parent_id: None,
                    };

                    let first_outcome = match tokio::time::timeout(
                        PLATFORM_PUBLISH_TIMEOUT,
                        platform.publish(&first_payload, &creds),
                    )
                    .await
                    {
                        Ok(Ok(outcome)) => outcome,
                        Ok(Err(err)) => {
                            let mut out = failure_outcome(platform_id, platform_name, err);
                            out.notes = notes;
                            return out;
                        }
                        Err(_) => {
                            let mut out = failure_outcome(
                                platform_id,
                                platform_name,
                                format!(
                                    "Publish request timed out after {} seconds.",
                                    PLATFORM_PUBLISH_TIMEOUT.as_secs()
                                ),
                            );
                            out.notes = notes;
                            return out;
                        }
                    };

                    let root_url = first_outcome.post_url.clone();
                    let Some(root_id) = first_outcome.post_id.clone() else {
                        let mut out = failure_outcome(
                            platform_id,
                            platform_name,
                            "Platform did not return a post ID for thread chaining.".to_string(),
                        );
                        out.post_url = root_url;
                        out.notes = notes;
                        return out;
                    };

                    let mut parent_id = root_id.clone();
                    let mut thread_post_ids = vec![root_id.clone()];

                    for (idx, chunk_text) in chunk_iter.enumerate() {
                        let published_so_far = idx + 1;
                        let reply_payload = PostPayload {
                            text: chunk_text,
                            media: Vec::new(),
                            reply_root_id: Some(root_id.clone()),
                            reply_parent_id: Some(parent_id.clone()),
                        };

                        let reply_result = tokio::time::timeout(
                            PLATFORM_PUBLISH_TIMEOUT,
                            platform.publish(&reply_payload, &creds),
                        )
                        .await;

                        match reply_result {
                            Ok(Ok(next_outcome)) => {
                                if let Some(next_id) = next_outcome.post_id {
                                    thread_post_ids.push(next_id.clone());
                                    parent_id = next_id;
                                }
                            }
                            Ok(Err(err)) => {
                                let mut out = failure_outcome(
                                    platform_id,
                                    platform_name,
                                    format!(
                                        "Published {published_so_far}/{total} posts before error: {err}"
                                    ),
                                );
                                out.post_id = Some(root_id);
                                out.post_ids = thread_post_ids;
                                out.post_url = root_url;
                                out.notes = notes;
                                return out;
                            }
                            Err(_) => {
                                let mut out = failure_outcome(
                                    platform_id,
                                    platform_name,
                                    format!(
                                        "Published {published_so_far}/{total} posts before error: timed out after {} seconds.",
                                        PLATFORM_PUBLISH_TIMEOUT.as_secs()
                                    ),
                                );
                                out.post_id = Some(root_id);
                                out.post_ids = thread_post_ids;
                                out.post_url = root_url;
                                out.notes = notes;
                                return out;
                            }
                        }
                    }

                    notes.push(format!("Published as {total}-part thread"));
                    return PostOutcome {
                        platform_id,
                        platform_name,
                        success: true,
                        post_id: Some(root_id),
                        post_ids: thread_post_ids,
                        post_url: root_url,
                        notes,
                        ..Default::default()
                    };
                }
            }

            let prepared_payload = PostPayload {
                text: target.text,
                media: prepared_media,
                reply_root_id: None,
                reply_parent_id: None,
            };

            match tokio::time::timeout(
                PLATFORM_PUBLISH_TIMEOUT,
                platform.publish(&prepared_payload, &creds),
            )
            .await
            {
                Ok(Ok(mut outcome)) => {
                    if outcome.post_ids.is_empty()
                        && let Some(ref id) = outcome.post_id
                    {
                        outcome.post_ids = vec![id.clone()];
                    }
                    outcome.notes.extend(notes);
                    outcome
                }
                Ok(Err(err)) => {
                    let mut out = failure_outcome(platform_id, platform_name, err);
                    out.notes = notes;
                    out
                }
                Err(_) => {
                    let mut out = failure_outcome(
                        platform_id,
                        platform_name,
                        format!(
                            "Publish request timed out after {} seconds.",
                            PLATFORM_PUBLISH_TIMEOUT.as_secs()
                        ),
                    );
                    out.notes = notes;
                    out
                }
            }
        });
    }

    while let Some(joined) = set.join_next().await {
        match joined {
            Ok(outcome) => outcomes.push(outcome),
            Err(join_err) => outcomes.push(failure_outcome(
                "unknown".to_string(),
                "Unknown Platform".to_string(),
                format!("Platform dispatch task failed unexpectedly: {join_err}"),
            )),
        }
    }

    outcomes
}
