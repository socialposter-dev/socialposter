use crate::core::dispatcher::{self, PlatformDispatchRequest};
use crate::core::history::{self, HistoryEntry};
use crate::core::platform::{MediaAttachment, PostOutcome};
use crate::core::profiles::{self, ProfileSessionState};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedMediaRef {
    pub filename: String,
    pub mime_type: String,
    #[serde(default)]
    pub alt_text: Option<String>,
    pub relative_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledPost {
    pub id: String,
    pub profile_id: String,
    pub scheduled_for: String,
    pub text: String,
    pub overrides: HashMap<String, String>,
    pub media_refs: Vec<SavedMediaRef>,
    pub platform_ids: Vec<String>,
    pub auto_thread: bool,
    pub status: String,
    pub error: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledPostsResponse {
    pub is_locked: bool,
    pub posts: Vec<ScheduledPost>,
}

/// Serialises synchronous `schedule.json` read-modify-write operations across tasks.
#[derive(Default)]
pub struct ScheduleFileLock(pub Mutex<()>);

fn schedule_file_path(profile_dir: &Path) -> PathBuf {
    profile_dir.join("schedule.json")
}

pub fn schedule_media_job_dir(profile_dir: &Path, job_id: &str) -> PathBuf {
    profile_dir.join("schedule_media").join(job_id)
}

pub fn load_schedule_from_dir(profile_dir: &Path) -> Result<Vec<ScheduledPost>, String> {
    let path = schedule_file_path(profile_dir);
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let raw =
        fs::read_to_string(&path).map_err(|err| format!("Failed to read schedule.json: {err}"))?;
    let mut jobs: Vec<ScheduledPost> = serde_json::from_str(&raw)
        .map_err(|err| format!("Failed to parse schedule.json: {err}"))?;
    jobs.sort_by(|a, b| a.scheduled_for.cmp(&b.scheduled_for));
    Ok(jobs)
}

pub fn save_schedule_to_dir(profile_dir: &Path, jobs: &[ScheduledPost]) -> Result<(), String> {
    fs::create_dir_all(profile_dir)
        .map_err(|err| format!("Failed to create profile directory: {err}"))?;
    let path = schedule_file_path(profile_dir);
    let json = serde_json::to_string_pretty(jobs)
        .map_err(|err| format!("Failed to serialise schedule.json: {err}"))?;
    fs::write(&path, json).map_err(|err| format!("Failed to write schedule.json: {err}"))
}

pub fn save_job_media(
    profile_dir: &Path,
    job_id: &str,
    media: &[MediaAttachment],
) -> Result<Vec<SavedMediaRef>, String> {
    if media.is_empty() {
        return Ok(Vec::new());
    }
    let job_dir = schedule_media_job_dir(profile_dir, job_id);
    fs::create_dir_all(&job_dir)
        .map_err(|err| format!("Failed to create schedule media directory: {err}"))?;

    let mut refs = Vec::with_capacity(media.len());
    for (idx, attachment) in media.iter().enumerate() {
        let filename = attachment
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("attachment")
            .to_string();
        let safe_name = filename.replace(['/', '\\', ':'], "_");
        let stored_name = format!("{idx}_{safe_name}");
        let dest_path = job_dir.join(&stored_name);

        let bytes = match attachment.bytes {
            Some(ref data) => data.clone(),
            None => fs::read(&attachment.path).map_err(|err| {
                format!(
                    "Failed to read attachment '{}': {err}",
                    attachment.path.display()
                )
            })?,
        };
        fs::write(&dest_path, bytes)
            .map_err(|err| format!("Failed to persist scheduled media '{stored_name}': {err}"))?;

        let relative_path = format!("schedule_media/{job_id}/{stored_name}");
        refs.push(SavedMediaRef {
            filename,
            mime_type: attachment.mime_type.clone(),
            alt_text: attachment.alt_text.clone(),
            relative_path,
        });
    }

    Ok(refs)
}

pub fn rehydrate_media(
    profile_dir: &Path,
    refs: &[SavedMediaRef],
) -> Result<Vec<MediaAttachment>, String> {
    let mut out = Vec::with_capacity(refs.len());
    for item in refs {
        let full_path = profile_dir.join(&item.relative_path);
        let bytes = fs::read(&full_path).map_err(|err| {
            format!(
                "Failed to read scheduled media '{}': {err}",
                full_path.display()
            )
        })?;
        out.push(MediaAttachment {
            path: full_path,
            mime_type: item.mime_type.clone(),
            alt_text: item.alt_text.clone(),
            bytes: Some(bytes),
        });
    }
    Ok(out)
}

pub fn cleanup_job_media(profile_dir: &Path, job_id: &str) {
    let job_dir = schedule_media_job_dir(profile_dir, job_id);
    if job_dir.exists() {
        let _ = fs::remove_dir_all(&job_dir);
    }
}

pub fn due_job_ids(jobs: &[ScheduledPost], now: DateTime<Utc>) -> Vec<String> {
    jobs.iter()
        .filter(|job| job.status == "pending")
        .filter(|job| {
            DateTime::parse_from_rfc3339(&job.scheduled_for)
                .map(|dt| dt.with_timezone(&Utc) <= now)
                .unwrap_or(false)
        })
        .map(|job| job.id.clone())
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub fn add_scheduled_job(
    app: &AppHandle,
    profile_id: &str,
    scheduled_for: String,
    text: String,
    overrides: HashMap<String, String>,
    media: &[MediaAttachment],
    platform_ids: Vec<String>,
    auto_thread: bool,
) -> Result<ScheduledPost, String> {
    let dir = profiles::profile_dir(app, profile_id)?;
    let job_id = Uuid::new_v4().to_string();
    let media_refs = save_job_media(&dir, &job_id, media)?;

    let job = ScheduledPost {
        id: job_id,
        profile_id: profile_id.to_string(),
        scheduled_for,
        text,
        overrides,
        media_refs,
        platform_ids,
        auto_thread,
        status: "pending".to_string(),
        error: None,
        created_at: Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
    };

    let lock = app.state::<ScheduleFileLock>();
    let _guard = lock
        .0
        .lock()
        .map_err(|_| "Schedule lock poisoned.".to_string())?;
    let mut jobs = load_schedule_from_dir(&dir)?;
    jobs.push(job.clone());
    jobs.sort_by(|a, b| a.scheduled_for.cmp(&b.scheduled_for));
    save_schedule_to_dir(&dir, &jobs)?;
    drop(_guard);

    refresh_tray_tooltip(app);
    Ok(job)
}

pub fn refresh_tray_tooltip(app: &AppHandle) {
    let Ok(all_profiles) = profiles::ensure_initialized(app) else {
        return;
    };
    let lock = app.state::<ScheduleFileLock>();
    let Ok(_guard) = lock.0.lock() else {
        return;
    };
    let mut pending_count = 0usize;
    for profile in all_profiles {
        if let Ok(dir) = profiles::profile_dir(app, &profile.id)
            && let Ok(jobs) = load_schedule_from_dir(&dir)
        {
            pending_count += jobs.iter().filter(|j| j.status == "pending").count();
        }
    }
    crate::core::desktop::update_tray_tooltip(app, pending_count);
}

fn recover_interrupted_jobs(jobs: &mut [ScheduledPost]) -> bool {
    let mut changed = false;
    for job in jobs.iter_mut() {
        if job.status == "publishing" {
            job.status = "failed".to_string();
            job.error = Some("Interrupted before completion".to_string());
            changed = true;
        }
    }
    changed
}

pub fn list_jobs(app: &AppHandle, profile_id: &str) -> Result<Vec<ScheduledPost>, String> {
    let dir = profiles::profile_dir(app, profile_id)?;
    let lock = app.state::<ScheduleFileLock>();
    let _guard = lock
        .0
        .lock()
        .map_err(|_| "Schedule lock poisoned.".to_string())?;
    load_schedule_from_dir(&dir)
}

pub fn cancel_job(app: &AppHandle, profile_id: &str, job_id: &str) -> Result<(), String> {
    let dir = profiles::profile_dir(app, profile_id)?;
    {
        let lock = app.state::<ScheduleFileLock>();
        let _guard = lock
            .0
            .lock()
            .map_err(|_| "Schedule lock poisoned.".to_string())?;
        let mut jobs = load_schedule_from_dir(&dir)?;
        let before = jobs.len();
        jobs.retain(|j| j.id != job_id);
        if jobs.len() == before {
            return Err("Scheduled post not found.".to_string());
        }
        save_schedule_to_dir(&dir, &jobs)?;
    }
    cleanup_job_media(&dir, job_id);
    refresh_tray_tooltip(app);
    let _ = app.emit("schedule-updated", ());
    Ok(())
}

pub fn retry_job(app: &AppHandle, profile_id: &str, job_id: &str) -> Result<ScheduledPost, String> {
    let dir = profiles::profile_dir(app, profile_id)?;
    let updated = {
        let lock = app.state::<ScheduleFileLock>();
        let _guard = lock
            .0
            .lock()
            .map_err(|_| "Schedule lock poisoned.".to_string())?;
        let mut jobs = load_schedule_from_dir(&dir)?;
        let Some(found) = jobs.iter_mut().find(|j| j.id == job_id) else {
            return Err("Scheduled post not found.".to_string());
        };
        found.status = "pending".to_string();
        found.error = None;
        let snapshot = found.clone();
        save_schedule_to_dir(&dir, &jobs)?;
        snapshot
    };
    refresh_tray_tooltip(app);
    let _ = app.emit("schedule-updated", ());
    Ok(updated)
}

pub async fn execute_scheduled_job(
    app: &AppHandle,
    profile_id: &str,
    job_id: &str,
) -> Result<Vec<PostOutcome>, String> {
    if !profiles::profile_exists(app, profile_id) {
        return Err(format!("Unknown profile '{profile_id}'."));
    }
    let dir = profiles::profile_dir(app, profile_id)?;

    let job = {
        let lock = app.state::<ScheduleFileLock>();
        let _guard = lock
            .0
            .lock()
            .map_err(|_| "Schedule lock poisoned.".to_string())?;
        let mut jobs = load_schedule_from_dir(&dir)?;
        let Some(found) = jobs.iter_mut().find(|j| j.id == job_id) else {
            return Err("Scheduled post not found.".to_string());
        };
        found.status = "publishing".to_string();
        found.error = None;
        let snapshot = found.clone();
        save_schedule_to_dir(&dir, &jobs)?;
        snapshot
    };
    let _ = app.emit("schedule-updated", ());

    let media = match rehydrate_media(&dir, &job.media_refs) {
        Ok(m) => m,
        Err(err) => {
            mark_job_failed(app, &dir, job_id, err.clone());
            return Err(err);
        }
    };

    let targets: Vec<PlatformDispatchRequest> = job
        .platform_ids
        .iter()
        .map(|pid| {
            let resolved_text = job
                .overrides
                .get(pid)
                .cloned()
                .unwrap_or_else(|| job.text.clone());
            PlatformDispatchRequest {
                platform_id: pid.clone(),
                text: resolved_text,
                media: media.clone(),
            }
        })
        .collect();

    let (allow_auto_thread, allow_auto_compress) = app
        .state::<crate::core::desktop::DesktopSettingsState>()
        .0
        .lock()
        .map(|g| (g.auto_thread_posts, g.auto_compress_images))
        .unwrap_or((true, true));
    let outcomes = dispatcher::dispatch_all(
        profile_id,
        targets,
        job.auto_thread && allow_auto_thread,
        allow_auto_compress,
    )
    .await;

    let entry = HistoryEntry {
        id: Uuid::new_v4().to_string(),
        created_at: Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        master_text: job.text.clone(),
        overrides: job.overrides.clone(),
        media,
        outcomes: outcomes.clone(),
    };
    let _ = history::append_history(app, profile_id, entry);

    let all_failed = !outcomes.is_empty() && outcomes.iter().all(|o| !o.success);
    if all_failed {
        let summary = outcomes
            .iter()
            .filter_map(|o| {
                o.error
                    .as_deref()
                    .map(|e| format!("{}: {e}", o.platform_name))
            })
            .collect::<Vec<_>>()
            .join("; ");
        mark_job_failed(app, &dir, job_id, summary);
    } else {
        {
            let lock = app.state::<ScheduleFileLock>();
            if let Ok(_guard) = lock.0.lock()
                && let Ok(mut jobs) = load_schedule_from_dir(&dir)
            {
                jobs.retain(|j| j.id != job_id);
                let _ = save_schedule_to_dir(&dir, &jobs);
            }
        }
        cleanup_job_media(&dir, job_id);
    }

    refresh_tray_tooltip(app);
    let _ = app.emit("schedule-updated", ());
    Ok(outcomes)
}

fn mark_job_failed(app: &AppHandle, dir: &Path, job_id: &str, error_msg: String) {
    {
        let lock = app.state::<ScheduleFileLock>();
        if let Ok(_guard) = lock.0.lock()
            && let Ok(mut jobs) = load_schedule_from_dir(dir)
        {
            if let Some(target) = jobs.iter_mut().find(|j| j.id == job_id) {
                target.status = "failed".to_string();
                target.error = Some(error_msg);
            }
            let _ = save_schedule_to_dir(dir, &jobs);
        }
    }
    refresh_tray_tooltip(app);
    let _ = app.emit("schedule-updated", ());
}

pub fn spawn_scheduler_worker(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // Recover any jobs left mid-publish from an unexpected shutdown before the first tick.
        if let Ok(all_profiles) = profiles::ensure_initialized(&app) {
            let mut recovered_any = false;
            {
                let lock = app.state::<ScheduleFileLock>();
                if let Ok(_guard) = lock.0.lock() {
                    for profile in all_profiles {
                        if let Ok(dir) = profiles::profile_dir(&app, &profile.id)
                            && let Ok(mut jobs) = load_schedule_from_dir(&dir)
                            && recover_interrupted_jobs(&mut jobs)
                        {
                            let _ = save_schedule_to_dir(&dir, &jobs);
                            recovered_any = true;
                        }
                    }
                }
            }
            if recovered_any {
                let _ = app.emit("schedule-updated", ());
            }
        }
        refresh_tray_tooltip(&app);

        let mut interval = tokio::time::interval(Duration::from_secs(10));
        loop {
            interval.tick().await;

            let Ok(all_profiles) = profiles::ensure_initialized(&app) else {
                continue;
            };
            let now = Utc::now();

            for profile in all_profiles {
                // Enforce profile lock: never execute due jobs for locked profiles.
                let is_unlocked = app.state::<ProfileSessionState>().is_unlocked(&profile);
                if !is_unlocked {
                    continue;
                }

                let Ok(dir) = profiles::profile_dir(&app, &profile.id) else {
                    continue;
                };

                let due_ids = {
                    let lock = app.state::<ScheduleFileLock>();
                    let Ok(_guard) = lock.0.lock() else {
                        continue;
                    };
                    let Ok(jobs) = load_schedule_from_dir(&dir) else {
                        continue;
                    };
                    due_job_ids(&jobs, now)
                };

                for job_id in due_ids {
                    if !profiles::profile_exists(&app, &profile.id) {
                        break;
                    }
                    let _ = execute_scheduled_job(&app, &profile.id, &job_id).await;
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeDelta;

    fn temp_profile_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("socialposter_test_{name}_{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).expect("creates temp dir");
        dir
    }

    #[test]
    fn persists_and_filters_due_jobs_respecting_lock_gate() {
        let dir = temp_profile_dir("due_filter");
        let now = Utc::now();
        let past = (now - TimeDelta::minutes(5))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();
        let future = (now + TimeDelta::minutes(15))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();

        let jobs = vec![
            ScheduledPost {
                id: "due-1".to_string(),
                profile_id: "default".to_string(),
                scheduled_for: past.clone(),
                text: "Due post".to_string(),
                overrides: HashMap::new(),
                media_refs: Vec::new(),
                platform_ids: vec!["bluesky".to_string()],
                auto_thread: false,
                status: "pending".to_string(),
                error: None,
                created_at: past.clone(),
            },
            ScheduledPost {
                id: "future-1".to_string(),
                profile_id: "default".to_string(),
                scheduled_for: future,
                text: "Future post".to_string(),
                overrides: HashMap::new(),
                media_refs: Vec::new(),
                platform_ids: vec!["bluesky".to_string()],
                auto_thread: false,
                status: "pending".to_string(),
                error: None,
                created_at: past.clone(),
            },
            ScheduledPost {
                id: "failed-1".to_string(),
                profile_id: "default".to_string(),
                scheduled_for: past,
                text: "Failed post".to_string(),
                overrides: HashMap::new(),
                media_refs: Vec::new(),
                platform_ids: vec!["bluesky".to_string()],
                auto_thread: false,
                status: "failed".to_string(),
                error: Some("HTTP 500".to_string()),
                created_at: "2026-01-01T00:00:00Z".to_string(),
            },
        ];

        save_schedule_to_dir(&dir, &jobs).expect("saves schedule");
        let loaded = load_schedule_from_dir(&dir).expect("loads schedule");
        assert_eq!(loaded.len(), 3);

        let due = due_job_ids(&loaded, now);
        assert_eq!(due, vec!["due-1".to_string()]);

        // Locked profile check: ProfileSessionState reports locked for passworded profiles.
        let session = ProfileSessionState::default();
        let locked_profile = profiles::Profile {
            id: "work".to_string(),
            name: "Work".to_string(),
            has_password: true,
            password_hash: Some("hash".to_string()),
            salt: Some("salt".to_string()),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            last_backup_at: None,
        };
        assert!(!session.is_unlocked(&locked_profile));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn saves_rehydrates_and_cleans_up_media_directory() {
        let dir = temp_profile_dir("media_cleanup");
        let attachment = MediaAttachment {
            path: PathBuf::from("sample.png"),
            mime_type: "image/png".to_string(),
            alt_text: Some("Sample".to_string()),
            bytes: Some(vec![1, 2, 3, 4]),
        };

        let refs = save_job_media(&dir, "job-1", &[attachment]).expect("saves media");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].alt_text.as_deref(), Some("Sample"));
        assert!(schedule_media_job_dir(&dir, "job-1").is_dir());

        // Verify schedule.json round-trips SavedMediaRef.alt_text.
        let json = serde_json::to_string(&refs).expect("serialises media refs");
        let parsed_refs: Vec<SavedMediaRef> =
            serde_json::from_str(&json).expect("deserialises media refs");
        assert_eq!(parsed_refs[0].alt_text.as_deref(), Some("Sample"));

        let rehydrated = rehydrate_media(&dir, &parsed_refs).expect("rehydrates media");
        assert_eq!(rehydrated[0].bytes.as_deref(), Some(&[1, 2, 3, 4][..]));
        assert_eq!(rehydrated[0].alt_text.as_deref(), Some("Sample"));

        cleanup_job_media(&dir, "job-1");
        assert!(!schedule_media_job_dir(&dir, "job-1").exists());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn recovers_interrupted_publishing_jobs_on_startup() {
        let mut jobs = vec![
            ScheduledPost {
                id: "interrupted".to_string(),
                profile_id: "default".to_string(),
                scheduled_for: "2026-01-01T00:00:00Z".to_string(),
                text: "Hello".to_string(),
                overrides: HashMap::new(),
                media_refs: Vec::new(),
                platform_ids: vec!["bluesky".to_string()],
                auto_thread: false,
                status: "publishing".to_string(),
                error: None,
                created_at: "2026-01-01T00:00:00Z".to_string(),
            },
            ScheduledPost {
                id: "pending".to_string(),
                profile_id: "default".to_string(),
                scheduled_for: "2026-01-01T01:00:00Z".to_string(),
                text: "World".to_string(),
                overrides: HashMap::new(),
                media_refs: Vec::new(),
                platform_ids: vec!["bluesky".to_string()],
                auto_thread: false,
                status: "pending".to_string(),
                error: None,
                created_at: "2026-01-01T00:00:00Z".to_string(),
            },
        ];

        assert!(recover_interrupted_jobs(&mut jobs));
        assert_eq!(jobs[0].status, "failed");
        assert_eq!(
            jobs[0].error.as_deref(),
            Some("Interrupted before completion")
        );
        assert_eq!(jobs[1].status, "pending");
        assert!(!recover_interrupted_jobs(&mut jobs));
    }
}
