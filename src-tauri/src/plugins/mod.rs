pub mod registry;
pub mod wasm_platform;

use crate::core::platform::{
    AuthMode, PlatformAuthor, PlatformCapabilities, PostOutcome, PostPayload, SocialPlatform,
};
use crate::plugins::registry::{
    InstalledPluginRecord, ManifestCapabilities, PluginManifestDetail, RegistryPluginView,
};
use crate::plugins::wasm_platform::WasmPlatform;
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock, RwLock};
use tauri::AppHandle;

static LOADED_WASM_PLUGINS: OnceLock<RwLock<HashMap<String, Arc<WasmPlatform>>>> = OnceLock::new();

fn live_map() -> &'static RwLock<HashMap<String, Arc<WasmPlatform>>> {
    LOADED_WASM_PLUGINS.get_or_init(|| RwLock::new(HashMap::new()))
}

struct ArcWasmPlatform(Arc<WasmPlatform>);

#[async_trait]
impl SocialPlatform for ArcWasmPlatform {
    fn id(&self) -> &str {
        self.0.id()
    }

    fn name(&self) -> &str {
        self.0.name()
    }

    fn accent_colour(&self) -> &str {
        self.0.accent_colour()
    }

    fn version(&self) -> &str {
        self.0.version()
    }

    fn compatible_app_version(&self) -> &str {
        self.0.compatible_app_version()
    }

    fn is_official(&self) -> bool {
        self.0.is_official()
    }

    fn authors(&self) -> Vec<PlatformAuthor> {
        self.0.authors()
    }

    fn auth_modes(&self) -> Vec<AuthMode> {
        self.0.auth_modes()
    }

    fn setup_instructions(&self) -> &str {
        self.0.setup_instructions()
    }

    fn capabilities(&self) -> PlatformCapabilities {
        self.0.capabilities()
    }

    async fn verify_credentials(&self, creds: &HashMap<String, String>) -> Result<(), String> {
        self.0.verify_credentials(creds).await
    }

    async fn publish(
        &self,
        post: &PostPayload,
        creds: &HashMap<String, String>,
    ) -> Result<PostOutcome, String> {
        self.0.publish(post, creds).await
    }

    async fn delete_post(
        &self,
        creds: &HashMap<String, String>,
        post_id: &str,
    ) -> Result<(), String> {
        self.0.delete_post(creds, post_id).await
    }
}

pub fn all_wasm_platforms() -> Vec<Box<dyn SocialPlatform>> {
    let Ok(guard) = live_map().read() else {
        return Vec::new();
    };
    guard
        .values()
        .map(|arc| Box::new(ArcWasmPlatform(Arc::clone(arc))) as Box<dyn SocialPlatform>)
        .collect()
}

pub fn find_wasm_platform(id: &str) -> Option<Box<dyn SocialPlatform>> {
    let Ok(guard) = live_map().read() else {
        return None;
    };
    guard
        .get(id)
        .map(|arc| Box::new(ArcWasmPlatform(Arc::clone(arc))) as Box<dyn SocialPlatform>)
}

pub fn reload_installed_plugins(app: &AppHandle) -> Result<(), String> {
    let records = registry::load_installed_records(app)?;
    let mut next_map: HashMap<String, Arc<WasmPlatform>> = HashMap::new();

    for record in records {
        if !record.enabled {
            continue;
        }
        let path = PathBuf::from(&record.wasm_path);
        if !path.is_file() {
            continue;
        }
        match WasmPlatform::from_wasm_file(&path) {
            Ok(platform) => {
                let fallback_authors = registry::load_cached_manifest_authors(app, &record.id);
                next_map.insert(
                    platform.id().to_string(),
                    Arc::new(
                        platform
                            .with_official(record.is_official)
                            .with_fallback_authors(fallback_authors),
                    ),
                );
            }
            Err(err) => {
                eprintln!(
                    "Warning: failed to load WebAssembly plugin '{}': {err}",
                    record.id
                );
            }
        }
    }

    if let Ok(mut guard) = live_map().write() {
        *guard = next_map;
    }
    Ok(())
}

#[tauri::command]
pub async fn get_registry_plugins(app: AppHandle) -> Result<Vec<RegistryPluginView>, String> {
    let installed = registry::load_installed_records(&app).unwrap_or_default();
    let installed_map: HashMap<String, InstalledPluginRecord> =
        installed.into_iter().map(|r| (r.id.clone(), r)).collect();

    let index = registry::fetch_registry_index(&app).await?;
    let mut views = Vec::new();
    let mut seen_ids = std::collections::HashSet::new();

    for entry in index.plugins {
        if entry.is_deprecated {
            continue;
        }
        seen_ids.insert(entry.slug.clone());
        let inst = installed_map.get(&entry.slug);
        let update_available = inst
            .map(|r| r.version != entry.latest_version)
            .unwrap_or(false);
        let is_official = registry::is_official_plugin(entry.is_official, entry.author.as_ref());
        let authors = entry
            .author
            .map(|a| vec![a.display_name])
            .unwrap_or_default();
        let cached_icon = registry::load_cached_manifest_icon(&app, &entry.slug);
        let compat_req = entry
            .compatible_app_version
            .clone()
            .unwrap_or_else(|| ">=1.0.0".to_string());
        let app_version = env!("CARGO_PKG_VERSION").to_string();
        let (is_compatible, compatibility_error) =
            crate::core::registry::evaluate_compatibility(&app_version, &compat_req);

        views.push(RegistryPluginView {
            id: entry.slug,
            name: entry.display_name,
            description: entry.summary,
            latest_version: Some(entry.latest_version),
            installed_version: inst.map(|r| r.version.clone()),
            is_installed: inst.is_some(),
            update_available,
            enabled: inst.map(|r| r.enabled).unwrap_or(false),
            verified: inst.map(|r| r.verified).unwrap_or(true),
            is_official,
            source: inst
                .map(|r| r.source.clone())
                .unwrap_or_else(|| "registry".to_string()),
            brand_colour: inst
                .and_then(|r| r.brand_colour.clone())
                .or(entry.brand_colour)
                .unwrap_or_else(|| "#64748B".to_string()),
            icon_svg: inst
                .and_then(|r| r.icon_svg.clone())
                .or(entry.icon_svg)
                .or(cached_icon),
            allowed_domains: inst
                .map(|r| r.allowed_domains.clone())
                .unwrap_or(entry.allowed_domains),
            authors,
            updated_at: index.generated_at.clone(),
            app_version,
            compatible_app_version: compat_req,
            is_compatible,
            compatibility_error,
        });
    }

    for (id, inst) in installed_map {
        if seen_ids.contains(&id) {
            continue;
        }
        let cached_icon = registry::load_cached_manifest_icon(&app, &id);
        let compat_req = ">=1.0.0".to_string();
        let app_version = env!("CARGO_PKG_VERSION").to_string();
        let (is_compatible, compatibility_error) =
            crate::core::registry::evaluate_compatibility(&app_version, &compat_req);
        views.push(RegistryPluginView {
            id: inst.id,
            name: inst.name,
            description: None,
            latest_version: None,
            installed_version: Some(inst.version),
            is_installed: true,
            update_available: false,
            enabled: inst.enabled,
            verified: inst.verified,
            is_official: inst.is_official,
            source: inst.source,
            brand_colour: inst.brand_colour.unwrap_or_else(|| "#64748B".to_string()),
            icon_svg: inst.icon_svg.or(cached_icon),
            allowed_domains: inst.allowed_domains,
            authors: Vec::new(),
            updated_at: Some(inst.installed_at),
            app_version,
            compatible_app_version: compat_req,
            is_compatible,
            compatibility_error,
        });
    }

    views.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(views)
}

#[tauri::command]
pub async fn get_plugin_manifest(
    app: AppHandle,
    plugin_id: String,
) -> Result<PluginManifestDetail, String> {
    let installed = registry::load_installed_records(&app).unwrap_or_default();
    let inst = installed.into_iter().find(|r| r.id == plugin_id);

    let index_opt = registry::fetch_registry_index(&app).await.ok();
    let entry_opt = index_opt
        .as_ref()
        .and_then(|idx| idx.plugins.iter().find(|p| p.slug == plugin_id));

    let manifest = registry::fetch_plugin_manifest(
        &app,
        &plugin_id,
        entry_opt.and_then(|e| e.manifest_url.as_deref()),
        entry_opt.map(|e| e.latest_version.as_str()),
    )
    .await?;

    let latest_version = manifest
        .latest_version
        .clone()
        .or_else(|| entry_opt.map(|e| e.latest_version.clone()))
        .or_else(|| manifest.versions.first().map(|v| v.version.clone()))
        .unwrap_or_else(|| "0.1.0".to_string());

    let update_available = inst
        .as_ref()
        .map(|r| r.version != latest_version)
        .unwrap_or(false);

    let resolved_icon = manifest.resolved_icon_svg();
    let capabilities = manifest.capabilities.unwrap_or(ManifestCapabilities {
        max_characters: None,
        max_attachments: None,
        supported_media_types: Vec::new(),
        supports_alt_text: false,
        supports_threads: false,
        supports_delete: false,
        allowed_domains: Vec::new(),
        credential_fields: Vec::new(),
        auth_modes: Vec::new(),
    });

    let allowed_domains = if !manifest.allowed_domains.is_empty() {
        manifest.allowed_domains
    } else if !capabilities.allowed_domains.is_empty() {
        capabilities.allowed_domains.clone()
    } else {
        inst.as_ref()
            .map(|r| r.allowed_domains.clone())
            .unwrap_or_default()
    };

    let brand_colour = manifest
        .brand_colour
        .clone()
        .or_else(|| entry_opt.and_then(|e| e.brand_colour.clone()))
        .or_else(|| inst.as_ref().and_then(|r| r.brand_colour.clone()))
        .unwrap_or_else(|| "#64748B".to_string());

    let resolved_author = manifest
        .author
        .clone()
        .or_else(|| entry_opt.and_then(|e| e.author.clone()));
    let is_official = registry::is_official_plugin(
        manifest.is_official || entry_opt.is_some_and(|e| e.is_official),
        resolved_author.as_ref(),
    );
    let icon_svg = resolved_icon.or_else(|| inst.as_ref().and_then(|r| r.icon_svg.clone()));
    let compat_req = manifest
        .compatible_app_version
        .clone()
        .or_else(|| entry_opt.and_then(|e| e.compatible_app_version.clone()))
        .unwrap_or_else(|| ">=1.0.0".to_string());
    let app_version = env!("CARGO_PKG_VERSION").to_string();
    let (is_compatible, compatibility_error) =
        crate::core::registry::evaluate_compatibility(&app_version, &compat_req);

    let versions = manifest
        .versions
        .into_iter()
        .map(|mut v| {
            let req = v
                .compatible_app_version
                .clone()
                .unwrap_or_else(|| compat_req.clone());
            let (ok, msg) = crate::core::registry::evaluate_compatibility(&app_version, &req);
            v.compatible_app_version = Some(req);
            v.is_compatible = ok;
            v.compatibility_message = msg;
            v
        })
        .collect();

    Ok(PluginManifestDetail {
        id: manifest.slug,
        name: manifest
            .display_name
            .or_else(|| entry_opt.map(|e| e.display_name.clone()))
            .unwrap_or_else(|| plugin_id.clone()),
        description: manifest
            .summary
            .or_else(|| entry_opt.and_then(|e| e.summary.clone())),
        author: resolved_author,
        is_official,
        brand_colour,
        icon_svg,
        setup_instructions: manifest.setup_instructions,
        app_version,
        compatible_app_version: compat_req,
        is_compatible,
        compatibility_error,
        latest_version,
        installed_version: inst.as_ref().map(|r| r.version.clone()),
        is_installed: inst.is_some(),
        update_available,
        enabled: inst.as_ref().map(|r| r.enabled).unwrap_or(false),
        verified: inst.as_ref().map(|r| r.verified).unwrap_or(true),
        allowed_domains,
        capabilities,
        versions,
        hub_url: format!("https://socialposter.dev/plugins/{plugin_id}"),
    })
}

#[tauri::command]
pub async fn install_registry_plugin(
    app: AppHandle,
    plugin_id: String,
    version: Option<String>,
) -> Result<InstalledPluginRecord, String> {
    let record = registry::install_from_registry(&app, &plugin_id, version.as_deref()).await?;
    reload_installed_plugins(&app)?;
    Ok(record)
}

#[tauri::command]
pub async fn install_local_plugin(
    app: AppHandle,
    file_path: String,
) -> Result<InstalledPluginRecord, String> {
    let record = registry::install_local_wasm(&app, std::path::Path::new(&file_path))?;
    reload_installed_plugins(&app)?;
    Ok(record)
}

#[tauri::command]
pub async fn set_plugin_enabled(
    app: AppHandle,
    plugin_id: String,
    enabled: bool,
) -> Result<(), String> {
    registry::set_plugin_enabled(&app, &plugin_id, enabled)?;
    reload_installed_plugins(&app)
}

#[tauri::command]
pub async fn uninstall_plugin(app: AppHandle, plugin_id: String) -> Result<(), String> {
    registry::uninstall_plugin(&app, &plugin_id)?;
    reload_installed_plugins(&app)
}
