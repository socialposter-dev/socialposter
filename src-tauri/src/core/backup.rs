use crate::core::history::HistoryEntry;
use crate::core::profiles::ProfileInfo;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use chrono::Utc;
use rand::RngCore;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

pub const BACKUP_FORMAT: &str = "socialposter-backup";
pub const BACKUP_VERSION: u32 = 1;
pub const BACKUP_ITERATIONS: u32 = 200_000;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncryptedBackupEnvelope {
    pub format: String,
    pub version: u32,
    pub exported_at: String,
    pub profile_name: String,
    pub iterations: u32,
    pub salt_b64: String,
    pub nonce_b64: String,
    pub ciphertext_b64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BackupPluginRequirement {
    pub plugin_id: String,
    pub version: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub is_official: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupRestorePluginItem {
    pub plugin_id: String,
    pub display_name: String,
    pub required_version: Option<String>,
    pub installed_version: Option<String>,
    pub target_version: Option<String>,
    pub icon_svg: Option<String>,
    pub brand_colour: String,
    pub is_official: bool,
    pub verified: bool,
    pub version_differs: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupRestorePreview {
    pub profile_name: String,
    pub backup_app_version: Option<String>,
    pub current_app_version: String,
    pub is_older_version: bool,
    pub is_newer_version: bool,
    pub version_warning: Option<String>,
    pub already_installed: Vec<BackupRestorePluginItem>,
    pub missing_available: Vec<BackupRestorePluginItem>,
    pub missing_unavailable: Vec<BackupRestorePluginItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupPayload {
    pub profile_name: String,
    #[serde(default)]
    pub app_version: String,
    pub credentials: HashMap<String, HashMap<String, String>>,
    pub history: Vec<HistoryEntry>,
    pub frontend_state: HashMap<String, String>,
    #[serde(default)]
    pub required_plugins: Vec<BackupPluginRequirement>,
}

pub fn evaluate_backup_app_version(
    raw_backup_version: &str,
    current_version_str: &str,
) -> (Option<String>, bool, bool, Option<String>) {
    let trimmed = raw_backup_version.trim();
    if trimmed.is_empty() {
        return (
            None,
            true,
            false,
            Some(format!(
                "This backup was created with an older version of SocialPoster (pre-v{current_version_str}). You can still restore it safely, though any unrecognised legacy fields will be ignored."
            )),
        );
    }

    let backup_label = trimmed.to_string();
    let Ok(backup_ver) = semver::Version::parse(trimmed.trim_start_matches('v')) else {
        return (
            Some(backup_label.clone()),
            true,
            false,
            Some(format!(
                "This backup reports an unrecognised SocialPoster version ({backup_label}). You can still restore it safely, though any unrecognised legacy fields will be ignored."
            )),
        );
    };

    let Ok(current_ver) = semver::Version::parse(current_version_str.trim_start_matches('v'))
    else {
        return (Some(backup_label), false, false, None);
    };

    if backup_ver < current_ver {
        (
            Some(backup_label.clone()),
            true,
            false,
            Some(format!(
                "This backup was created with an older version of SocialPoster (v{backup_label}). You can still restore it safely, though any unrecognised legacy fields will be ignored."
            )),
        )
    } else if backup_ver > current_ver {
        (
            Some(backup_label.clone()),
            false,
            true,
            Some(format!(
                "This backup was created with a newer version of SocialPoster (v{backup_label}, current: v{current_version_str}). Some newer settings may be ignored."
            )),
        )
    } else {
        (Some(backup_label), false, false, None)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupMetadata {
    pub profile_name: String,
    pub exported_at: String,
    pub version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedProfileResponse {
    pub profile: ProfileInfo,
    pub frontend_state: HashMap<String, String>,
    pub restored_platforms_count: usize,
    pub restored_history_count: usize,
}

fn derive_key(salt: &[u8], passphrase: &str, iterations: u32) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(salt);
    hasher.update(passphrase.as_bytes());
    let mut digest: [u8; 32] = hasher.finalize().into();

    for _ in 1..iterations {
        digest = Sha256::digest(digest).into();
    }
    digest
}

pub fn encrypt_payload(
    payload: &BackupPayload,
    passphrase: &str,
) -> Result<EncryptedBackupEnvelope, String> {
    if passphrase.trim().len() < 8 {
        return Err("Passphrase must be at least 8 characters.".to_string());
    }

    let plaintext = serde_json::to_vec(payload)
        .map_err(|err| format!("Failed to serialise backup payload: {err}"))?;

    let mut salt = [0u8; SALT_LEN];
    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut salt);
    OsRng.fill_bytes(&mut nonce_bytes);

    let key = derive_key(&salt, passphrase, BACKUP_ITERATIONS);
    let cipher = ChaCha20Poly1305::new_from_slice(&key)
        .map_err(|err| format!("Failed to initialise backup cipher: {err}"))?;
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_ref())
        .map_err(|err| format!("Failed to encrypt backup payload: {err}"))?;

    Ok(EncryptedBackupEnvelope {
        format: BACKUP_FORMAT.to_string(),
        version: BACKUP_VERSION,
        exported_at: Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        profile_name: payload.profile_name.clone(),
        iterations: BACKUP_ITERATIONS,
        salt_b64: STANDARD.encode(salt),
        nonce_b64: STANDARD.encode(nonce_bytes),
        ciphertext_b64: STANDARD.encode(ciphertext),
    })
}

pub fn parse_and_validate_envelope(raw: &str) -> Result<EncryptedBackupEnvelope, String> {
    let envelope: EncryptedBackupEnvelope = serde_json::from_str(raw)
        .map_err(|_| "Selected file is not a valid SocialPoster backup.".to_string())?;

    if envelope.format != BACKUP_FORMAT {
        return Err(format!("Unsupported backup format '{}'.", envelope.format));
    }
    if envelope.version != BACKUP_VERSION {
        return Err(format!(
            "Unsupported backup version '{}'.",
            envelope.version
        ));
    }
    if envelope.iterations == 0 {
        return Err("Backup archive has invalid key derivation settings.".to_string());
    }

    Ok(envelope)
}

pub fn inspect_envelope(raw: &str) -> Result<BackupMetadata, String> {
    let envelope = parse_and_validate_envelope(raw)?;
    Ok(BackupMetadata {
        profile_name: envelope.profile_name,
        exported_at: envelope.exported_at,
        version: envelope.version,
    })
}

pub fn decrypt_envelope(
    envelope: &EncryptedBackupEnvelope,
    passphrase: &str,
) -> Result<BackupPayload, String> {
    if passphrase.is_empty() {
        return Err("Passphrase is required.".to_string());
    }

    let salt = STANDARD
        .decode(&envelope.salt_b64)
        .map_err(|_| "Backup salt is corrupted.".to_string())?;
    if salt.len() != SALT_LEN {
        return Err("Backup salt has an invalid length.".to_string());
    }

    let nonce_bytes = STANDARD
        .decode(&envelope.nonce_b64)
        .map_err(|_| "Backup nonce is corrupted.".to_string())?;
    if nonce_bytes.len() != NONCE_LEN {
        return Err("Backup nonce has an invalid length.".to_string());
    }

    let ciphertext = STANDARD
        .decode(&envelope.ciphertext_b64)
        .map_err(|_| "Backup ciphertext is corrupted.".to_string())?;

    let key = derive_key(&salt, passphrase, envelope.iterations);
    let cipher = ChaCha20Poly1305::new_from_slice(&key)
        .map_err(|err| format!("Failed to initialise backup cipher: {err}"))?;
    let nonce = Nonce::from_slice(&nonce_bytes);

    let plaintext = cipher
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|_| "Incorrect passphrase or corrupted backup file.".to_string())?;

    serde_json::from_slice(&plaintext).map_err(|err| format!("Backup payload is corrupted: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_payload() -> BackupPayload {
        let mut creds = HashMap::new();
        let mut bsky = HashMap::new();
        bsky.insert("handle".to_string(), "alice.bsky.social".to_string());
        bsky.insert(
            "app_password".to_string(),
            "abcd-efgh-ijkl-mnop".to_string(),
        );
        creds.insert("bluesky".to_string(), bsky);

        let mut frontend_state = HashMap::new();
        frontend_state.insert("enabled-platforms".to_string(), "[\"bluesky\"]".to_string());

        BackupPayload {
            profile_name: "Work".to_string(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            credentials: creds,
            history: Vec::new(),
            frontend_state,
            required_plugins: vec![BackupPluginRequirement {
                plugin_id: "bluesky".to_string(),
                version: "1.0.0".to_string(),
                display_name: Some("Bluesky".to_string()),
                is_official: true,
            }],
        }
    }

    #[test]
    fn round_trip_encrypts_and_decrypts_payload() {
        let payload = sample_payload();
        let envelope =
            encrypt_payload(&payload, "correct-horse-battery-staple").expect("encrypts payload");
        let json = serde_json::to_string_pretty(&envelope).expect("serialises envelope");

        let meta = inspect_envelope(&json).expect("inspects envelope");
        assert_eq!(meta.profile_name, "Work");
        assert_eq!(meta.version, BACKUP_VERSION);

        let parsed = parse_and_validate_envelope(&json).expect("validates envelope");
        let decrypted =
            decrypt_envelope(&parsed, "correct-horse-battery-staple").expect("decrypts payload");
        assert_eq!(decrypted.profile_name, "Work");
        assert_eq!(
            decrypted
                .credentials
                .get("bluesky")
                .and_then(|c| c.get("handle"))
                .map(String::as_str),
            Some("alice.bsky.social")
        );
        assert_eq!(
            decrypted
                .frontend_state
                .get("enabled-platforms")
                .map(String::as_str),
            Some("[\"bluesky\"]")
        );
        assert_eq!(decrypted.required_plugins.len(), 1);
        assert_eq!(decrypted.required_plugins[0].plugin_id, "bluesky");
        assert_eq!(decrypted.required_plugins[0].version, "1.0.0");
    }

    #[test]
    fn deserialises_legacy_backup_payload_without_required_plugins() {
        let legacy_json = r#"{
            "profileName": "Legacy",
            "credentials": { "discord": { "webhook_url": "https://example.com" } },
            "history": [],
            "frontendState": {}
        }"#;
        let payload: BackupPayload =
            serde_json::from_str(legacy_json).expect("deserialises legacy backup");
        assert!(payload.app_version.is_empty());
        assert!(payload.required_plugins.is_empty());
        assert!(payload.credentials.contains_key("discord"));
    }

    #[test]
    fn evaluates_backup_app_version_warnings() {
        let (ver_empty, older_empty, newer_empty, warn_empty) =
            evaluate_backup_app_version("", "1.0.0");
        assert_eq!(ver_empty, None);
        assert!(older_empty);
        assert!(!newer_empty);
        assert!(warn_empty.unwrap().contains("older version"));

        let (ver_old, older_old, newer_old, warn_old) =
            evaluate_backup_app_version("0.3.0", "1.0.0");
        assert_eq!(ver_old.as_deref(), Some("0.3.0"));
        assert!(older_old);
        assert!(!newer_old);
        assert!(warn_old.unwrap().contains("v0.3.0"));

        let (ver_same, older_same, newer_same, warn_same) =
            evaluate_backup_app_version("1.0.0", "1.0.0");
        assert_eq!(ver_same.as_deref(), Some("1.0.0"));
        assert!(!older_same);
        assert!(!newer_same);
        assert!(warn_same.is_none());

        let (ver_new, older_new, newer_new, warn_new) =
            evaluate_backup_app_version("1.2.0", "1.0.0");
        assert_eq!(ver_new.as_deref(), Some("1.2.0"));
        assert!(!older_new);
        assert!(newer_new);
        assert!(warn_new.unwrap().contains("newer version"));
    }

    #[test]
    fn rejects_wrong_passphrase() {
        let payload = sample_payload();
        let envelope = encrypt_payload(&payload, "correct-passphrase").expect("encrypts");
        let err = decrypt_envelope(&envelope, "wrong-passphrase").expect_err("rejects wrong key");
        assert!(err.contains("Incorrect passphrase"));
    }

    #[test]
    fn rejects_tampered_ciphertext() {
        let payload = sample_payload();
        let mut envelope = encrypt_payload(&payload, "correct-passphrase").expect("encrypts");
        let mut raw = STANDARD
            .decode(&envelope.ciphertext_b64)
            .expect("decodes ciphertext");
        raw[0] ^= 0xFF;
        envelope.ciphertext_b64 = STANDARD.encode(raw);

        let err =
            decrypt_envelope(&envelope, "correct-passphrase").expect_err("rejects tampered bytes");
        assert!(err.contains("Incorrect passphrase or corrupted"));
    }

    #[test]
    fn rejects_invalid_format_and_version() {
        let payload = sample_payload();
        let mut envelope = encrypt_payload(&payload, "correct-passphrase").expect("encrypts");
        envelope.format = "other-app".to_string();
        let raw_bad_fmt = serde_json::to_string(&envelope).unwrap();
        assert!(parse_and_validate_envelope(&raw_bad_fmt).is_err());

        envelope.format = BACKUP_FORMAT.to_string();
        envelope.version = 99;
        let raw_bad_ver = serde_json::to_string(&envelope).unwrap();
        assert!(parse_and_validate_envelope(&raw_bad_ver).is_err());
    }
}
