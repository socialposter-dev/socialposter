use crate::core::platform::AUTH_KEYS_RESERVED;
use crate::core::{registry, vault};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

const PASSWORD_ROUNDS: usize = 100_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub has_password: bool,
    pub password_hash: Option<String>,
    pub salt: Option<String>,
    pub created_at: String,
    #[serde(default)]
    pub last_backup_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileInfo {
    pub id: String,
    pub name: String,
    pub has_password: bool,
    pub is_unlocked: bool,
    pub created_at: String,
    #[serde(default)]
    pub last_backup_at: Option<String>,
}

#[derive(Default)]
pub struct ProfileSessionState {
    pub unlocked: Mutex<HashSet<String>>,
}

impl ProfileSessionState {
    pub fn is_unlocked(&self, profile: &Profile) -> bool {
        if !profile.has_password {
            return true;
        }
        self.unlocked
            .lock()
            .map(|guard| guard.contains(&profile.id))
            .unwrap_or(false)
    }

    pub fn mark_unlocked(&self, profile_id: &str) {
        if let Ok(mut guard) = self.unlocked.lock() {
            guard.insert(profile_id.to_string());
        }
    }

    pub fn mark_locked(&self, profile_id: &str) {
        if let Ok(mut guard) = self.unlocked.lock() {
            guard.remove(profile_id);
        }
    }
}

pub fn app_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|err| format!("Failed to resolve app data directory: {err}"))
}

fn profiles_file_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app_data_dir(app)?.join("profiles.json"))
}

pub fn profile_dir(app: &AppHandle, profile_id: &str) -> Result<PathBuf, String> {
    Ok(app_data_dir(app)?.join("profiles").join(profile_id))
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

fn stretch_password_hash(password: &str, salt: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(salt);
    hasher.update(password.as_bytes());
    let mut digest: [u8; 32] = hasher.finalize().into();

    for _ in 1..PASSWORD_ROUNDS {
        let mut round = Sha256::new();
        round.update(digest);
        round.update(salt);
        digest = round.finalize().into();
    }

    digest
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

pub fn hash_new_password(password: &str) -> Result<(String, String), String> {
    let mut salt = [0u8; 16];
    getrandom::getrandom(&mut salt)
        .map_err(|err| format!("Failed to generate password salt: {err}"))?;
    let digest = stretch_password_hash(password, &salt);
    Ok((URL_SAFE_NO_PAD.encode(salt), URL_SAFE_NO_PAD.encode(digest)))
}

pub fn verify_password(password: &str, salt_b64: &str, expected_hash_b64: &str) -> bool {
    let Ok(salt) = URL_SAFE_NO_PAD.decode(salt_b64) else {
        return false;
    };
    let Ok(expected) = URL_SAFE_NO_PAD.decode(expected_hash_b64) else {
        return false;
    };
    let computed = stretch_password_hash(password, &salt);
    constant_time_eq(&computed, &expected)
}

fn save_profiles(app: &AppHandle, profiles: &[Profile]) -> Result<(), String> {
    let path = profiles_file_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("Failed to create app data directory: {err}"))?;
    }
    let json = serde_json::to_string_pretty(profiles)
        .map_err(|err| format!("Failed to serialise profiles: {err}"))?;
    fs::write(&path, json).map_err(|err| format!("Failed to write profiles.json: {err}"))
}

fn migrate_v0_1_0_to_default(app: &AppHandle) -> Result<(), String> {
    // 1. Migrate any un-namespaced keyring secrets to `default:{platform_id}:{key}`.
    for platform in registry::all_platforms() {
        let mut keys: HashSet<String> = HashSet::new();
        keys.insert(vault::AUTH_MODE_KEY.to_string());
        for reserved in AUTH_KEYS_RESERVED {
            keys.insert((*reserved).to_string());
        }
        for mode in platform.auth_modes() {
            for field in mode.fields {
                keys.insert(field.key.to_string());
            }
        }

        for key in keys {
            if let Ok(Some(secret)) = vault::get_legacy_secret(platform.id(), &key) {
                if !secret.trim().is_empty() {
                    let _ = vault::save_secret("default", platform.id(), &key, &secret);
                }
                let _ = vault::delete_legacy_secret(platform.id(), &key);
            }
        }
    }

    // 2. Migrate any legacy `app_data_dir()/history.json` into `profiles/default/history.json`.
    let data_dir = app_data_dir(app)?;
    let legacy_history = data_dir.join("history.json");
    if legacy_history.is_file() {
        let target_dir = profile_dir(app, "default")?;
        fs::create_dir_all(&target_dir)
            .map_err(|err| format!("Failed to create default profile directory: {err}"))?;
        let target_history = target_dir.join("history.json");
        if !target_history.exists() {
            fs::rename(&legacy_history, &target_history)
                .map_err(|err| format!("Failed to migrate legacy history.json: {err}"))?;
        }
    }

    Ok(())
}

/// Ensures `profiles.json` exists, running the one-time v0.1.0 migration if needed.
pub fn ensure_initialized(app: &AppHandle) -> Result<Vec<Profile>, String> {
    let path = profiles_file_path(app)?;
    if path.is_file() {
        let raw = fs::read_to_string(&path)
            .map_err(|err| format!("Failed to read profiles.json: {err}"))?;
        let profiles: Vec<Profile> = serde_json::from_str(&raw)
            .map_err(|err| format!("Failed to parse profiles.json: {err}"))?;
        if !profiles.is_empty() {
            return Ok(profiles);
        }
    }

    let default_profile = Profile {
        id: "default".to_string(),
        name: "Default".to_string(),
        has_password: false,
        password_hash: None,
        salt: None,
        created_at: now_iso8601_utc(),
        last_backup_at: None,
    };

    let profiles = vec![default_profile];
    save_profiles(app, &profiles)?;
    migrate_v0_1_0_to_default(app)?;
    Ok(profiles)
}

pub fn to_info(profile: &Profile, state: &ProfileSessionState) -> ProfileInfo {
    ProfileInfo {
        id: profile.id.clone(),
        name: profile.name.clone(),
        has_password: profile.has_password,
        is_unlocked: state.is_unlocked(profile),
        created_at: profile.created_at.clone(),
        last_backup_at: profile.last_backup_at.clone(),
    }
}

pub fn ensure_profile_unlocked(
    app: &AppHandle,
    state: &ProfileSessionState,
    profile_id: &str,
) -> Result<(), String> {
    let profiles = ensure_initialized(app)?;
    let profile = profiles
        .iter()
        .find(|p| p.id == profile_id)
        .ok_or_else(|| format!("Unknown profile '{profile_id}'."))?;

    if state.is_unlocked(profile) {
        Ok(())
    } else {
        Err(format!("Profile '{}' is locked.", profile.name))
    }
}

pub fn profile_exists(app: &AppHandle, profile_id: &str) -> bool {
    ensure_initialized(app)
        .map(|list| list.iter().any(|p| p.id == profile_id))
        .unwrap_or(false)
}

pub fn list_profiles(
    app: &AppHandle,
    state: &ProfileSessionState,
) -> Result<Vec<ProfileInfo>, String> {
    let profiles = ensure_initialized(app)?;
    Ok(profiles.iter().map(|p| to_info(p, state)).collect())
}

fn generate_profile_id(name: &str, existing: &[Profile]) -> String {
    let base: String = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string();

    let prefix = if base.is_empty() {
        "profile".to_string()
    } else {
        base
    };

    if !existing.iter().any(|p| p.id == prefix) {
        return prefix;
    }

    let mut suffix = 2usize;
    loop {
        let candidate = format!("{prefix}-{suffix}");
        if !existing.iter().any(|p| p.id == candidate) {
            return candidate;
        }
        suffix += 1;
    }
}

pub fn create_profile(
    app: &AppHandle,
    state: &ProfileSessionState,
    name: &str,
    password: Option<&str>,
) -> Result<ProfileInfo, String> {
    let trimmed_name = name.trim();
    if trimmed_name.is_empty() {
        return Err("Profile name is required.".to_string());
    }

    let mut profiles = ensure_initialized(app)?;
    let id = generate_profile_id(trimmed_name, &profiles);

    let (has_password, salt, password_hash) = match password.filter(|p| !p.is_empty()) {
        Some(pw) => {
            let (s, h) = hash_new_password(pw)?;
            (true, Some(s), Some(h))
        }
        None => (false, None, None),
    };

    let profile = Profile {
        id: id.clone(),
        name: trimmed_name.to_string(),
        has_password,
        password_hash,
        salt,
        created_at: now_iso8601_utc(),
        last_backup_at: None,
    };

    profiles.push(profile.clone());
    save_profiles(app, &profiles)?;
    state.mark_unlocked(&id);
    Ok(to_info(&profile, state))
}

pub fn rename_profile(
    app: &AppHandle,
    state: &ProfileSessionState,
    profile_id: &str,
    name: &str,
) -> Result<ProfileInfo, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Profile name is required.".to_string());
    }

    let mut profiles = ensure_initialized(app)?;
    let profile = profiles
        .iter_mut()
        .find(|p| p.id == profile_id)
        .ok_or_else(|| format!("Unknown profile '{profile_id}'."))?;

    profile.name = trimmed.to_string();
    let updated = profile.clone();
    save_profiles(app, &profiles)?;
    Ok(to_info(&updated, state))
}

pub fn unlock_profile(
    app: &AppHandle,
    state: &ProfileSessionState,
    profile_id: &str,
    password: &str,
) -> Result<bool, String> {
    let profiles = ensure_initialized(app)?;
    let profile = profiles
        .iter()
        .find(|p| p.id == profile_id)
        .ok_or_else(|| format!("Unknown profile '{profile_id}'."))?;

    if !profile.has_password {
        state.mark_unlocked(profile_id);
        return Ok(true);
    }

    let (Some(salt), Some(hash)) = (profile.salt.as_deref(), profile.password_hash.as_deref())
    else {
        return Err("Profile password metadata is corrupted.".to_string());
    };

    if verify_password(password, salt, hash) {
        state.mark_unlocked(profile_id);
        Ok(true)
    } else {
        Ok(false)
    }
}

pub fn lock_profile(state: &ProfileSessionState, profile_id: &str) {
    state.mark_locked(profile_id);
}

pub fn set_profile_password(
    app: &AppHandle,
    state: &ProfileSessionState,
    profile_id: &str,
    current_password: Option<&str>,
    new_password: Option<&str>,
) -> Result<ProfileInfo, String> {
    let mut profiles = ensure_initialized(app)?;
    let profile = profiles
        .iter_mut()
        .find(|p| p.id == profile_id)
        .ok_or_else(|| format!("Unknown profile '{profile_id}'."))?;

    if profile.has_password {
        let provided = current_password.unwrap_or("");
        let (Some(salt), Some(hash)) = (profile.salt.as_deref(), profile.password_hash.as_deref())
        else {
            return Err("Profile password metadata is corrupted.".to_string());
        };
        if !verify_password(provided, salt, hash) {
            return Err("Current password is incorrect.".to_string());
        }
    }

    match new_password.filter(|p| !p.is_empty()) {
        Some(pw) => {
            let (salt, hash) = hash_new_password(pw)?;
            profile.has_password = true;
            profile.salt = Some(salt);
            profile.password_hash = Some(hash);
            state.mark_unlocked(profile_id);
        }
        None => {
            profile.has_password = false;
            profile.salt = None;
            profile.password_hash = None;
            state.mark_unlocked(profile_id);
        }
    }

    let updated = profile.clone();
    save_profiles(app, &profiles)?;
    Ok(to_info(&updated, state))
}

pub fn delete_profile(
    app: &AppHandle,
    state: &ProfileSessionState,
    profile_id: &str,
) -> Result<(), String> {
    let mut profiles = ensure_initialized(app)?;
    let Some(idx) = profiles.iter().position(|p| p.id == profile_id) else {
        return Err(format!("Unknown profile '{profile_id}'."));
    };

    let is_corrupted = profiles[idx].has_password
        && (profiles[idx].password_hash.is_none() || profiles[idx].salt.is_none());

    if profiles.len() <= 1 && !is_corrupted {
        return Err("Cannot delete the final remaining profile.".to_string());
    }

    for platform in registry::all_platforms() {
        vault::wipe_profile_platform_secrets(profile_id, platform.id(), &platform.auth_modes())?;
    }

    let dir = profile_dir(app, profile_id)?;
    if dir.exists() {
        fs::remove_dir_all(&dir)
            .map_err(|err| format!("Failed to remove profile data directory: {err}"))?;
    }

    state.mark_locked(profile_id);
    profiles.remove(idx);

    if profiles.is_empty() {
        profiles.push(Profile {
            id: "default".to_string(),
            name: "Default".to_string(),
            has_password: false,
            password_hash: None,
            salt: None,
            created_at: now_iso8601_utc(),
            last_backup_at: None,
        });
    }

    save_profiles(app, &profiles)?;
    crate::core::scheduler::refresh_tray_tooltip(app);
    Ok(())
}

pub fn record_last_backup(app: &AppHandle, profile_id: &str) -> Result<String, String> {
    let mut profiles = ensure_initialized(app)?;
    let Some(profile) = profiles.iter_mut().find(|p| p.id == profile_id) else {
        return Err(format!("Unknown profile '{profile_id}'."));
    };
    let ts = now_iso8601_utc();
    profile.last_backup_at = Some(ts.clone());
    save_profiles(app, &profiles)?;
    Ok(ts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_and_verifies_passwords() {
        let (salt, hash) = hash_new_password("correct-horse-battery").expect("hash succeeds");
        assert!(verify_password("correct-horse-battery", &salt, &hash));
        assert!(!verify_password("wrong-password", &salt, &hash));
        assert!(!verify_password("", &salt, &hash));
    }

    #[test]
    fn persists_password_hash_and_salt_in_profile_json() {
        let (salt, hash) = hash_new_password("secret-pass").expect("hash succeeds");
        let profile = Profile {
            id: "work".to_string(),
            name: "Work".to_string(),
            has_password: true,
            password_hash: Some(hash.clone()),
            salt: Some(salt.clone()),
            created_at: "2026-11-16T00:00:00Z".to_string(),
            last_backup_at: None,
        };

        let json = serde_json::to_string(&profile).expect("serialises profile");
        let restored: Profile = serde_json::from_str(&json).expect("deserialises profile");
        assert_eq!(restored.password_hash.as_deref(), Some(hash.as_str()));
        assert_eq!(restored.salt.as_deref(), Some(salt.as_str()));
    }
}
