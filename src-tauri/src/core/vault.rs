use crate::core::platform::{AuthMode, FieldKind};
use keyring::{Entry, Error as KeyringError};
use std::collections::{HashMap, HashSet};

const KEYRING_SERVICE: &str = "universal-poster";
pub const AUTH_MODE_KEY: &str = "__auth_mode";
pub const PROFILE_ID_KEY: &str = "__profile_id";
pub const OAUTH_RESERVED_KEYS: [&str; 3] = ["access_token", "refresh_token", "expires_at"];
/// Legacy and common credential keys wiped unconditionally during `wipe_profile_platform_secrets`
/// so orphaned credentials (e.g. from uninstalled or legacy native platforms) are always cleared.
const LEGACY_AND_COMMON_CREDENTIAL_KEYS: &[&str] = &[
    "webhook_url",
    "username",
    "avatar_url",
    "bot_token",
    "channel_id",
    "token",
    "api_key",
    "api_secret",
    "access_secret",
    "client_id",
    "client_secret",
    "instance_url",
    "handle",
    "app_password",
    "chat_id",
    "blog_identifier",
    "identifier",
];

pub fn profile_keyring_user(profile_id: &str, platform_id: &str, key: &str) -> String {
    format!("{profile_id}:{platform_id}:{key}")
}

pub fn legacy_keyring_user(platform_id: &str, key: &str) -> String {
    format!("{platform_id}:{key}")
}

/// Shared key-builder returning every `(service, user)` keyring target candidate for
/// `(profile_id, platform_id)` across `field_keys` plus base platform entries.
pub fn candidate_keyring_entries<'a, I>(
    profile_id: &str,
    platform_id: &str,
    field_keys: I,
) -> Vec<(String, String)>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut entries = Vec::new();
    let mut seen = HashSet::new();

    for key in field_keys {
        for user in [
            profile_keyring_user(profile_id, platform_id, key),
            legacy_keyring_user(platform_id, key),
        ] {
            if seen.insert(user.clone()) {
                entries.push((KEYRING_SERVICE.to_string(), user));
            }
        }
    }

    for base_user in [
        format!("{profile_id}:{platform_id}"),
        platform_id.to_string(),
    ] {
        if seen.insert(base_user.clone()) {
            entries.push((KEYRING_SERVICE.to_string(), base_user));
        }
    }

    entries
}

fn entry_for(profile_id: &str, platform_id: &str, key: &str) -> Result<Entry, String> {
    let user = profile_keyring_user(profile_id, platform_id, key);
    Entry::new(KEYRING_SERVICE, &user)
        .map_err(|err| format!("Failed to initialise keyring entry for '{user}': {err}"))
}

fn legacy_entry_for(platform_id: &str, key: &str) -> Result<Entry, String> {
    let user = legacy_keyring_user(platform_id, key);
    Entry::new(KEYRING_SERVICE, &user)
        .map_err(|err| format!("Failed to initialise legacy keyring entry for '{user}': {err}"))
}

/// Persists a secret value in the OS keyring under `{profile_id}:{platform_id}:{key}`.
pub fn save_secret(
    profile_id: &str,
    platform_id: &str,
    key: &str,
    value: &str,
) -> Result<(), String> {
    let entry = entry_for(profile_id, platform_id, key)?;
    entry.set_password(value).map_err(|err| {
        format!("Failed to save credential '{profile_id}:{platform_id}:{key}': {err}")
    })
}

/// Reads a secret value from the OS keyring, returning `Ok(None)` if not present.
pub fn get_secret(
    profile_id: &str,
    platform_id: &str,
    key: &str,
) -> Result<Option<String>, String> {
    let entry = entry_for(profile_id, platform_id, key)?;
    match entry.get_password() {
        Ok(secret) => Ok(Some(secret)),
        Err(KeyringError::NoEntry) => Ok(None),
        Err(err) => Err(format!(
            "Failed to read credential '{profile_id}:{platform_id}:{key}': {err}"
        )),
    }
}

/// Deletes a secret value from the OS keyring. Absent entries succeed idempotently.
pub fn delete_secret(profile_id: &str, platform_id: &str, key: &str) -> Result<(), String> {
    let entry = entry_for(profile_id, platform_id, key)?;
    match entry.delete_credential() {
        Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
        Err(err) => Err(format!(
            "Failed to delete credential '{profile_id}:{platform_id}:{key}': {err}"
        )),
    }
}

/// Reads an un-namespaced v0.1.0 secret (`{platform_id}:{key}`) for migration.
pub fn get_legacy_secret(platform_id: &str, key: &str) -> Result<Option<String>, String> {
    let entry = legacy_entry_for(platform_id, key)?;
    match entry.get_password() {
        Ok(secret) => Ok(Some(secret)),
        Err(KeyringError::NoEntry) => Ok(None),
        Err(err) => Err(format!(
            "Failed to read legacy credential '{platform_id}:{key}': {err}"
        )),
    }
}

/// Deletes an un-namespaced v0.1.0 secret (`{platform_id}:{key}`) after migration.
pub fn delete_legacy_secret(platform_id: &str, key: &str) -> Result<(), String> {
    let entry = legacy_entry_for(platform_id, key)?;
    match entry.delete_credential() {
        Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
        Err(err) => Err(format!(
            "Failed to delete legacy credential '{platform_id}:{key}': {err}"
        )),
    }
}

fn has_non_empty_secret(profile_id: &str, platform_id: &str, key: &str) -> bool {
    matches!(
        get_secret(profile_id, platform_id, key),
        Ok(Some(val)) if !val.trim().is_empty()
    )
}

/// Returns the active authentication mode ID stored in the keyring for a profile,
/// falling back to the first mode declared by the platform.
pub fn get_active_auth_mode(
    profile_id: &str,
    platform_id: &str,
    auth_modes: &[AuthMode],
) -> Option<String> {
    if let Ok(Some(stored)) = get_secret(profile_id, platform_id, AUTH_MODE_KEY)
        && auth_modes.iter().any(|mode| mode.id == stored)
    {
        return Some(stored);
    }
    auth_modes.first().map(|mode| mode.id.to_string())
}

fn field_has_non_empty_secret(
    profile_id: &str,
    platform_id: &str,
    field: &crate::core::platform::AuthField,
) -> bool {
    match &field.kind {
        FieldKind::OAuthButton { target_key, .. } => {
            has_non_empty_secret(profile_id, platform_id, target_key.as_ref())
                || has_non_empty_secret(profile_id, platform_id, field.key.as_ref())
        }
        _ => has_non_empty_secret(profile_id, platform_id, field.key.as_ref()),
    }
}

/// Returns `true` if the active `AuthMode` has saved credentials in the keyring for the profile.
/// Modes with `required` fields require all of them; modes without `required` flags require at
/// least one non-empty field so empty modes never report configured vacuously.
pub fn is_platform_configured(
    profile_id: &str,
    platform_id: &str,
    auth_modes: &[AuthMode],
) -> bool {
    let Some(active_id) = get_active_auth_mode(profile_id, platform_id, auth_modes) else {
        return false;
    };
    let Some(active_mode) = auth_modes.iter().find(|mode| mode.id == active_id) else {
        return false;
    };

    let required_fields: Vec<_> = active_mode
        .fields
        .iter()
        .filter(|field| field.required)
        .collect();

    if required_fields.is_empty() {
        active_mode
            .fields
            .iter()
            .any(|field| field_has_non_empty_secret(profile_id, platform_id, field))
    } else {
        required_fields
            .into_iter()
            .all(|field| field_has_non_empty_secret(profile_id, platform_id, field))
    }
}

/// Loads all non-empty stored credentials (including `__auth_mode`, `__profile_id`,
/// and OAuth tokens) across all `AuthMode`s declared by the platform.
pub fn load_platform_credentials(
    profile_id: &str,
    platform_id: &str,
    auth_modes: &[AuthMode],
) -> Result<HashMap<String, String>, String> {
    let mut keys: HashSet<&str> = HashSet::new();
    keys.insert(AUTH_MODE_KEY);
    for key in OAUTH_RESERVED_KEYS {
        keys.insert(key);
    }
    for mode in auth_modes {
        for field in &mode.fields {
            keys.insert(field.key.as_ref());
        }
    }

    let mut creds = HashMap::new();
    creds.insert(PROFILE_ID_KEY.to_string(), profile_id.to_string());
    for key in keys {
        if let Some(value) = get_secret(profile_id, platform_id, key)?
            && !value.trim().is_empty()
        {
            creds.insert(key.to_string(), value);
        }
    }
    Ok(creds)
}

/// Collects all keyring field keys to wipe for a platform (declared fields, OAuth targets,
/// reserved tokens, `__auth_mode`, and legacy/common keys).
fn collect_wipe_keys(auth_modes: &[AuthMode]) -> HashSet<String> {
    let mut keys: HashSet<String> = HashSet::new();
    keys.insert(AUTH_MODE_KEY.to_string());
    for reserved in OAUTH_RESERVED_KEYS {
        keys.insert(reserved.to_string());
    }
    for legacy_key in LEGACY_AND_COMMON_CREDENTIAL_KEYS {
        keys.insert((*legacy_key).to_string());
    }
    for mode in auth_modes {
        for field in &mode.fields {
            keys.insert(field.key.to_string());
            if let FieldKind::OAuthButton { target_key, .. } = &field.kind {
                keys.insert(target_key.to_string());
            }
        }
    }
    keys
}

fn delete_keyring_entry(service: &str, user: &str) -> Result<(), String> {
    let entry = Entry::new(service, user).map_err(|err| {
        format!("Failed to initialise keyring entry for '{service}/{user}': {err}")
    })?;
    match entry.delete_credential() {
        Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
        Err(err) => Err(format!(
            "Failed to delete credential '{service}/{user}': {err}"
        )),
    }
}

#[cfg(any(target_os = "windows", test))]
fn parse_matching_cmdkey_targets(
    list_output: &str,
    profile_id: &str,
    platform_id: &str,
) -> Vec<String> {
    let profile_prefix = format!("{profile_id}:{platform_id}").to_lowercase();
    let legacy_colon_prefix = format!("{platform_id}:").to_lowercase();
    let legacy_dot_prefix = format!("{platform_id}.").to_lowercase();
    let service_lower = KEYRING_SERVICE.to_lowercase();

    let mut targets = Vec::new();
    for line in list_output.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("Target:") else {
            continue;
        };
        let raw_target = rest.trim();
        let target = raw_target
            .strip_prefix("LegacyGeneric:target=")
            .or_else(|| raw_target.strip_prefix("Generic:target="))
            .or_else(|| raw_target.strip_prefix("Domain:target="))
            .unwrap_or(raw_target)
            .trim();

        if target.is_empty() {
            continue;
        }

        let lower = target.to_lowercase();
        let has_service = lower.contains(&service_lower) || lower.contains("socialposter");
        let matches_profile_or_legacy = lower.contains(&profile_prefix)
            || lower.starts_with(&legacy_colon_prefix)
            || lower.starts_with(&legacy_dot_prefix);

        if has_service && matches_profile_or_legacy {
            targets.push(target.to_string());
        }
    }
    targets
}

#[cfg(target_os = "windows")]
fn sweep_windows_credentials(profile_id: &str, platform_id: &str) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let Ok(output) = std::process::Command::new("cmdkey")
        .arg("/list")
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    else {
        return;
    };

    if !output.status.success() {
        return;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for target in parse_matching_cmdkey_targets(&stdout, profile_id, platform_id) {
        let _ = std::process::Command::new("cmdkey")
            .arg(format!("/delete:{target}"))
            .creation_flags(CREATE_NO_WINDOW)
            .output();
    }
}

/// Sweeps every SocialPoster keyring entry on Windows during a factory reset.
#[cfg(target_os = "windows")]
pub fn sweep_all_app_windows_credentials() {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let Ok(output) = std::process::Command::new("cmdkey")
        .arg("/list")
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    else {
        return;
    };

    if !output.status.success() {
        return;
    }

    let service_lower = KEYRING_SERVICE.to_lowercase();
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("Target:") else {
            continue;
        };
        let raw_target = rest.trim();
        let target = raw_target
            .strip_prefix("LegacyGeneric:target=")
            .or_else(|| raw_target.strip_prefix("Generic:target="))
            .or_else(|| raw_target.strip_prefix("Domain:target="))
            .unwrap_or(raw_target)
            .trim();
        let lower = target.to_lowercase();
        if !target.is_empty() && (lower.contains(&service_lower) || lower.contains("socialposter"))
        {
            let _ = std::process::Command::new("cmdkey")
                .arg(format!("/delete:{target}"))
                .creation_flags(CREATE_NO_WINDOW)
                .output();
        }
    }
}

fn remaining_configured_identifiers(
    profile_id: &str,
    platform_id: &str,
    keys: &HashSet<String>,
) -> Vec<String> {
    let mut remaining = Vec::new();
    for key in keys {
        if has_non_empty_secret(profile_id, platform_id, key) {
            remaining.push(format!(
                "{KEYRING_SERVICE}/{}",
                profile_keyring_user(profile_id, platform_id, key)
            ));
        }
    }
    remaining
}

/// Deletes all keyring entries for a given profile + platform (including legacy/orphaned keys
/// and any base `{profile_id}:{platform_id}` entry). Missing keyring entries (`NoEntry`) are
/// treated as a successful no-op.
pub fn wipe_profile_platform_secrets(
    profile_id: &str,
    platform_id: &str,
    auth_modes: &[AuthMode],
) -> Result<(), String> {
    let keys = collect_wipe_keys(auth_modes);
    let key_refs: Vec<&str> = keys.iter().map(String::as_str).collect();

    for (service, user) in candidate_keyring_entries(profile_id, platform_id, key_refs) {
        delete_keyring_entry(&service, &user)?;
    }

    #[cfg(target_os = "windows")]
    sweep_windows_credentials(profile_id, platform_id);

    // Verify with the same status check used by `get_platforms` that zero configured fields remain.
    let remaining = remaining_configured_identifiers(profile_id, platform_id, &keys);
    if !remaining.is_empty()
        || (!auth_modes.is_empty() && is_platform_configured(profile_id, platform_id, auth_modes))
    {
        let detail = if remaining.is_empty() {
            format!("{KEYRING_SERVICE}/{profile_id}:{platform_id}")
        } else {
            remaining.join(", ")
        };
        return Err(format!(
            "Failed to completely unlink '{platform_id}': configured credentials still remain in the keyring ({detail})."
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::platform::AuthField;
    use std::borrow::Cow;

    #[test]
    fn collect_wipe_keys_includes_legacy_oauth_and_declared_fields() {
        let modes = vec![AuthMode {
            id: Cow::Borrowed("bot"),
            label: Cow::Borrowed("Bot"),
            description: None,
            setup_instructions: None,
            fields: vec![
                AuthField {
                    key: Cow::Borrowed("custom_secret"),
                    label: Cow::Borrowed("Custom"),
                    placeholder: Cow::Borrowed(""),
                    kind: FieldKind::Secret,
                    required: true,
                    help_text: None,
                },
                AuthField {
                    key: Cow::Borrowed("oauth_trigger"),
                    label: Cow::Borrowed("OAuth"),
                    placeholder: Cow::Borrowed(""),
                    kind: FieldKind::OAuthButton {
                        button_label: Cow::Borrowed("Connect"),
                        target_key: Cow::Borrowed("oauth_token_target"),
                    },
                    required: true,
                    help_text: None,
                },
            ],
        }];

        let keys = collect_wipe_keys(&modes);
        for expected in [
            AUTH_MODE_KEY,
            "webhook_url",
            "username",
            "avatar_url",
            "bot_token",
            "channel_id",
            "access_token",
            "refresh_token",
            "expires_at",
            "custom_secret",
            "oauth_trigger",
            "oauth_token_target",
        ] {
            assert!(keys.contains(expected), "missing expected key '{expected}'");
        }
    }

    #[test]
    fn candidate_keyring_entries_includes_profile_legacy_and_base_targets() {
        let entries = candidate_keyring_entries("default", "discord", ["webhook_url"]);
        assert!(entries.contains(&(
            KEYRING_SERVICE.to_string(),
            "default:discord:webhook_url".to_string()
        )));
        assert!(entries.contains(&(
            KEYRING_SERVICE.to_string(),
            "discord:webhook_url".to_string()
        )));
        assert!(entries.contains(&(KEYRING_SERVICE.to_string(), "default:discord".to_string())));
        assert!(entries.contains(&(KEYRING_SERVICE.to_string(), "discord".to_string())));
    }

    #[test]
    fn parses_windows_cmdkey_targets_for_active_profile_and_legacy_only() {
        let sample = "\
    Target: LegacyGeneric:target=default:discord:webhook_url.universal-poster\n\
    Target: LegacyGeneric:target=other_profile:discord:webhook_url.universal-poster\n\
    Target: Generic:target=discord:bot_token.universal-poster\n\
    Target: LegacyGeneric:target=default:bluesky:handle.universal-poster\n";

        let matched = parse_matching_cmdkey_targets(sample, "default", "discord");
        assert_eq!(
            matched,
            vec![
                "default:discord:webhook_url.universal-poster".to_string(),
                "discord:bot_token.universal-poster".to_string(),
            ]
        );
    }
}
