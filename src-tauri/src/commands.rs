use crate::core::backup::{
    self, BackupMetadata, BackupPayload, BackupPluginRequirement, BackupRestorePluginItem,
    BackupRestorePreview, ImportedProfileResponse,
};
use crate::core::desktop::{self, DesktopSettings, DesktopSettingsState};
use crate::core::dispatcher::{self, PlatformDispatchRequest};
use crate::core::history::{self, HistoryEntry};
use crate::core::platform::{FieldKind, MediaAttachment, PlatformMetadata, PostOutcome};
use crate::core::profiles::{self, ProfileInfo, ProfileSessionState};
use crate::core::scheduler::{self, ScheduledPost, ScheduledPostsResponse};
use crate::core::{registry, vault};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, State};

fn new_history_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos}")
}

fn now_iso8601_utc() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let hour = rem / 3_600;
    let minute = (rem % 3_600) / 60;
    let second = rem % 60;

    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if m <= 2 { 1 } else { 0 };

    format!("{year:04}-{m:02}-{d:02}T{hour:02}:{minute:02}:{second:02}Z")
}

#[tauri::command]
pub async fn list_profiles(
    app: AppHandle,
    state: State<'_, ProfileSessionState>,
) -> Result<Vec<ProfileInfo>, String> {
    profiles::list_profiles(&app, &state)
}

#[tauri::command]
pub async fn create_profile(
    app: AppHandle,
    state: State<'_, ProfileSessionState>,
    name: String,
    password: Option<String>,
) -> Result<ProfileInfo, String> {
    profiles::create_profile(&app, &state, &name, password.as_deref())
}

#[tauri::command]
pub async fn rename_profile(
    app: AppHandle,
    state: State<'_, ProfileSessionState>,
    profile_id: String,
    name: String,
) -> Result<ProfileInfo, String> {
    profiles::rename_profile(&app, &state, &profile_id, &name)
}

#[tauri::command]
pub async fn unlock_profile(
    app: AppHandle,
    state: State<'_, ProfileSessionState>,
    profile_id: String,
    password: String,
) -> Result<bool, String> {
    profiles::unlock_profile(&app, &state, &profile_id, &password)
}

#[tauri::command]
pub async fn lock_profile(
    state: State<'_, ProfileSessionState>,
    profile_id: String,
) -> Result<(), String> {
    profiles::lock_profile(&state, &profile_id);
    Ok(())
}

#[tauri::command]
pub async fn set_profile_password(
    app: AppHandle,
    state: State<'_, ProfileSessionState>,
    profile_id: String,
    current_password: Option<String>,
    new_password: Option<String>,
) -> Result<ProfileInfo, String> {
    profiles::set_profile_password(
        &app,
        &state,
        &profile_id,
        current_password.as_deref(),
        new_password.as_deref(),
    )
}

#[tauri::command]
pub async fn delete_profile(
    app: AppHandle,
    state: State<'_, ProfileSessionState>,
    profile_id: String,
) -> Result<(), String> {
    profiles::delete_profile(&app, &state, &profile_id)
}

#[tauri::command]
pub async fn get_platforms(
    app: AppHandle,
    state: State<'_, ProfileSessionState>,
    profile_id: String,
) -> Result<Vec<PlatformMetadata>, String> {
    profiles::ensure_profile_unlocked(&app, &state, &profile_id)?;

    tokio::task::spawn_blocking(move || {
        registry::all_platforms()
            .into_iter()
            .map(|platform| {
                let auth_modes = platform.auth_modes();
                let active_mode =
                    vault::get_active_auth_mode(&profile_id, platform.id(), &auth_modes);
                let is_configured =
                    vault::is_platform_configured(&profile_id, platform.id(), &auth_modes);
                registry::to_metadata(&*platform, is_configured, active_mode)
            })
            .collect()
    })
    .await
    .map_err(|err| format!("Failed to collect platform metadata: {err}"))
}

#[tauri::command]
pub async fn save_credentials(
    app: AppHandle,
    state: State<'_, ProfileSessionState>,
    profile_id: String,
    platform_id: String,
    mode_id: String,
    values: HashMap<String, String>,
) -> Result<(), String> {
    profiles::ensure_profile_unlocked(&app, &state, &profile_id)?;

    tokio::task::spawn_blocking(move || {
        let platform = registry::find_platform(&platform_id)
            .ok_or_else(|| format!("Unknown platform '{platform_id}'."))?;

        let auth_modes = platform.auth_modes();
        let mode = auth_modes
            .iter()
            .find(|m| m.id == mode_id)
            .ok_or_else(|| format!("Unknown authentication mode '{mode_id}'."))?;

        vault::save_secret(
            &profile_id,
            platform.id(),
            vault::AUTH_MODE_KEY,
            mode.id.as_ref(),
        )?;

        for field in &mode.fields {
            if matches!(field.kind, FieldKind::OAuthButton { .. }) {
                continue;
            }
            if let Some(value) = values.get(field.key.as_ref()) {
                if value.trim().is_empty() {
                    vault::delete_secret(&profile_id, platform.id(), field.key.as_ref())?;
                } else {
                    vault::save_secret(&profile_id, platform.id(), field.key.as_ref(), value)?;
                }
            }
        }

        Ok(())
    })
    .await
    .map_err(|err| format!("Failed to save credentials: {err}"))?
}

#[tauri::command]
pub async fn verify_platform(
    app: AppHandle,
    state: State<'_, ProfileSessionState>,
    profile_id: String,
    platform_id: String,
) -> Result<(), String> {
    profiles::ensure_profile_unlocked(&app, &state, &profile_id)?;

    let platform = registry::find_platform(&platform_id)
        .ok_or_else(|| format!("Unknown platform '{platform_id}'."))?;

    let auth_modes = platform.auth_modes();
    let id = platform.id().to_string();

    let creds = tokio::task::spawn_blocking(move || {
        vault::load_platform_credentials(&profile_id, &id, &auth_modes)
    })
    .await
    .map_err(|err| format!("Failed to load credentials: {err}"))??;

    platform.verify_credentials(&creds).await
}

#[tauri::command]
pub async fn unlink_platform(
    app: AppHandle,
    state: State<'_, ProfileSessionState>,
    profile_id: String,
    platform_id: String,
) -> Result<(), String> {
    profiles::ensure_profile_unlocked(&app, &state, &profile_id)?;

    tokio::task::spawn_blocking(move || {
        // Decouple unlinking from active/loaded plugins so orphaned or legacy credentials
        // (such as "discord" from an older build) can always be wiped cleanly.
        let auth_modes = registry::find_platform(&platform_id)
            .map(|platform| platform.auth_modes())
            .unwrap_or_default();

        vault::wipe_profile_platform_secrets(&profile_id, &platform_id, &auth_modes)
    })
    .await
    .map_err(|err| format!("Failed to unlink platform: {err}"))?
}

#[tauri::command]
pub async fn start_platform_oauth(
    app: AppHandle,
    state: State<'_, ProfileSessionState>,
    profile_id: String,
    platform_id: String,
    mode_id: String,
    inputs: HashMap<String, String>,
) -> Result<(), String> {
    profiles::ensure_profile_unlocked(&app, &state, &profile_id)?;

    let platform = registry::find_platform(&platform_id)
        .ok_or_else(|| format!("Unknown platform '{platform_id}'."))?;

    if !platform.auth_modes().iter().any(|m| m.id == mode_id) {
        return Err(format!("Unknown authentication mode '{mode_id}'."));
    }

    let tokens = platform.start_oauth(&mode_id, &inputs).await?;
    let id = platform.id().to_string();

    tokio::task::spawn_blocking(move || {
        vault::save_secret(&profile_id, &id, vault::AUTH_MODE_KEY, &mode_id)?;
        for (key, value) in tokens {
            if value.trim().is_empty() {
                vault::delete_secret(&profile_id, &id, &key)?;
            } else {
                vault::save_secret(&profile_id, &id, &key, &value)?;
            }
        }
        Ok(())
    })
    .await
    .map_err(|err| format!("Failed to store OAuth credentials: {err}"))?
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn broadcast_post(
    app: AppHandle,
    state: State<'_, ProfileSessionState>,
    profile_id: String,
    targets: Vec<PlatformDispatchRequest>,
    master_text: String,
    overrides: HashMap<String, String>,
    media: Vec<MediaAttachment>,
    auto_thread: Option<bool>,
) -> Result<Vec<PostOutcome>, String> {
    profiles::ensure_profile_unlocked(&app, &state, &profile_id)?;

    let outcomes =
        dispatcher::dispatch_all(&profile_id, targets, auto_thread.unwrap_or(false)).await;

    let entry = HistoryEntry {
        id: new_history_id(),
        created_at: now_iso8601_utc(),
        master_text,
        overrides,
        media,
        outcomes: outcomes.clone(),
    };
    let _ = history::append_history(&app, &profile_id, entry);

    Ok(outcomes)
}

#[tauri::command]
pub async fn get_history(
    app: AppHandle,
    state: State<'_, ProfileSessionState>,
    profile_id: String,
) -> Result<Vec<HistoryEntry>, String> {
    profiles::ensure_profile_unlocked(&app, &state, &profile_id)?;
    history::load_history(&app, &profile_id)
}

#[tauri::command]
pub async fn clear_history(
    app: AppHandle,
    state: State<'_, ProfileSessionState>,
    profile_id: String,
) -> Result<(), String> {
    profiles::ensure_profile_unlocked(&app, &state, &profile_id)?;
    history::clear_history(&app, &profile_id)
}

#[tauri::command]
pub async fn recall_post(
    app: AppHandle,
    state: State<'_, ProfileSessionState>,
    profile_id: String,
    history_id: String,
    platform_ids: Option<Vec<String>>,
) -> Result<HistoryEntry, String> {
    profiles::ensure_profile_unlocked(&app, &state, &profile_id)?;

    let mut entries = history::load_history(&app, &profile_id)?;
    let entry_idx = entries
        .iter()
        .position(|e| e.id == history_id)
        .ok_or_else(|| "History entry not found.".to_string())?;

    for outcome in &mut entries[entry_idx].outcomes {
        if !outcome.success || outcome.recalled {
            continue;
        }
        if let Some(ref filter_ids) = platform_ids
            && !filter_ids.iter().any(|id| id == &outcome.platform_id)
        {
            continue;
        }

        let ids = history::outcome_post_ids(outcome);
        if ids.is_empty() {
            continue;
        }

        let Some(platform) = registry::find_platform(&outcome.platform_id) else {
            outcome.recall_error = Some("Platform plugin is not registered.".to_string());
            continue;
        };

        if !platform.capabilities().supports_delete {
            outcome.recall_error =
                Some("Post deletion is not supported by this platform.".to_string());
            continue;
        }

        let creds = match vault::load_platform_credentials(
            &profile_id,
            &outcome.platform_id,
            &platform.auth_modes(),
        ) {
            Ok(c) => c,
            Err(err) => {
                outcome.recall_error = Some(err);
                continue;
            }
        };

        let mut delete_err: Option<String> = None;
        for id in ids.iter().rev() {
            if let Err(err) = platform.delete_post(&creds, id).await {
                delete_err = Some(err);
                break;
            }
        }

        match delete_err {
            None => {
                outcome.recalled = true;
                outcome.recalled_at = Some(Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string());
                outcome.recall_error = None;
            }
            Some(err) => {
                outcome.recalled = false;
                outcome.recall_error = Some(err);
            }
        }
    }

    history::save_history(&app, &profile_id, &entries)?;
    Ok(entries[entry_idx].clone())
}

#[tauri::command]
pub async fn get_desktop_settings(
    state: State<'_, DesktopSettingsState>,
) -> Result<DesktopSettings, String> {
    state
        .0
        .lock()
        .map(|guard| guard.clone())
        .map_err(|_| "Desktop settings lock poisoned.".to_string())
}

#[tauri::command]
pub async fn update_desktop_settings(
    app: AppHandle,
    state: State<'_, DesktopSettingsState>,
    settings: DesktopSettings,
) -> Result<DesktopSettings, String> {
    let normalized = DesktopSettings {
        minimize_to_tray: settings.minimize_to_tray,
        global_shortcut: settings
            .global_shortcut
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
    };

    desktop::apply_global_shortcut(&app, normalized.global_shortcut.as_deref())?;
    desktop::save_settings(&app, &normalized)?;

    if let Ok(mut guard) = state.0.lock() {
        *guard = normalized.clone();
    }

    Ok(normalized)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn schedule_post(
    app: AppHandle,
    state: State<'_, ProfileSessionState>,
    profile_id: String,
    scheduled_for: String,
    text: String,
    overrides: HashMap<String, String>,
    media: Vec<MediaAttachment>,
    platform_ids: Vec<String>,
    auto_thread: Option<bool>,
) -> Result<ScheduledPost, String> {
    profiles::ensure_profile_unlocked(&app, &state, &profile_id)?;

    let parsed_time = DateTime::parse_from_rfc3339(&scheduled_for)
        .map_err(|_| "Invalid scheduled time format.".to_string())?
        .with_timezone(&Utc);
    if parsed_time <= Utc::now() {
        return Err("Scheduled time must be in the future.".to_string());
    }

    if platform_ids.is_empty() {
        return Err("Select at least one platform to schedule.".to_string());
    }

    let use_auto_thread = auto_thread.unwrap_or(false);
    for pid in &platform_ids {
        let platform =
            registry::find_platform(pid).ok_or_else(|| format!("Unknown platform '{pid}'."))?;
        let caps = platform.capabilities();
        let resolved_text = overrides.get(pid).map(String::as_str).unwrap_or(&text);

        if resolved_text.trim().is_empty() && media.is_empty() {
            return Err("Post cannot be empty.".to_string());
        }
        if caps.requires_media && media.is_empty() {
            return Err(format!("{} requires media.", platform.name()));
        }
        if resolved_text.chars().count() > caps.max_chars
            && !(use_auto_thread && caps.supports_threads)
        {
            return Err(format!(
                "{} exceeds the {} character limit.",
                platform.name(),
                caps.max_chars
            ));
        }
    }

    let normalized_time = parsed_time.format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let job = scheduler::add_scheduled_job(
        &app,
        &profile_id,
        normalized_time,
        text,
        overrides,
        &media,
        platform_ids,
        use_auto_thread,
    )?;
    let _ = app.emit("schedule-updated", ());
    Ok(job)
}

#[tauri::command]
pub async fn list_scheduled_posts(
    app: AppHandle,
    state: State<'_, ProfileSessionState>,
    profile_id: String,
) -> Result<ScheduledPostsResponse, String> {
    let profiles = profiles::ensure_initialized(&app)?;
    let profile = profiles
        .iter()
        .find(|p| p.id == profile_id)
        .ok_or_else(|| format!("Unknown profile '{profile_id}'."))?;

    if !state.is_unlocked(profile) {
        return Ok(ScheduledPostsResponse {
            is_locked: true,
            posts: Vec::new(),
        });
    }

    let posts = scheduler::list_jobs(&app, &profile_id)?;
    Ok(ScheduledPostsResponse {
        is_locked: false,
        posts,
    })
}

#[tauri::command]
pub async fn cancel_scheduled_post(
    app: AppHandle,
    state: State<'_, ProfileSessionState>,
    profile_id: String,
    job_id: String,
) -> Result<(), String> {
    profiles::ensure_profile_unlocked(&app, &state, &profile_id)?;
    scheduler::cancel_job(&app, &profile_id, &job_id)
}

#[tauri::command]
pub async fn publish_scheduled_now(
    app: AppHandle,
    state: State<'_, ProfileSessionState>,
    profile_id: String,
    job_id: String,
) -> Result<Vec<PostOutcome>, String> {
    profiles::ensure_profile_unlocked(&app, &state, &profile_id)?;
    scheduler::execute_scheduled_job(&app, &profile_id, &job_id).await
}

#[tauri::command]
pub async fn retry_scheduled_post(
    app: AppHandle,
    state: State<'_, ProfileSessionState>,
    profile_id: String,
    job_id: String,
) -> Result<ScheduledPost, String> {
    profiles::ensure_profile_unlocked(&app, &state, &profile_id)?;
    scheduler::retry_job(&app, &profile_id, &job_id)
}

#[tauri::command]
pub async fn export_profile_backup(
    app: AppHandle,
    state: State<'_, ProfileSessionState>,
    profile_id: String,
    file_path: String,
    passphrase: String,
    frontend_state: HashMap<String, String>,
) -> Result<(), String> {
    if passphrase.trim().len() < 8 {
        return Err("Passphrase must be at least 8 characters.".to_string());
    }

    profiles::ensure_profile_unlocked(&app, &state, &profile_id)?;
    let all_profiles = profiles::ensure_initialized(&app)?;
    let profile_name = all_profiles
        .into_iter()
        .find(|p| p.id == profile_id)
        .map(|p| p.name)
        .ok_or_else(|| format!("Unknown profile '{profile_id}'."))?;

    let mut credentials: HashMap<String, HashMap<String, String>> = HashMap::new();
    for platform in registry::all_platforms() {
        let mut creds =
            vault::load_platform_credentials(&profile_id, platform.id(), &platform.auth_modes())?;
        creds.remove(vault::PROFILE_ID_KEY);
        let has_real_creds = creds
            .iter()
            .any(|(k, v)| k != vault::AUTH_MODE_KEY && !v.trim().is_empty());
        if has_real_creds {
            credentials.insert(platform.id().to_string(), creds);
        }
    }

    let installed_records =
        crate::plugins::registry::load_installed_records(&app).unwrap_or_default();
    let required_plugins: Vec<BackupPluginRequirement> = installed_records
        .into_iter()
        .filter(|rec| credentials.contains_key(&rec.id))
        .map(|rec| BackupPluginRequirement {
            plugin_id: rec.id,
            version: rec.version,
            display_name: Some(rec.name),
            is_official: rec.is_official,
        })
        .collect();

    let history = history::load_history(&app, &profile_id)?;
    let payload = BackupPayload {
        profile_name,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        credentials,
        history,
        frontend_state,
        required_plugins,
    };

    let envelope = backup::encrypt_payload(&payload, &passphrase)?;
    let json = serde_json::to_string_pretty(&envelope)
        .map_err(|err| format!("Failed to serialise backup envelope: {err}"))?;
    std::fs::write(&file_path, json).map_err(|err| format!("Failed to write backup file: {err}"))
}

#[tauri::command]
pub async fn inspect_profile_backup(file_path: String) -> Result<BackupMetadata, String> {
    let raw = std::fs::read_to_string(&file_path)
        .map_err(|err| format!("Failed to read backup file: {err}"))?;
    backup::inspect_envelope(&raw)
}

#[tauri::command]
pub async fn preview_backup_restore(
    app: AppHandle,
    file_path: String,
    passphrase: String,
) -> Result<BackupRestorePreview, String> {
    let raw = std::fs::read_to_string(&file_path)
        .map_err(|err| format!("Failed to read backup file: {err}"))?;
    let envelope = backup::parse_and_validate_envelope(&raw)?;
    let payload = backup::decrypt_envelope(&envelope, &passphrase)?;

    let requirements: Vec<BackupPluginRequirement> = if !payload.required_plugins.is_empty() {
        payload.required_plugins
    } else {
        payload
            .credentials
            .iter()
            .filter(|(_, map)| {
                map.iter()
                    .any(|(k, v)| k != vault::AUTH_MODE_KEY && !v.trim().is_empty())
            })
            .map(|(platform_id, _)| BackupPluginRequirement {
                plugin_id: platform_id.clone(),
                version: String::new(),
                display_name: None,
                is_official: false,
            })
            .collect()
    };

    let installed_records =
        crate::plugins::registry::load_installed_records(&app).unwrap_or_default();
    let installed_map: HashMap<String, crate::plugins::registry::InstalledPluginRecord> =
        installed_records
            .into_iter()
            .map(|r| (r.id.clone(), r))
            .collect();

    let index_opt = crate::plugins::registry::fetch_registry_index(&app)
        .await
        .ok();
    let host_version = env!("CARGO_PKG_VERSION");
    let (backup_app_version, is_older_version, is_newer_version, version_warning) =
        backup::evaluate_backup_app_version(&payload.app_version, host_version);

    let mut already_installed = Vec::new();
    let mut missing_available = Vec::new();
    let mut missing_unavailable = Vec::new();

    for req in requirements {
        let required_version = (!req.version.trim().is_empty()).then(|| req.version.clone());

        if let Some(inst) = installed_map.get(&req.plugin_id) {
            let version_differs = required_version
                .as_deref()
                .is_some_and(|v| v != inst.version);
            already_installed.push(BackupRestorePluginItem {
                plugin_id: req.plugin_id,
                display_name: req.display_name.unwrap_or_else(|| inst.name.clone()),
                required_version,
                installed_version: Some(inst.version.clone()),
                target_version: None,
                icon_svg: inst.icon_svg.clone(),
                brand_colour: inst
                    .brand_colour
                    .clone()
                    .unwrap_or_else(|| "#64748B".to_string()),
                is_official: inst.is_official || req.is_official,
                verified: inst.verified,
                version_differs,
                reason: None,
            });
            continue;
        }

        let entry_opt = index_opt
            .as_ref()
            .and_then(|idx| idx.plugins.iter().find(|p| p.slug == req.plugin_id));

        let Some(entry) = entry_opt else {
            missing_unavailable.push(BackupRestorePluginItem {
                plugin_id: req.plugin_id.clone(),
                display_name: req.display_name.unwrap_or_else(|| req.plugin_id.clone()),
                required_version,
                installed_version: None,
                target_version: None,
                icon_svg: crate::plugins::registry::load_cached_manifest_icon(&app, &req.plugin_id),
                brand_colour: "#64748B".to_string(),
                is_official: req.is_official,
                verified: false,
                version_differs: false,
                reason: Some(
                    "Not found in the official plugin registry (may be a locally sideloaded .wasm plugin)."
                        .to_string(),
                ),
            });
            continue;
        };

        let is_official = crate::plugins::registry::is_official_plugin(
            entry.is_official || req.is_official,
            entry.author.as_ref(),
        );
        let fallback_icon = entry
            .icon_svg
            .clone()
            .or_else(|| crate::plugins::registry::load_cached_manifest_icon(&app, &req.plugin_id));
        let fallback_colour = entry
            .brand_colour
            .clone()
            .unwrap_or_else(|| "#64748B".to_string());

        match crate::plugins::registry::fetch_plugin_manifest(
            &app,
            &req.plugin_id,
            entry.manifest_url.as_deref(),
            Some(&entry.latest_version),
        )
        .await
        {
            Ok(manifest) => {
                let icon_svg = manifest.resolved_icon_svg().or(fallback_icon);
                let brand_colour = manifest.brand_colour.clone().unwrap_or(fallback_colour);
                let display_name = manifest
                    .display_name
                    .clone()
                    .or(req.display_name)
                    .unwrap_or_else(|| entry.display_name.clone());

                let mut chosen_version: Option<String> = None;
                let mut compat_error: Option<String> = None;

                if let Some(ref backed_up_ver) = required_version
                    && let Some(ver_entry) = manifest
                        .versions
                        .iter()
                        .find(|v| &v.version == backed_up_ver)
                {
                    let req_str = ver_entry
                        .compatible_app_version
                        .as_deref()
                        .or(manifest.compatible_app_version.as_deref())
                        .or(entry.compatible_app_version.as_deref())
                        .unwrap_or(">=1.0.0");
                    let (ok, err) = registry::evaluate_compatibility(host_version, req_str);
                    if ok {
                        chosen_version = Some(ver_entry.version.clone());
                    } else {
                        compat_error = err;
                    }
                }

                if chosen_version.is_none()
                    && let Some(latest_entry) = manifest
                        .versions
                        .iter()
                        .find(|v| v.version == entry.latest_version)
                        .or_else(|| manifest.versions.first())
                {
                    let req_str = latest_entry
                        .compatible_app_version
                        .as_deref()
                        .or(manifest.compatible_app_version.as_deref())
                        .or(entry.compatible_app_version.as_deref())
                        .unwrap_or(">=1.0.0");
                    let (ok, err) = registry::evaluate_compatibility(host_version, req_str);
                    if ok {
                        chosen_version = Some(latest_entry.version.clone());
                    } else {
                        compat_error = err;
                    }
                }

                if let Some(target_version) = chosen_version {
                    missing_available.push(BackupRestorePluginItem {
                        plugin_id: req.plugin_id,
                        display_name,
                        required_version,
                        installed_version: None,
                        target_version: Some(target_version),
                        icon_svg,
                        brand_colour,
                        is_official,
                        verified: true,
                        version_differs: false,
                        reason: None,
                    });
                } else {
                    missing_unavailable.push(BackupRestorePluginItem {
                        plugin_id: req.plugin_id,
                        display_name,
                        required_version,
                        installed_version: None,
                        target_version: None,
                        icon_svg,
                        brand_colour,
                        is_official,
                        verified: true,
                        version_differs: false,
                        reason: Some(compat_error.unwrap_or_else(|| {
                            format!("Incompatible with SocialPoster v{host_version}.")
                        })),
                    });
                }
            }
            Err(_) => {
                let req_str = entry.compatible_app_version.as_deref().unwrap_or(">=1.0.0");
                let (ok, err) = registry::evaluate_compatibility(host_version, req_str);
                if ok {
                    missing_available.push(BackupRestorePluginItem {
                        plugin_id: req.plugin_id,
                        display_name: req
                            .display_name
                            .unwrap_or_else(|| entry.display_name.clone()),
                        required_version,
                        installed_version: None,
                        target_version: Some(entry.latest_version.clone()),
                        icon_svg: fallback_icon,
                        brand_colour: fallback_colour,
                        is_official,
                        verified: true,
                        version_differs: false,
                        reason: None,
                    });
                } else {
                    missing_unavailable.push(BackupRestorePluginItem {
                        plugin_id: req.plugin_id,
                        display_name: req
                            .display_name
                            .unwrap_or_else(|| entry.display_name.clone()),
                        required_version,
                        installed_version: None,
                        target_version: None,
                        icon_svg: fallback_icon,
                        brand_colour: fallback_colour,
                        is_official,
                        verified: true,
                        version_differs: false,
                        reason: Some(err.unwrap_or_else(|| {
                            format!("Incompatible with SocialPoster v{host_version}.")
                        })),
                    });
                }
            }
        }
    }

    Ok(BackupRestorePreview {
        profile_name: payload.profile_name,
        backup_app_version,
        current_app_version: host_version.to_string(),
        is_older_version,
        is_newer_version,
        version_warning,
        already_installed,
        missing_available,
        missing_unavailable,
    })
}

fn resolve_non_colliding_profile_name(existing_names: &[String], requested: &str) -> String {
    let base = if requested.trim().is_empty() {
        "Restored"
    } else {
        requested.trim()
    };

    if !existing_names
        .iter()
        .any(|name| name.eq_ignore_ascii_case(base))
    {
        return base.to_string();
    }

    let first_candidate = format!("{base} (Restored)");
    if !existing_names
        .iter()
        .any(|name| name.eq_ignore_ascii_case(&first_candidate))
    {
        return first_candidate;
    }

    let mut suffix = 2usize;
    loop {
        let candidate = format!("{base} (Restored {suffix})");
        if !existing_names
            .iter()
            .any(|name| name.eq_ignore_ascii_case(&candidate))
        {
            return candidate;
        }
        suffix += 1;
    }
}

#[tauri::command]
pub async fn import_profile_backup(
    app: AppHandle,
    state: State<'_, ProfileSessionState>,
    file_path: String,
    passphrase: String,
    custom_name: Option<String>,
) -> Result<ImportedProfileResponse, String> {
    let raw = std::fs::read_to_string(&file_path)
        .map_err(|err| format!("Failed to read backup file: {err}"))?;
    let envelope = backup::parse_and_validate_envelope(&raw)?;
    let payload = backup::decrypt_envelope(&envelope, &passphrase)?;

    let existing_names: Vec<String> = profiles::ensure_initialized(&app)?
        .into_iter()
        .map(|p| p.name)
        .collect();
    let requested = custom_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(&payload.profile_name);
    let final_name = resolve_non_colliding_profile_name(&existing_names, requested);

    let new_profile = profiles::create_profile(&app, &state, &final_name, None)?;
    let restored_platforms_count = payload.credentials.len();
    let restored_history_count = payload.history.len();

    for (platform_id, creds_map) in &payload.credentials {
        for (key, value) in creds_map {
            if key == vault::PROFILE_ID_KEY {
                continue;
            }
            vault::save_secret(&new_profile.id, platform_id, key, value)?;
        }
    }

    if !payload.history.is_empty() {
        history::save_history(&app, &new_profile.id, &payload.history)?;
    }

    Ok(ImportedProfileResponse {
        profile: new_profile,
        frontend_state: payload.frontend_state,
        restored_platforms_count,
        restored_history_count,
    })
}
