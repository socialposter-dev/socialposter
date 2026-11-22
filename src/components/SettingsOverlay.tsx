import { useEffect, useMemo, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  AlertTriangle,
  BadgeCheck,
  Check,
  Code2,
  Download,
  ExternalLink,
  FolderOpen,
  Info,
  KeyRound,
  Loader2,
  Lock,
  Pencil,
  Plus,
  RefreshCw,
  RotateCw,
  Settings,
  Shield,
  Trash2,
  Unlink,
  Upload,
  User,
  X,
} from "lucide-react";
import { toast } from "sonner";
import { AppLogo } from "@/components/AppLogo";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import {
  formatShortcutDisplay,
  keyEventToTauriShortcut,
} from "@/lib/shortcuts";
import {
  clearMediaCache,
  openDataFolder,
  updateDesktopSettings,
  type AuthorLink,
  type DesktopSettings,
  type PlatformMetadata,
  type ProfileInfo,
} from "@/lib/tauri";
import {
  checkForUpdateManual,
  downloadAndInstallUpdate,
  relaunchApp,
  type Update,
  type UpdateStatus,
} from "@/lib/updater";

export type SettingsSection =
  | "general"
  | "profiles"
  | "developer"
  | "about"
  | "reset";

interface SettingsOverlayProps {
  open: boolean;
  initialSection?: SettingsSection;
  suppressEscape?: boolean;
  availableUpdate?: Update | null;
  onClose: () => void;
  desktopSettings: DesktopSettings;
  onDesktopSettingsChange: (next: DesktopSettings) => void;
  profiles: ProfileInfo[];
  activeProfile: ProfileInfo | null;
  platforms: PlatformMetadata[];
  onSelectProfile: (profileId: string) => void;
  onOpenAddProfile: () => void;
  onLockProfile: (profileId: string) => void;
  onRenameProfile: (profileId: string, name: string) => Promise<void>;
  onDeleteProfile: (profileId: string) => Promise<void>;
  onSetProfilePassword: (
    profileId: string,
    currentPassword: string | null,
    newPassword: string | null,
  ) => Promise<void>;
  onOpenBackup: (mode: "export" | "import") => void;
  onClearHistoryAndDrafts: () => Promise<void>;
  onUnlinkAllAccounts: () => Promise<void>;
  onFactoryReset: () => Promise<void>;
}

interface ContributorEntry {
  name: string;
  links: AuthorLink[];
  platforms: { name: string; isOfficial: boolean }[];
  isOfficial: boolean;
}

const SECTION_TITLES: Record<SettingsSection, string> = {
  general: "General",
  profiles: "Profiles & Backups",
  developer: "Developer",
  about: "About & Credits",
  reset: "Reset & Data",
};

function formatLastExported(iso: string | null | undefined): string {
  if (!iso) return "Never";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  });
}

export function SettingsOverlay({
  open,
  initialSection = "general",
  suppressEscape = false,
  availableUpdate = null,
  onClose,
  desktopSettings,
  onDesktopSettingsChange,
  profiles,
  activeProfile,
  platforms,
  onSelectProfile,
  onOpenAddProfile,
  onLockProfile,
  onRenameProfile,
  onDeleteProfile,
  onSetProfilePassword,
  onOpenBackup,
  onClearHistoryAndDrafts,
  onUnlinkAllAccounts,
  onFactoryReset,
}: SettingsOverlayProps) {
  const [section, setSection] = useState<SettingsSection>(initialSection);
  const [isRecordingHotkey, setIsRecordingHotkey] = useState(false);
  const [savingDesktop, setSavingDesktop] = useState(false);

  // Profile management state
  const [renameValue, setRenameValue] = useState(activeProfile?.name ?? "");
  const [isRenaming, setIsRenaming] = useState(false);
  const [confirmDeleteProfile, setConfirmDeleteProfile] = useState(false);
  const [isDeletingProfile, setIsDeletingProfile] = useState(false);

  // Profile password form state
  const [currentPassword, setCurrentPassword] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [passwordError, setPasswordError] = useState<string | null>(null);
  const [savingPassword, setSavingPassword] = useState(false);

  // About & Updates state
  const [appVersion, setAppVersion] = useState("1.1.0");
  const [updateStatus, setUpdateStatus] = useState<UpdateStatus>(
    availableUpdate ? "available" : "idle",
  );
  const [foundUpdate, setFoundUpdate] = useState<Update | null>(
    availableUpdate,
  );
  const [downloadPercent, setDownloadPercent] = useState<number | null>(null);

  // Danger zone confirmations
  const [confirmAction, setConfirmAction] = useState<
    "media" | "history" | "unlink" | "factory" | null
  >(null);
  const [busyAction, setBusyAction] = useState<
    "media" | "history" | "unlink" | "factory" | null
  >(null);

  useEffect(() => {
    if (open) {
      setSection(initialSection);
    }
  }, [open, initialSection]);

  useEffect(() => {
    if (availableUpdate && updateStatus === "idle") {
      setFoundUpdate(availableUpdate);
      setUpdateStatus("available");
    }
  }, [availableUpdate, updateStatus]);

  useEffect(() => {
    setRenameValue(activeProfile?.name ?? "");
    setConfirmDeleteProfile(false);
  }, [activeProfile]);

  useEffect(() => {
    if (!confirmDeleteProfile) return;
    const timer = window.setTimeout(() => setConfirmDeleteProfile(false), 4000);
    return () => window.clearTimeout(timer);
  }, [confirmDeleteProfile]);

  useEffect(() => {
    if (!open) {
      setIsRecordingHotkey(false);
      setConfirmAction(null);
      setConfirmDeleteProfile(false);
      setPasswordError(null);
      return;
    }
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !isRecordingHotkey && !suppressEscape) {
        onClose();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [open, isRecordingHotkey, suppressEscape, onClose]);

  useEffect(() => {
    if (!confirmAction) return;
    const timer = window.setTimeout(() => setConfirmAction(null), 4000);
    return () => window.clearTimeout(timer);
  }, [confirmAction]);

  useEffect(() => {
    let active = true;
    getVersion()
      .then((ver) => {
        if (active && ver) setAppVersion(ver);
      })
      .catch(() => {
        // Ignore outside Tauri shell.
      });
    return () => {
      active = false;
    };
  }, []);

  const contributors = useMemo<ContributorEntry[]>(() => {
    const map = new Map<string, ContributorEntry>();
    for (const platform of platforms) {
      for (const author of platform.authors) {
        const key = author.name.trim().toLowerCase();
        if (!key) continue;
        const existing = map.get(key);
        const platformIsOfficial = Boolean(platform.isOfficial);
        if (existing) {
          if (!existing.platforms.some((p) => p.name === platform.name)) {
            existing.platforms.push({
              name: platform.name,
              isOfficial: platformIsOfficial,
            });
          }
          if (platformIsOfficial) existing.isOfficial = true;
          for (const link of author.links) {
            if (!existing.links.some((l) => l.url === link.url)) {
              existing.links.push(link);
            }
          }
        } else {
          map.set(key, {
            name: author.name,
            links: [...author.links],
            platforms: [
              { name: platform.name, isOfficial: platformIsOfficial },
            ],
            isOfficial: platformIsOfficial,
          });
        }
      }
    }
    return Array.from(map.values()).sort((a, b) =>
      a.name.localeCompare(b.name),
    );
  }, [platforms]);

  if (!open) return null;

  const persistDesktopSetting = async (patch: Partial<DesktopSettings>) => {
    setSavingDesktop(true);
    try {
      const updated = await updateDesktopSettings({
        ...desktopSettings,
        ...patch,
      });
      onDesktopSettingsChange(updated);
    } catch (err) {
      toast.error(`Failed to update settings: ${String(err)}`);
    } finally {
      setSavingDesktop(false);
    }
  };

  const handleShortcutKeyDown = async (e: React.KeyboardEvent) => {
    if (!isRecordingHotkey) return;
    e.preventDefault();
    e.stopPropagation();

    if (e.key === "Escape") {
      setIsRecordingHotkey(false);
      return;
    }

    const combo = keyEventToTauriShortcut(e);
    if (!combo) return;

    setIsRecordingHotkey(false);
    await persistDesktopSetting({ globalShortcut: combo });
    toast.success(`Quick Compose hotkey set to ${formatShortcutDisplay(combo)}.`);
  };

  const handleRenameSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!activeProfile || !renameValue.trim()) return;
    setIsRenaming(true);
    try {
      await onRenameProfile(activeProfile.id, renameValue.trim());
    } finally {
      setIsRenaming(false);
    }
  };

  const handleDeleteCurrentProfile = async () => {
    if (!activeProfile || profiles.length <= 1) return;
    if (!confirmDeleteProfile) {
      setConfirmDeleteProfile(true);
      return;
    }
    setIsDeletingProfile(true);
    try {
      await onDeleteProfile(activeProfile.id);
      setConfirmDeleteProfile(false);
    } finally {
      setIsDeletingProfile(false);
    }
  };

  const handlePasswordSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!activeProfile) return;
    setSavingPassword(true);
    setPasswordError(null);
    try {
      await onSetProfilePassword(
        activeProfile.id,
        activeProfile.hasPassword ? currentPassword : null,
        newPassword.length > 0 ? newPassword : null,
      );
      setCurrentPassword("");
      setNewPassword("");
      toast.success(
        newPassword.length > 0
          ? "Profile password updated."
          : "Profile password removed.",
      );
    } catch (err) {
      setPasswordError(String(err));
    } finally {
      setSavingPassword(false);
    }
  };

  const handleOpenExternal = async (url: string) => {
    if (!url.startsWith("https://")) return;
    try {
      await openUrl(url);
    } catch (err) {
      toast.error(`Unable to open link: ${String(err)}`);
    }
  };

  const handleUpdateAction = async () => {
    if (updateStatus === "ready") {
      try {
        await relaunchApp();
      } catch (err) {
        toast.error(`Restart failed: ${String(err)}`);
      }
      return;
    }

    if (updateStatus === "available" && foundUpdate) {
      setUpdateStatus("downloading");
      setDownloadPercent(null);
      try {
        await downloadAndInstallUpdate(foundUpdate, (progress) => {
          setDownloadPercent(progress.percent);
        });
        setUpdateStatus("ready");
      } catch (err) {
        setUpdateStatus("available");
        toast.error(`Update failed: ${String(err)}`);
      }
      return;
    }

    setUpdateStatus("checking");
    try {
      const update = await checkForUpdateManual();
      if (update) {
        setFoundUpdate(update);
        setUpdateStatus("available");
      } else {
        setUpdateStatus("up-to-date");
        window.setTimeout(() => setUpdateStatus("idle"), 4000);
      }
    } catch (err) {
      setUpdateStatus("idle");
      toast.error(`Update check failed: ${String(err)}`);
    }
  };

  const runDangerAction = async (
    kind: "media" | "history" | "unlink" | "factory",
    fn: () => Promise<void>,
  ) => {
    if (confirmAction !== kind) {
      setConfirmAction(kind);
      return;
    }
    setBusyAction(kind);
    try {
      await fn();
      setConfirmAction(null);
    } catch (err) {
      toast.error(String(err));
    } finally {
      setBusyAction(null);
    }
  };

  const navGroups: {
    label: string;
    items: { id: SettingsSection; label: string; danger?: boolean }[];
  }[] = [
    {
      label: "WORKSPACE",
      items: [
        { id: "general", label: "General" },
        { id: "profiles", label: "Profiles & Backups" },
      ],
    },
    {
      label: "APP & ADVANCED",
      items: [
        { id: "developer", label: "Developer" },
        { id: "about", label: "About & Credits" },
      ],
    },
    {
      label: "DANGER ZONE",
      items: [{ id: "reset", label: "Reset & Data", danger: true }],
    },
  ];

  const canDeleteProfile = profiles.length > 1 && activeProfile !== null;
  const canLockProfile = Boolean(
    activeProfile && activeProfile.hasPassword && activeProfile.isUnlocked,
  );

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Settings"
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/20 p-4 supports-backdrop-filter:backdrop-blur-xs"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget && !suppressEscape) onClose();
      }}
    >
      <div className="flex h-[640px] max-h-[85vh] w-full max-w-4xl overflow-hidden rounded-2xl bg-popover text-popover-foreground shadow-2xl ring-1 ring-foreground/10">
        {/* Left navigation rail */}
        <aside className="flex w-52 shrink-0 flex-col justify-between border-r border-border/60 bg-muted/20 p-3">
          <div className="space-y-5">
            <div className="flex items-center gap-2 px-2 pt-1">
              <Settings className="size-4 text-primary" />
              <span className="text-sm font-semibold">Settings</span>
            </div>

            <nav className="space-y-4">
              {navGroups.map((group) => (
                <div key={group.label} className="space-y-1">
                  <p className="px-2 text-[10px] font-semibold tracking-wider text-muted-foreground uppercase">
                    {group.label}
                  </p>
                  <div className="space-y-0.5">
                    {group.items.map((item) => {
                      const active = section === item.id;
                      return (
                        <button
                          key={item.id}
                          type="button"
                          onClick={() => setSection(item.id)}
                          className={`flex w-full cursor-pointer items-center justify-between rounded-md px-2.5 py-1.5 text-left text-xs font-medium transition-colors ${
                            active
                              ? item.danger
                                ? "bg-destructive/15 text-destructive shadow-2xs"
                                : "bg-background text-foreground shadow-2xs"
                              : item.danger
                                ? "text-destructive/80 hover:bg-destructive/10 hover:text-destructive"
                                : "text-muted-foreground hover:bg-muted/40 hover:text-foreground"
                          }`}
                        >
                          <span>{item.label}</span>
                          {item.id === "about" && availableUpdate && (
                            <span className="size-2 rounded-full bg-primary" />
                          )}
                        </button>
                      );
                    })}
                  </div>
                </div>
              ))}
            </nav>
          </div>

          <p className="px-2 text-[11px] text-muted-foreground">
            SocialPoster v{appVersion}
          </p>
        </aside>

        {/* Right panel */}
        <div className="flex min-w-0 flex-1 flex-col">
          <header className="flex shrink-0 items-center justify-between border-b border-border/60 px-6 py-4">
            <h2
              className={`text-base font-semibold ${
                section === "reset" ? "text-destructive" : "text-foreground"
              }`}
            >
              {SECTION_TITLES[section]}
            </h2>
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              onClick={onClose}
              aria-label="Close settings"
              title="Close (Esc)"
            >
              <X className="size-4" />
            </Button>
          </header>

          <div className="min-h-0 flex-1 overflow-y-auto p-6">
            <div className="mx-auto max-w-2xl space-y-6">
              {section === "general" && (
                <>
                  <p className="text-xs text-muted-foreground">
                    Customise startup behaviour, system tray minimising, update
                    checks, and your global Quick Compose shortcut.
                  </p>

                  <div className="divide-y divide-border/60 rounded-xl border border-border/60 bg-card px-5">
                    <div className="flex items-center justify-between gap-6 py-4">
                      <div className="space-y-0.5">
                        <p className="text-sm font-medium">Launch at Startup</p>
                        <p className="text-xs text-muted-foreground">
                          Automatically start SocialPoster when you sign in to
                          your computer.
                        </p>
                      </div>
                      <Switch
                        checked={desktopSettings.launchAtStartup}
                        disabled={savingDesktop}
                        onCheckedChange={(checked) =>
                          void persistDesktopSetting({
                            launchAtStartup: checked,
                          })
                        }
                      />
                    </div>

                    <div className="flex items-center justify-between gap-6 py-4">
                      <div className="space-y-0.5">
                        <p className="text-sm font-medium">
                          Minimise to System Tray
                        </p>
                        <p className="text-xs text-muted-foreground">
                          Keep SocialPoster running in the tray when you close
                          the main window.
                        </p>
                      </div>
                      <Switch
                        checked={desktopSettings.minimizeToTray}
                        disabled={savingDesktop}
                        onCheckedChange={(checked) =>
                          void persistDesktopSetting({
                            minimizeToTray: checked,
                          })
                        }
                      />
                    </div>

                    <div className="flex items-center justify-between gap-6 py-4">
                      <div className="space-y-0.5">
                        <p className="text-sm font-medium">
                          Check for Updates on Startup
                        </p>
                        <p className="text-xs text-muted-foreground">
                          Automatically check GitHub Releases for new versions
                          when SocialPoster launches.
                        </p>
                      </div>
                      <Switch
                        checked={desktopSettings.checkUpdatesOnStartup}
                        disabled={savingDesktop}
                        onCheckedChange={(checked) =>
                          void persistDesktopSetting({
                            checkUpdatesOnStartup: checked,
                          })
                        }
                      />
                    </div>

                    <div className="flex items-center justify-between gap-6 py-4">
                      <div className="space-y-0.5">
                        <p className="text-sm font-medium">
                          Quick Compose Hotkey
                        </p>
                        <p className="text-xs text-muted-foreground">
                          Toggle the composer window from anywhere on your
                          desktop.
                        </p>
                      </div>
                      <div className="flex shrink-0 items-center gap-2">
                        <button
                          type="button"
                          disabled={savingDesktop}
                          onClick={() => setIsRecordingHotkey((prev) => !prev)}
                          onKeyDown={(e) => void handleShortcutKeyDown(e)}
                          className={`h-8 min-w-36 cursor-pointer rounded-md border px-3 text-xs font-medium transition-colors ${
                            isRecordingHotkey
                              ? "border-primary bg-primary/10 text-primary"
                              : "border-border bg-background text-foreground hover:bg-muted/40"
                          }`}
                        >
                          {isRecordingHotkey
                            ? "Press shortcut..."
                            : formatShortcutDisplay(
                                desktopSettings.globalShortcut,
                              )}
                        </button>
                        {desktopSettings.globalShortcut && (
                          <Button
                            type="button"
                            variant="ghost"
                            size="sm"
                            className="h-8 px-2 text-xs text-muted-foreground"
                            disabled={savingDesktop}
                            onClick={() =>
                              void persistDesktopSetting({
                                globalShortcut: null,
                              })
                            }
                          >
                            Clear
                          </Button>
                        )}
                      </div>
                    </div>
                  </div>

                  <div className="space-y-2.5">
                    <h3 className="text-xs font-semibold tracking-wider text-muted-foreground uppercase">
                      Publishing &amp; Media
                    </h3>
                    <div className="divide-y divide-border/60 rounded-xl border border-border/60 bg-card px-5">
                      <div className="flex items-center justify-between gap-6 py-4">
                        <div className="space-y-0.5">
                          <p className="text-sm font-medium">
                            Auto-Split Long Posts into Threads
                          </p>
                          <p className="text-xs text-muted-foreground">
                            Automatically split long posts at word boundaries
                            into chained reply threads on supported platforms.
                          </p>
                        </div>
                        <Switch
                          checked={desktopSettings.autoThreadPosts}
                          disabled={savingDesktop}
                          onCheckedChange={(checked) =>
                            void persistDesktopSetting({
                              autoThreadPosts: checked,
                            })
                          }
                        />
                      </div>

                      <div className="flex items-center justify-between gap-6 py-4">
                        <div className="space-y-0.5">
                          <p className="text-sm font-medium">
                            Auto-Compress Oversized Images
                          </p>
                          <p className="text-xs text-muted-foreground">
                            Resize and compress oversized images to fit each
                            platform&apos;s file size limit.
                          </p>
                        </div>
                        <Switch
                          checked={desktopSettings.autoCompressImages}
                          disabled={savingDesktop}
                          onCheckedChange={(checked) =>
                            void persistDesktopSetting({
                              autoCompressImages: checked,
                            })
                          }
                        />
                      </div>
                    </div>
                  </div>
                </>
              )}

              {section === "profiles" && (
                <div className="space-y-5">
                  {/* Profile Management Card */}
                  <div className="space-y-4 rounded-xl border border-border/60 bg-card p-5">
                    <div className="flex flex-wrap items-center justify-between gap-3">
                      <div className="space-y-0.5">
                        <h3 className="flex items-center gap-2 text-sm font-medium">
                          <User className="size-4 text-primary" />
                          <span>Profiles</span>
                        </h3>
                        <p className="text-xs text-muted-foreground">
                          Switch between isolated workspaces, create new
                          profiles, or rename and lock the active profile.
                        </p>
                      </div>
                      <div className="flex items-center gap-2">
                        {canLockProfile && activeProfile && (
                          <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={() => onLockProfile(activeProfile.id)}
                          >
                            <Lock className="mr-1.5 size-3.5" />
                            Lock Profile
                          </Button>
                        )}
                        <Button
                          type="button"
                          variant="secondary"
                          size="sm"
                          onClick={onOpenAddProfile}
                        >
                          <Plus className="mr-1.5 size-3.5" />
                          Add Profile
                        </Button>
                      </div>
                    </div>

                    <div className="flex flex-wrap gap-1.5">
                      {profiles.map((profile) => {
                        const isActive = profile.id === activeProfile?.id;
                        return (
                          <button
                            key={profile.id}
                            type="button"
                            onClick={() => onSelectProfile(profile.id)}
                            className={`inline-flex cursor-pointer items-center gap-1.5 rounded-lg border px-2.5 py-1.5 text-xs font-medium transition-colors ${
                              isActive
                                ? "border-primary/50 bg-primary/15 text-foreground"
                                : "border-border/60 bg-muted/30 text-muted-foreground hover:text-foreground"
                            }`}
                          >
                            {isActive && (
                              <Check className="size-3 text-primary" />
                            )}
                            <span>{profile.name}</span>
                            {profile.hasPassword && (
                              <Lock className="size-3 text-muted-foreground" />
                            )}
                          </button>
                        );
                      })}
                    </div>

                    {activeProfile && (
                      <form
                        onSubmit={(e) => void handleRenameSubmit(e)}
                        className="flex flex-wrap items-end justify-between gap-3 border-t border-border/60 pt-4"
                      >
                        <div className="min-w-48 flex-1 space-y-1.5">
                          <Label
                            htmlFor="settings-rename-profile"
                            className="text-xs"
                          >
                            Active Profile Name
                          </Label>
                          <div className="flex items-center gap-2">
                            <Input
                              id="settings-rename-profile"
                              value={renameValue}
                              onChange={(e) => setRenameValue(e.target.value)}
                              disabled={isRenaming}
                              placeholder="Profile name"
                            />
                            <Button
                              type="submit"
                              variant="outline"
                              size="sm"
                              disabled={
                                isRenaming ||
                                !renameValue.trim() ||
                                renameValue.trim() === activeProfile.name
                              }
                            >
                              {isRenaming ? (
                                <Loader2 className="mr-1.5 size-3.5 animate-spin" />
                              ) : (
                                <Pencil className="mr-1.5 size-3.5" />
                              )}
                              Rename
                            </Button>
                          </div>
                        </div>

                        {canDeleteProfile && (
                          <Button
                            type="button"
                            variant={
                              confirmDeleteProfile ? "destructive" : "outline"
                            }
                            size="sm"
                            disabled={isDeletingProfile}
                            onClick={() => void handleDeleteCurrentProfile()}
                          >
                            {isDeletingProfile ? (
                              <Loader2 className="mr-1.5 size-3.5 animate-spin" />
                            ) : (
                              <Trash2 className="mr-1.5 size-3.5" />
                            )}
                            {confirmDeleteProfile
                              ? "Confirm Delete Profile"
                              : "Delete Profile"}
                          </Button>
                        )}
                      </form>
                    )}
                  </div>

                  {/* Password Lock Card */}
                  <form
                    onSubmit={(e) => void handlePasswordSubmit(e)}
                    className="space-y-4 rounded-xl border border-border/60 bg-card p-5"
                  >
                    <div className="flex items-start justify-between gap-4">
                      <div className="space-y-0.5">
                        <h3 className="flex items-center gap-2 text-sm font-medium">
                          <KeyRound className="size-4 text-primary" />
                          <span>Profile Password Lock</span>
                        </h3>
                        <p className="text-xs text-muted-foreground">
                          {activeProfile?.hasPassword
                            ? "Enter your current password and a new password, or leave the new password blank to remove the lock."
                            : "Add a local password to lock this profile when not in use."}
                        </p>
                      </div>
                      <Badge variant="outline" className="text-[11px]">
                        {activeProfile?.hasPassword ? "Protected" : "Unlocked"}
                      </Badge>
                    </div>

                    <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                      {activeProfile?.hasPassword && (
                        <div className="space-y-1.5">
                          <Label
                            htmlFor="settings-current-pw"
                            className="text-xs"
                          >
                            Current Password
                          </Label>
                          <Input
                            id="settings-current-pw"
                            type="password"
                            value={currentPassword}
                            onChange={(e) => setCurrentPassword(e.target.value)}
                            disabled={
                              savingPassword || !activeProfile?.isUnlocked
                            }
                            placeholder="Enter current password"
                          />
                        </div>
                      )}
                      <div className="space-y-1.5">
                        <Label htmlFor="settings-new-pw" className="text-xs">
                          New Password
                        </Label>
                        <Input
                          id="settings-new-pw"
                          type="password"
                          value={newPassword}
                          onChange={(e) => setNewPassword(e.target.value)}
                          disabled={
                            savingPassword || !activeProfile?.isUnlocked
                          }
                          placeholder={
                            activeProfile?.hasPassword
                              ? "Leave blank to remove"
                              : "Enter new password"
                          }
                        />
                      </div>
                    </div>

                    {passwordError && (
                      <p className="text-xs text-destructive">
                        {passwordError}
                      </p>
                    )}

                    <div className="flex justify-end">
                      <Button
                        type="submit"
                        variant="secondary"
                        size="sm"
                        disabled={
                          savingPassword ||
                          !activeProfile ||
                          !activeProfile.isUnlocked ||
                          (!activeProfile.hasPassword && !newPassword)
                        }
                      >
                        {savingPassword && (
                          <Loader2 className="mr-1.5 size-3.5 animate-spin" />
                        )}
                        {activeProfile?.hasPassword
                          ? newPassword.length > 0
                            ? "Update Password"
                            : "Remove Password"
                          : "Set Password"}
                      </Button>
                    </div>
                  </form>

                  {/* Encrypted Backups Card */}
                  <div className="divide-y divide-border/60 rounded-xl border border-border/60 bg-card px-5">
                    <div className="flex items-center justify-between gap-6 py-4">
                      <div className="space-y-0.5">
                        <p className="text-sm font-medium">
                          Export Backup (.spbackup)
                        </p>
                        <p className="text-xs text-muted-foreground">
                          Create a passphrase-encrypted archive of this
                          profile&apos;s credentials, history, and required
                          plugins.
                        </p>
                        <p className="text-[11px] text-muted-foreground/80">
                          Last exported:{" "}
                          {formatLastExported(activeProfile?.lastBackupAt)}
                        </p>
                      </div>
                      <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        disabled={!activeProfile || !activeProfile.isUnlocked}
                        onClick={() => onOpenBackup("export")}
                      >
                        <Download className="mr-1.5 size-3.5" />
                        Export Backup (.spbackup)
                      </Button>
                    </div>

                    <div className="flex items-center justify-between gap-6 py-4">
                      <div className="space-y-0.5">
                        <p className="text-sm font-medium">
                          Import Backup (.spbackup)
                        </p>
                        <p className="text-xs text-muted-foreground">
                          Restore a profile and its saved credentials from an
                          encrypted backup archive.
                        </p>
                      </div>
                      <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        onClick={() => onOpenBackup("import")}
                      >
                        <Upload className="mr-1.5 size-3.5" />
                        Import Backup (.spbackup)
                      </Button>
                    </div>
                  </div>
                </div>
              )}

              {section === "developer" && (
                <>
                  <p className="text-xs text-muted-foreground">
                    Tools for plugin authors and inspecting local application
                    data.
                  </p>

                  <div className="divide-y divide-border/60 rounded-xl border border-border/60 bg-card px-5">
                    <div className="flex items-center justify-between gap-6 py-4">
                      <div className="space-y-0.5">
                        <p className="flex items-center gap-2 text-sm font-medium">
                          <Code2 className="size-4 text-primary" />
                          <span>Developer Mode</span>
                        </p>
                        <p className="text-xs text-muted-foreground">
                          Show Sideload .wasm in Plugins and display plugin IDs
                          and allowed domains on installed plugin cards.
                        </p>
                      </div>
                      <Switch
                        checked={desktopSettings.developerMode}
                        disabled={savingDesktop}
                        onCheckedChange={(checked) =>
                          void persistDesktopSetting({ developerMode: checked })
                        }
                      />
                    </div>

                    <div className="flex items-center justify-between gap-6 py-4">
                      <div className="space-y-0.5">
                        <p className="text-sm font-medium">Open Data Folder</p>
                        <p className="text-xs text-muted-foreground">
                          Open SocialPoster&apos;s local application data and
                          installed plugins directory.
                        </p>
                      </div>
                      <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        onClick={() => {
                          void openDataFolder().catch((err) =>
                            toast.error(String(err)),
                          );
                        }}
                      >
                        <FolderOpen className="mr-1.5 size-3.5" />
                        Open Folder
                      </Button>
                    </div>
                  </div>
                </>
              )}

              {section === "about" && (
                <div className="space-y-5">
                  <div className="space-y-4 rounded-xl border border-border/60 bg-card p-5">
                    <div className="flex items-center gap-3">
                      <AppLogo className="size-10 shrink-0" />
                      <div className="space-y-0.5">
                        <div className="flex items-center gap-2">
                          <span className="text-base font-semibold">
                            SocialPoster
                          </span>
                          <Badge variant="secondary" className="text-xs">
                            v{appVersion}
                          </Badge>
                        </div>
                        <p className="text-xs text-muted-foreground">
                          Cross-platform desktop social publisher powered by
                          Community plugins.
                        </p>
                      </div>
                    </div>

                    <div className="flex min-h-[40px] flex-wrap items-center gap-3 border-t border-border/60 pt-3">
                      <Button
                        type="button"
                        variant={
                          updateStatus === "available" ||
                          updateStatus === "ready"
                            ? "default"
                            : "outline"
                        }
                        size="sm"
                        disabled={
                          updateStatus === "checking" ||
                          updateStatus === "downloading"
                        }
                        onClick={() => void handleUpdateAction()}
                      >
                        {updateStatus === "checking" ? (
                          <>
                            <Loader2 className="mr-1.5 size-3.5 animate-spin" />
                            Check for Updates
                          </>
                        ) : updateStatus === "available" && foundUpdate ? (
                          <>
                            <Download className="mr-1.5 size-3.5" />
                            Install v{foundUpdate.version}
                          </>
                        ) : updateStatus === "downloading" ? (
                          <>
                            <Loader2 className="mr-1.5 size-3.5 animate-spin" />
                            {downloadPercent !== null
                              ? `Downloading ${downloadPercent}%`
                              : "Downloading..."}
                          </>
                        ) : updateStatus === "ready" ? (
                          <>
                            <RotateCw className="mr-1.5 size-3.5" />
                            Restart to update
                          </>
                        ) : (
                          <>
                            <RefreshCw className="mr-1.5 size-3.5" />
                            Check for Updates
                          </>
                        )}
                      </Button>

                      <span className="text-xs text-muted-foreground">
                        {updateStatus === "checking" && "Checking..."}
                        {updateStatus === "up-to-date" && (
                          <span className="inline-flex items-center gap-1 text-emerald-400">
                            <Check className="size-3.5" />
                            You are on the latest version
                          </span>
                        )}
                        {updateStatus === "available" && foundUpdate && (
                          <span className="text-primary">
                            Update v{foundUpdate.version} available
                          </span>
                        )}
                        {updateStatus === "ready" && (
                          <span className="text-emerald-400">
                            Update ready — restart to apply
                          </span>
                        )}
                      </span>
                    </div>
                  </div>

                  <div className="flex flex-wrap gap-2">
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      onClick={() =>
                        void handleOpenExternal("https://socialposter.dev")
                      }
                    >
                      <Info className="mr-1.5 size-3.5" />
                      socialposter.dev
                      <ExternalLink className="ml-1.5 size-3" />
                    </Button>
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      onClick={() =>
                        void handleOpenExternal(
                          "https://github.com/socialposter-dev/socialposter/releases",
                        )
                      }
                    >
                      GitHub Releases
                      <ExternalLink className="ml-1.5 size-3" />
                    </Button>
                  </div>

                  <div className="space-y-2.5">
                    <h3 className="text-xs font-semibold tracking-wider text-muted-foreground uppercase">
                      Installed Plugin Attributions
                    </h3>
                    {contributors.length === 0 ? (
                      <p className="rounded-xl border border-border/60 bg-card p-4 text-xs text-muted-foreground">
                        No plugin author attributions loaded.
                      </p>
                    ) : (
                      <div className="space-y-2">
                        {contributors.map((entry) => (
                          <div
                            key={entry.name}
                            className="flex flex-wrap items-center justify-between gap-2 rounded-xl border border-border/60 bg-card px-4 py-3 text-xs"
                          >
                            <div className="space-y-1">
                              <div className="flex flex-wrap items-center gap-1.5">
                                <p className="font-medium text-foreground">
                                  {entry.name}
                                </p>
                                {entry.isOfficial && (
                                  <Badge
                                    variant="outline"
                                    className="gap-1 border-indigo-500/30 bg-indigo-500/10 text-[10px] font-medium text-indigo-600 dark:text-indigo-300"
                                  >
                                    <BadgeCheck className="size-2.5" />
                                    Official
                                  </Badge>
                                )}
                              </div>
                              <p className="text-muted-foreground">
                                {entry.platforms.map((p) => p.name).join(", ")}
                              </p>
                            </div>
                            <div className="flex flex-wrap gap-1.5">
                              {entry.links
                                .filter((l) => l.url.startsWith("https://"))
                                .map((link) => (
                                  <Button
                                    key={link.url}
                                    type="button"
                                    variant="outline"
                                    size="sm"
                                    className="h-7 px-2.5 text-xs"
                                    onClick={() =>
                                      void handleOpenExternal(link.url)
                                    }
                                  >
                                    {link.label}
                                    <ExternalLink className="ml-1 size-3" />
                                  </Button>
                                ))}
                            </div>
                          </div>
                        ))}
                      </div>
                    )}
                  </div>
                </div>
              )}

              {section === "reset" && (
                <>
                  <p className="flex items-center gap-2 text-xs text-muted-foreground">
                    <Shield className="size-3.5 text-destructive" />
                    <span>
                      Clear cached media, profile history, saved credentials, or
                      perform a full factory reset.
                    </span>
                  </p>

                  <div className="divide-y divide-border/60 rounded-xl border border-border/60 bg-card px-5">
                    <div className="flex items-center justify-between gap-6 py-4">
                      <div className="space-y-0.5">
                        <p className="text-sm font-medium">Clear Media Cache</p>
                        <p className="text-xs text-muted-foreground">
                          Delete temporary compressed and cached media files
                          without affecting posts or credentials.
                        </p>
                      </div>
                      <Button
                        type="button"
                        variant={
                          confirmAction === "media" ? "destructive" : "outline"
                        }
                        size="sm"
                        disabled={busyAction !== null}
                        onClick={() =>
                          void runDangerAction("media", async () => {
                            await clearMediaCache();
                            toast.success("Media cache cleared.");
                          })
                        }
                      >
                        {busyAction === "media" && (
                          <Loader2 className="mr-1.5 size-3.5 animate-spin" />
                        )}
                        {confirmAction === "media"
                          ? "Confirm Clear Cache"
                          : "Clear Cache"}
                      </Button>
                    </div>

                    <div className="flex items-center justify-between gap-6 py-4">
                      <div className="space-y-0.5">
                        <p className="text-sm font-medium">
                          Clear Post History &amp; Drafts
                        </p>
                        <p className="text-xs text-muted-foreground">
                          Delete sent post history and saved drafts for{" "}
                          <span className="font-medium text-foreground">
                            {activeProfile?.name ?? "this profile"}
                          </span>{" "}
                          only.
                        </p>
                      </div>
                      <Button
                        type="button"
                        variant={
                          confirmAction === "history"
                            ? "destructive"
                            : "outline"
                        }
                        size="sm"
                        disabled={
                          busyAction !== null ||
                          !activeProfile ||
                          !activeProfile.isUnlocked
                        }
                        onClick={() =>
                          void runDangerAction(
                            "history",
                            onClearHistoryAndDrafts,
                          )
                        }
                      >
                        {busyAction === "history" ? (
                          <Loader2 className="mr-1.5 size-3.5 animate-spin" />
                        ) : (
                          <Trash2 className="mr-1.5 size-3.5" />
                        )}
                        {confirmAction === "history"
                          ? "Confirm Clear History"
                          : "Clear History & Drafts"}
                      </Button>
                    </div>

                    <div className="flex items-center justify-between gap-6 py-4">
                      <div className="space-y-0.5">
                        <p className="text-sm font-medium">
                          Unlink All Accounts
                        </p>
                        <p className="text-xs text-muted-foreground">
                          Remove all saved platform credentials for this profile
                          from the OS keyring while keeping plugins installed.
                        </p>
                      </div>
                      <Button
                        type="button"
                        variant={
                          confirmAction === "unlink" ? "destructive" : "outline"
                        }
                        size="sm"
                        disabled={
                          busyAction !== null ||
                          !activeProfile ||
                          !activeProfile.isUnlocked
                        }
                        onClick={() =>
                          void runDangerAction("unlink", onUnlinkAllAccounts)
                        }
                      >
                        {busyAction === "unlink" ? (
                          <Loader2 className="mr-1.5 size-3.5 animate-spin" />
                        ) : (
                          <Unlink className="mr-1.5 size-3.5" />
                        )}
                        {confirmAction === "unlink"
                          ? "Confirm Unlink All"
                          : "Unlink All"}
                      </Button>
                    </div>

                    <div className="flex items-center justify-between gap-6 py-4">
                      <div className="space-y-0.5">
                        <p className="flex items-center gap-1.5 text-sm font-medium text-destructive">
                          <AlertTriangle className="size-4" />
                          <span>Factory Reset</span>
                        </p>
                        <p className="text-xs text-muted-foreground">
                          Delete all profiles, local data, installed .wasm
                          plugins, and all SocialPoster OS keyring entries, then
                          reinitialise a clean default profile.
                        </p>
                      </div>
                      <Button
                        type="button"
                        variant="destructive"
                        size="sm"
                        disabled={busyAction !== null}
                        onClick={() =>
                          void runDangerAction("factory", onFactoryReset)
                        }
                      >
                        {busyAction === "factory" && (
                          <Loader2 className="mr-1.5 size-3.5 animate-spin" />
                        )}
                        {confirmAction === "factory"
                          ? "Confirm Factory Reset"
                          : "Factory Reset"}
                      </Button>
                    </div>
                  </div>
                </>
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
