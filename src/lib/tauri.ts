import { invoke } from "@tauri-apps/api/core";

export interface ProfileInfo {
  id: string;
  name: string;
  hasPassword: boolean;
  isUnlocked: boolean;
  createdAt: string;
}

export interface AuthorLink {
  label: string;
  url: string;
}

export interface PlatformAuthor {
  name: string;
  links: AuthorLink[];
}

export interface SelectOption {
  value: string;
  label: string;
}

export type FieldKind =
  | { type: "text" }
  | { type: "secret" }
  | { type: "url" }
  | { type: "select"; options: SelectOption[] }
  | { type: "oAuthButton"; buttonLabel: string; targetKey: string };

export interface AuthField {
  key: string;
  label: string;
  placeholder: string;
  kind: FieldKind;
  required: boolean;
  helpText: string | null;
}

export interface AuthMode {
  id: string;
  label: string;
  description: string | null;
  fields: AuthField[];
  setupInstructions: string | null;
}

export interface PlatformCapabilities {
  maxChars: number;
  maxImages: number;
  maxImageBytes?: number | null;
  supportsVideo: boolean;
  requiresMedia: boolean;
  supportsThreads?: boolean;
  supportsDelete?: boolean;
}

export interface PlatformMetadata {
  id: string;
  name: string;
  accentColour: string;
  version: string;
  appVersion: string;
  compatibleAppVersion: string;
  isOfficial?: boolean;
  isCompatible: boolean;
  compatibilityError: string | null;
  authors: PlatformAuthor[];
  authModes: AuthMode[];
  activeAuthMode: string | null;
  setupInstructions: string;
  capabilities: PlatformCapabilities;
  isConfigured: boolean;
}

export interface MediaAttachment {
  path: string;
  mimeType: string;
  altText?: string | null;
}

export interface PostPayload {
  text: string;
  media: MediaAttachment[];
}

export interface PlatformDispatchRequest {
  platformId: string;
  text: string;
  media: MediaAttachment[];
}

export interface PostOutcome {
  platformId: string;
  platformName: string;
  success: boolean;
  postId?: string | null;
  postIds?: string[];
  postUrl: string | null;
  error: string | null;
  notes?: string[];
  recalled?: boolean;
  recalledAt?: string | null;
  recallError?: string | null;
}

export interface HistoryEntry {
  id: string;
  createdAt: string;
  masterText: string;
  overrides: Record<string, string>;
  media: MediaAttachment[];
  outcomes: PostOutcome[];
}

export interface DesktopSettings {
  minimizeToTray: boolean;
  globalShortcut: string | null;
}

export interface SavedMediaRef {
  filename: string;
  mimeType: string;
  altText?: string | null;
  relativePath: string;
}

export interface ScheduledPost {
  id: string;
  profileId: string;
  scheduledFor: string;
  text: string;
  overrides: Record<string, string>;
  mediaRefs: SavedMediaRef[];
  platformIds: string[];
  autoThread: boolean;
  status: "pending" | "publishing" | "failed" | string;
  error: string | null;
  createdAt: string;
}

export interface ScheduledPostsResponse {
  isLocked: boolean;
  posts: ScheduledPost[];
}

export async function listProfiles(): Promise<ProfileInfo[]> {
  return invoke<ProfileInfo[]>("list_profiles");
}

export async function createProfile(
  name: string,
  password?: string | null,
): Promise<ProfileInfo> {
  return invoke<ProfileInfo>("create_profile", {
    name,
    password: password && password.length > 0 ? password : null,
  });
}

export async function renameProfile(
  profileId: string,
  name: string,
): Promise<ProfileInfo> {
  return invoke<ProfileInfo>("rename_profile", { profileId, name });
}

export async function unlockProfile(
  profileId: string,
  password: string,
): Promise<boolean> {
  return invoke<boolean>("unlock_profile", { profileId, password });
}

export async function lockProfile(profileId: string): Promise<void> {
  return invoke<void>("lock_profile", { profileId });
}

export async function setProfilePassword(
  profileId: string,
  currentPassword: string | null,
  newPassword: string | null,
): Promise<ProfileInfo> {
  return invoke<ProfileInfo>("set_profile_password", {
    profileId,
    currentPassword:
      currentPassword && currentPassword.length > 0 ? currentPassword : null,
    newPassword: newPassword && newPassword.length > 0 ? newPassword : null,
  });
}

export async function deleteProfile(profileId: string): Promise<void> {
  return invoke<void>("delete_profile", { profileId });
}

export async function getPlatforms(
  profileId: string,
): Promise<PlatformMetadata[]> {
  return invoke<PlatformMetadata[]>("get_platforms", { profileId });
}

export async function saveCredentials(
  profileId: string,
  platformId: string,
  modeId: string,
  values: Record<string, string>,
): Promise<void> {
  return invoke<void>("save_credentials", {
    profileId,
    platformId,
    modeId,
    values,
  });
}

export async function verifyPlatform(
  profileId: string,
  platformId: string,
): Promise<void> {
  return invoke<void>("verify_platform", { profileId, platformId });
}

export async function unlinkPlatform(
  profileId: string,
  platformId: string,
): Promise<void> {
  return invoke<void>("unlink_platform", { profileId, platformId });
}

export async function startPlatformOauth(
  profileId: string,
  platformId: string,
  modeId: string,
  inputs: Record<string, string>,
): Promise<void> {
  return invoke<void>("start_platform_oauth", {
    profileId,
    platformId,
    modeId,
    inputs,
  });
}

export async function broadcastPost(
  profileId: string,
  targets: PlatformDispatchRequest[],
  masterText: string,
  overrides: Record<string, string>,
  media: MediaAttachment[],
  autoThread: boolean,
): Promise<PostOutcome[]> {
  return invoke<PostOutcome[]>("broadcast_post", {
    profileId,
    targets,
    masterText,
    overrides,
    media,
    autoThread,
  });
}

export async function getHistory(profileId: string): Promise<HistoryEntry[]> {
  return invoke<HistoryEntry[]>("get_history", { profileId });
}

export async function clearHistory(profileId: string): Promise<void> {
  return invoke<void>("clear_history", { profileId });
}

export async function recallPost(
  profileId: string,
  historyId: string,
  platformIds?: string[],
): Promise<HistoryEntry> {
  return invoke<HistoryEntry>("recall_post", {
    profileId,
    historyId,
    platformIds: platformIds ?? null,
  });
}

export async function getDesktopSettings(): Promise<DesktopSettings> {
  return invoke<DesktopSettings>("get_desktop_settings");
}

export async function updateDesktopSettings(
  settings: DesktopSettings,
): Promise<DesktopSettings> {
  return invoke<DesktopSettings>("update_desktop_settings", { settings });
}

export async function schedulePost(
  profileId: string,
  scheduledFor: string,
  text: string,
  overrides: Record<string, string>,
  media: MediaAttachment[],
  platformIds: string[],
  autoThread: boolean,
): Promise<ScheduledPost> {
  return invoke<ScheduledPost>("schedule_post", {
    profileId,
    scheduledFor,
    text,
    overrides,
    media,
    platformIds,
    autoThread,
  });
}

export async function listScheduledPosts(
  profileId: string,
): Promise<ScheduledPostsResponse> {
  return invoke<ScheduledPostsResponse>("list_scheduled_posts", { profileId });
}

export async function cancelScheduledPost(
  profileId: string,
  jobId: string,
): Promise<void> {
  return invoke<void>("cancel_scheduled_post", { profileId, jobId });
}

export async function publishScheduledNow(
  profileId: string,
  jobId: string,
): Promise<PostOutcome[]> {
  return invoke<PostOutcome[]>("publish_scheduled_now", { profileId, jobId });
}

export async function retryScheduledPost(
  profileId: string,
  jobId: string,
): Promise<ScheduledPost> {
  return invoke<ScheduledPost>("retry_scheduled_post", { profileId, jobId });
}

export interface BackupMetadata {
  profileName: string;
  exportedAt: string;
  version: number;
}

export interface BackupRestorePluginItem {
  pluginId: string;
  displayName: string;
  requiredVersion: string | null;
  installedVersion: string | null;
  targetVersion: string | null;
  iconSvg: string | null;
  brandColour: string;
  isOfficial: boolean;
  verified: boolean;
  versionDiffers: boolean;
  reason: string | null;
}

export interface BackupRestorePreview {
  profileName: string;
  backupAppVersion: string | null;
  currentAppVersion: string;
  isOlderVersion: boolean;
  isNewerVersion: boolean;
  versionWarning: string | null;
  alreadyInstalled: BackupRestorePluginItem[];
  missingAvailable: BackupRestorePluginItem[];
  missingUnavailable: BackupRestorePluginItem[];
}

export interface ImportedProfileResponse {
  profile: ProfileInfo;
  frontendState: Record<string, string>;
  restoredPlatformsCount: number;
  restoredHistoryCount: number;
}

export async function exportProfileBackup(
  profileId: string,
  filePath: string,
  passphrase: string,
  frontendState: Record<string, string>,
): Promise<void> {
  return invoke<void>("export_profile_backup", {
    profileId,
    filePath,
    passphrase,
    frontendState,
  });
}

export async function inspectProfileBackup(
  filePath: string,
): Promise<BackupMetadata> {
  return invoke<BackupMetadata>("inspect_profile_backup", { filePath });
}

export async function previewBackupRestore(
  filePath: string,
  passphrase: string,
): Promise<BackupRestorePreview> {
  return invoke<BackupRestorePreview>("preview_backup_restore", {
    filePath,
    passphrase,
  });
}

export async function importProfileBackup(
  filePath: string,
  passphrase: string,
  customName: string | null,
): Promise<ImportedProfileResponse> {
  return invoke<ImportedProfileResponse>("import_profile_backup", {
    filePath,
    passphrase,
    customName,
  });
}

export interface RegistryPluginView {
  id: string;
  name: string;
  description: string | null;
  latestVersion: string | null;
  installedVersion: string | null;
  isInstalled: boolean;
  updateAvailable: boolean;
  enabled: boolean;
  verified: boolean;
  isOfficial?: boolean;
  source: string;
  brandColour: string;
  iconSvg: string | null;
  allowedDomains: string[];
  authors: string[];
  updatedAt?: string | null;
  appVersion: string;
  compatibleAppVersion: string;
  isCompatible: boolean;
  compatibilityError?: string | null;
}

export interface RegistryAuthor {
  slug?: string | null;
  display_name: string;
  url?: string | null;
  avatar_url?: string | null;
  is_official: boolean;
}

export interface PluginVersionEntry {
  version: string;
  download_url: string;
  sha256: string;
  ed25519_signature?: string | null;
  file_size_bytes?: number | null;
  release_notes?: string | null;
  published_at?: string | null;
  compatible_app_version?: string | null;
  is_compatible?: boolean;
  compatibility_message?: string | null;
}

export interface ManifestCredentialField {
  key: string;
  label: string;
  secret: boolean;
  required: boolean;
  kind?: string | null;
  placeholder?: string | null;
  help_text?: string | null;
}

export interface ManifestCapabilities {
  max_characters?: number | null;
  max_attachments?: number | null;
  supported_media_types: string[];
  supports_alt_text: boolean;
  supports_threads: boolean;
  supports_delete: boolean;
  allowed_domains: string[];
  credential_fields: ManifestCredentialField[];
}

export interface PluginManifestDetail {
  id: string;
  name: string;
  description: string | null;
  author: RegistryAuthor | null;
  isOfficial?: boolean;
  brandColour: string;
  iconSvg: string | null;
  setupInstructions: string | null;
  appVersion: string;
  compatibleAppVersion: string;
  isCompatible: boolean;
  compatibilityError?: string | null;
  latestVersion: string;
  installedVersion: string | null;
  isInstalled: boolean;
  updateAvailable: boolean;
  enabled: boolean;
  verified: boolean;
  allowedDomains: string[];
  capabilities: ManifestCapabilities;
  versions: PluginVersionEntry[];
  hubUrl: string;
}

export async function getPluginManifest(
  pluginId: string,
): Promise<PluginManifestDetail> {
  return invoke<PluginManifestDetail>("get_plugin_manifest", { pluginId });
}

export interface InstalledPluginRecord {
  id: string;
  name: string;
  version: string;
  wasmPath: string;
  enabled: boolean;
  verified: boolean;
  isOfficial?: boolean;
  source: string;
  sha256: string;
  installedAt: string;
  allowedDomains: string[];
  brandColour: string | null;
  iconSvg: string | null;
}

export async function getRegistryPlugins(): Promise<RegistryPluginView[]> {
  return invoke<RegistryPluginView[]>("get_registry_plugins");
}

export async function installRegistryPlugin(
  pluginId: string,
  version?: string | null,
): Promise<InstalledPluginRecord> {
  return invoke<InstalledPluginRecord>("install_registry_plugin", {
    pluginId,
    version: version ?? null,
  });
}

export async function installLocalPlugin(
  filePath: string,
): Promise<InstalledPluginRecord> {
  return invoke<InstalledPluginRecord>("install_local_plugin", { filePath });
}

export async function setPluginEnabled(
  pluginId: string,
  enabled: boolean,
): Promise<void> {
  return invoke<void>("set_plugin_enabled", { pluginId, enabled });
}

export async function uninstallPlugin(pluginId: string): Promise<void> {
  return invoke<void>("uninstall_plugin", { pluginId });
}
