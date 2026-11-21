use crate::core::platform::{PlatformMetadata, SocialPlatform};
use semver::{Version, VersionReq};

/// Collects all dynamically loaded WebAssembly platform plugins, sorted by `id`.
pub fn all_platforms() -> Vec<Box<dyn SocialPlatform>> {
    let mut platforms = crate::plugins::all_wasm_platforms();
    platforms.sort_by(|a, b| a.id().cmp(b.id()));
    platforms
}

/// Looks up a single dynamically loaded WebAssembly platform plugin by `id`.
pub fn find_platform(id: &str) -> Option<Box<dyn SocialPlatform>> {
    crate::plugins::find_wasm_platform(id)
}

/// Evaluates whether a SemVer requirement string is satisfied by `host_version_str`.
pub fn evaluate_compatibility(host_version_str: &str, req_str: &str) -> (bool, Option<String>) {
    let host_version = match Version::parse(host_version_str) {
        Ok(v) => v,
        Err(err) => {
            return (
                false,
                Some(format!(
                    "Invalid host application version '{host_version_str}': {err}"
                )),
            );
        }
    };

    let req = match VersionReq::parse(req_str) {
        Ok(r) => r,
        Err(err) => {
            return (
                false,
                Some(format!(
                    "Invalid compatibility constraint '{req_str}': {err}"
                )),
            );
        }
    };

    if req.matches(&host_version) {
        (true, None)
    } else {
        (
            false,
            Some(format!(
                "Requires SocialPoster {req_str} (current: {host_version})"
            )),
        )
    }
}

/// Checks whether a platform plugin is compatible with the current SocialPoster host version.
pub fn check_compatibility(platform: &dyn SocialPlatform) -> (bool, Option<String>) {
    evaluate_compatibility(env!("CARGO_PKG_VERSION"), platform.compatible_app_version())
}

/// Builds serialisable UI metadata for a platform plugin.
pub fn to_metadata(
    platform: &dyn SocialPlatform,
    is_configured: bool,
    active_auth_mode: Option<String>,
) -> PlatformMetadata {
    let (is_compatible, compatibility_error) = check_compatibility(platform);

    PlatformMetadata {
        id: platform.id().to_string(),
        name: platform.name().to_string(),
        accent_colour: platform.accent_colour().to_string(),
        version: platform.version().to_string(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        compatible_app_version: platform.compatible_app_version().to_string(),
        is_compatible,
        compatibility_error,
        is_official: platform.is_official(),
        authors: platform.authors(),
        auth_modes: platform.auth_modes(),
        active_auth_mode,
        setup_instructions: platform.setup_instructions().to_string(),
        capabilities: platform.capabilities(),
        is_configured,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_none_for_unknown_platform() {
        assert!(find_platform("non_existent_platform").is_none());
    }

    #[test]
    fn evaluates_semver_compatibility_safely() {
        assert_eq!(evaluate_compatibility("0.1.0", "^0.1.0"), (true, None));

        let (ok_mismatch, err_mismatch) = evaluate_compatibility("0.1.0", "^0.2.0");
        assert!(!ok_mismatch);
        assert_eq!(
            err_mismatch.as_deref(),
            Some("Requires SocialPoster ^0.2.0 (current: 0.1.0)")
        );

        let (ok_invalid, err_invalid) = evaluate_compatibility("0.1.0", "not-a-semver");
        assert!(!ok_invalid);
        assert!(
            err_invalid
                .as_deref()
                .unwrap_or("")
                .starts_with("Invalid compatibility constraint 'not-a-semver':")
        );
    }

    #[test]
    fn all_registered_platform_semver_constraints_parse_cleanly() {
        for platform in all_platforms() {
            let req_str = platform.compatible_app_version();
            assert!(
                VersionReq::parse(req_str).is_ok(),
                "platform '{}' has invalid compatible_app_version '{}'",
                platform.id(),
                req_str
            );
            assert!(
                Version::parse(platform.version()).is_ok(),
                "platform '{}' has invalid version '{}'",
                platform.id(),
                platform.version()
            );
        }
    }
}
