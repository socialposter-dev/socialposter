import { useEffect, useState } from "react";
import { open as openFileDialog, save as saveFileDialog } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  BadgeCheck,
  CheckCircle2,
  Download,
  FileArchive,
  Loader2,
  ShieldCheck,
  Upload,
  X,
} from "lucide-react";
import { toast } from "sonner";
import { PluginIcon } from "@/components/PluginIcon";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { exportProfileStorage, restoreProfileStorage } from "@/lib/storage";
import {
  exportProfileBackup,
  importProfileBackup,
  inspectProfileBackup,
  installRegistryPlugin,
  previewBackupRestore,
  type BackupMetadata,
  type BackupRestorePreview,
  type ImportedProfileResponse,
  type ProfileInfo,
} from "@/lib/tauri";

interface BackupDialogProps {
  mode: "export" | "import" | null;
  activeProfile: ProfileInfo | null;
  onClose: () => void;
  onExported?: () => Promise<void> | void;
  onImported: (result: ImportedProfileResponse) => Promise<void>;
}

function slugifyProfileName(name: string): string {
  const slug = name
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return slug || "profile";
}

function formatExportDate(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  return date.toLocaleString(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  });
}

export function BackupDialog({
  mode,
  activeProfile,
  onClose,
  onExported,
  onImported,
}: BackupDialogProps) {
  const [passphrase, setPassphrase] = useState("");
  const [confirmPassphrase, setConfirmPassphrase] = useState("");
  const [importFilePath, setImportFilePath] = useState<string | null>(null);
  const [metadata, setMetadata] = useState<BackupMetadata | null>(null);
  const [customName, setCustomName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [isBusy, setIsBusy] = useState(false);
  const [restorePreview, setRestorePreview] =
    useState<BackupRestorePreview | null>(null);
  const [installingPluginId, setInstallingPluginId] = useState<string | null>(
    null,
  );
  const [installedPluginIds, setInstalledPluginIds] = useState<Set<string>>(
    () => new Set(),
  );
  const [failedPluginErrors, setFailedPluginErrors] = useState<
    Record<string, string>
  >({});

  useEffect(() => {
    if (mode !== null) {
      setPassphrase("");
      setConfirmPassphrase("");
      setImportFilePath(null);
      setMetadata(null);
      setCustomName("");
      setError(null);
      setIsBusy(false);
      setRestorePreview(null);
      setInstallingPluginId(null);
      setInstalledPluginIds(new Set());
      setFailedPluginErrors({});
    }
  }, [mode]);

  const handleExportSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!activeProfile) return;

    if (passphrase.trim().length < 8) {
      setError("Passphrase must be at least 8 characters.");
      return;
    }
    if (passphrase !== confirmPassphrase) {
      setError("Passphrases do not match.");
      return;
    }

    setError(null);
    setIsBusy(true);
    try {
      const slug = slugifyProfileName(activeProfile.name);
      const selectedPath = await saveFileDialog({
        defaultPath: `${slug}-backup.spbackup`,
        filters: [
          { name: "SocialPoster Backup", extensions: ["spbackup", "json"] },
        ],
      });
      if (!selectedPath) return;

      const frontendState = exportProfileStorage(activeProfile.id);
      await exportProfileBackup(
        activeProfile.id,
        selectedPath,
        passphrase,
        frontendState,
      );
      await onExported?.();
      toast.success("Backup exported.");
      onClose();
    } catch (err) {
      setError(String(err));
    } finally {
      setIsBusy(false);
    }
  };

  const handlePickImportFile = async () => {
    setError(null);
    try {
      const selected = await openFileDialog({
        multiple: false,
        filters: [
          { name: "SocialPoster Backup", extensions: ["spbackup", "json"] },
        ],
      });
      if (!selected || Array.isArray(selected)) return;

      const meta = await inspectProfileBackup(selected);
      setImportFilePath(selected);
      setMetadata(meta);
      setCustomName(meta.profileName);
    } catch (err) {
      setImportFilePath(null);
      setMetadata(null);
      setError(String(err));
    }
  };

  const finishRestore = async () => {
    if (!importFilePath) return;
    const result = await importProfileBackup(
      importFilePath,
      passphrase,
      customName.trim() || null,
    );
    restoreProfileStorage(result.profile.id, result.frontendState);
    await onImported(result);
    toast.success(
      `Restored ${result.restoredPlatformsCount} platforms and ${result.restoredHistoryCount} history entries.`,
    );
    onClose();
  };

  const handleImportSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!importFilePath) {
      setError("Choose a backup file.");
      return;
    }
    if (!passphrase) {
      setError("Passphrase is required.");
      return;
    }

    setError(null);
    setIsBusy(true);
    try {
      const preview = await previewBackupRestore(importFilePath, passphrase);
      const hasMissingPlugins =
        preview.missingAvailable.length > 0 ||
        preview.missingUnavailable.length > 0;
      if (!preview.isOlderVersion && !preview.isNewerVersion && !hasMissingPlugins) {
        await finishRestore();
      } else {
        setRestorePreview(preview);
      }
    } catch (err) {
      setError(String(err));
    } finally {
      setIsBusy(false);
    }
  };

  const handleInstallPluginsAndRestore = async () => {
    if (!restorePreview) return;
    setError(null);
    setIsBusy(true);
    setFailedPluginErrors({});
    try {
      for (const item of restorePreview.missingAvailable) {
        if (installedPluginIds.has(item.pluginId)) continue;
        setInstallingPluginId(item.pluginId);
        try {
          await installRegistryPlugin(item.pluginId, item.targetVersion);
          setInstalledPluginIds((prev) => {
            const next = new Set(prev);
            next.add(item.pluginId);
            return next;
          });
        } catch (err) {
          setFailedPluginErrors((prev) => ({
            ...prev,
            [item.pluginId]: String(err),
          }));
          throw err;
        }
      }
      setInstallingPluginId(null);
      await finishRestore();
    } catch (err) {
      setError(String(err));
      setInstallingPluginId(null);
    } finally {
      setIsBusy(false);
    }
  };

  const handleRestoreWithoutInstalling = async () => {
    setError(null);
    setIsBusy(true);
    try {
      await finishRestore();
    } catch (err) {
      setError(String(err));
    } finally {
      setIsBusy(false);
    }
  };

  return (
    <Dialog
      open={mode !== null}
      onOpenChange={(open) => {
        if (!open && !isBusy) onClose();
      }}
    >
      <DialogContent
        showCloseButton={false}
        className={
          restorePreview
            ? "flex max-h-[85vh] flex-col gap-0 overflow-hidden border-[#27272a] bg-[#111113] p-0 text-foreground sm:max-w-lg"
            : "sm:max-w-md"
        }
      >
        {restorePreview ? (
          <>
            <DialogHeader className="shrink-0 border-b border-[#27272a] p-5">
              <div className="flex items-center justify-between gap-4">
                <DialogTitle className="flex items-center gap-2 text-base">
                  <Download className="size-4 text-primary" />
                  <span>
                    {restorePreview.missingAvailable.length > 0 ||
                    restorePreview.missingUnavailable.length > 0
                      ? "Required Plugins for Restore"
                      : "Backup Version Notice"}
                  </span>
                </DialogTitle>
                <DialogClose asChild>
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon-sm"
                    disabled={isBusy}
                    aria-label="Close"
                  >
                    <X className="size-4" />
                  </Button>
                </DialogClose>
              </div>
            </DialogHeader>

            <div className="min-h-0 flex-1 space-y-4 overflow-y-auto p-5 text-xs">
              {(restorePreview.isOlderVersion || restorePreview.isNewerVersion) &&
                restorePreview.versionWarning && (
                  <div
                    role="alert"
                    className="flex items-start gap-2.5 rounded-lg border border-amber-500/40 bg-amber-500/10 p-3.5 text-xs text-amber-200"
                  >
                    <AlertTriangle className="mt-0.5 size-4 shrink-0 text-amber-400" />
                    <div className="space-y-1">
                      <p className="font-semibold text-amber-300">
                        {restorePreview.isOlderVersion
                          ? `Older Backup (${restorePreview.backupAppVersion ? `v${restorePreview.backupAppVersion}` : "Legacy"} → v${restorePreview.currentAppVersion})`
                          : `Newer Backup (v${restorePreview.backupAppVersion ?? "?"} → v${restorePreview.currentAppVersion})`}
                      </p>
                      <p className="leading-relaxed text-amber-200/90">
                        {restorePreview.versionWarning}
                      </p>
                    </div>
                  </div>
                )}

              {(restorePreview.missingAvailable.length > 0 ||
                restorePreview.missingUnavailable.length > 0) && (
                <p className="text-sm leading-relaxed text-muted-foreground">
                  This backup contains credentials for the following plugins.
                  Install them now so your accounts work immediately.
                </p>
              )}

              <div className="space-y-2.5">
                {restorePreview.missingAvailable.map((item) => {
                  const isInstalling = installingPluginId === item.pluginId;
                  const isDone = installedPluginIds.has(item.pluginId);
                  const itemError = failedPluginErrors[item.pluginId];
                  const versionLabel =
                    item.targetVersion ?? item.requiredVersion;

                  return (
                    <div
                      key={item.pluginId}
                      className="flex items-start justify-between gap-3 rounded-lg border border-[#27272a] bg-[#18181b] p-3"
                    >
                      <div className="flex min-w-0 items-start gap-2.5">
                        <PluginIcon
                          svg={item.iconSvg}
                          colour={item.brandColour}
                          size="sm"
                        />
                        <div className="min-w-0 space-y-1">
                          <div className="flex flex-wrap items-center gap-1.5">
                            <span className="truncate font-medium text-foreground">
                              {item.displayName}
                            </span>
                            {versionLabel && (
                              <Badge
                                variant="secondary"
                                className="text-[10px] font-normal"
                              >
                                v{versionLabel}
                              </Badge>
                            )}
                            {item.isOfficial && (
                              <Badge
                                variant="outline"
                                className="gap-1 border-indigo-500/30 bg-indigo-500/10 text-[10px] font-medium text-indigo-300"
                              >
                                <BadgeCheck className="size-2.5" />
                                Official
                              </Badge>
                            )}
                            {item.verified && (
                              <Badge
                                variant="outline"
                                className="gap-1 border-emerald-500/30 bg-emerald-500/10 text-[10px] font-normal text-emerald-300"
                              >
                                <ShieldCheck className="size-2.5" />
                                Verified
                              </Badge>
                            )}
                          </div>
                          {itemError ? (
                            <p className="text-[11px] text-destructive">
                              {itemError}
                            </p>
                          ) : (
                            <p className="text-[11px] text-muted-foreground">
                              {isDone
                                ? "Installed and verified"
                                : isInstalling
                                  ? "Downloading and verifying..."
                                  : "Available in official registry"}
                            </p>
                          )}
                        </div>
                      </div>

                      <div className="shrink-0 pt-0.5">
                        {isInstalling ? (
                          <Loader2 className="size-4 animate-spin text-primary" />
                        ) : isDone ? (
                          <CheckCircle2 className="size-4 text-emerald-400" />
                        ) : (
                          <Download className="size-4 text-muted-foreground" />
                        )}
                      </div>
                    </div>
                  );
                })}

                {restorePreview.missingUnavailable.map((item) => (
                  <div
                    key={item.pluginId}
                    className="flex items-start justify-between gap-3 rounded-lg border border-amber-500/30 bg-amber-500/10 p-3"
                  >
                    <div className="flex min-w-0 items-start gap-2.5">
                      <PluginIcon
                        svg={item.iconSvg}
                        colour={item.brandColour}
                        size="sm"
                      />
                      <div className="min-w-0 space-y-1">
                        <div className="flex flex-wrap items-center gap-1.5">
                          <span className="truncate font-medium text-foreground">
                            {item.displayName}
                          </span>
                          {item.requiredVersion && (
                            <Badge
                              variant="secondary"
                              className="text-[10px] font-normal"
                            >
                              v{item.requiredVersion}
                            </Badge>
                          )}
                          {item.isOfficial && (
                            <Badge
                              variant="outline"
                              className="gap-1 border-indigo-500/30 bg-indigo-500/10 text-[10px] font-medium text-indigo-300"
                            >
                              <BadgeCheck className="size-2.5" />
                              Official
                            </Badge>
                          )}
                        </div>
                        <p className="text-[11px] text-amber-200/90">
                          {item.reason ?? "Unavailable in registry."}
                        </p>
                      </div>
                    </div>
                    <AlertTriangle className="mt-0.5 size-4 shrink-0 text-amber-400" />
                  </div>
                ))}

                {restorePreview.alreadyInstalled.map((item) => (
                  <div
                    key={item.pluginId}
                    className="flex items-start justify-between gap-3 rounded-lg border border-[#27272a] bg-[#18181b]/50 p-3"
                  >
                    <div className="flex min-w-0 items-start gap-2.5">
                      <PluginIcon
                        svg={item.iconSvg}
                        colour={item.brandColour}
                        size="sm"
                      />
                      <div className="min-w-0 space-y-1">
                        <div className="flex flex-wrap items-center gap-1.5">
                          <span className="truncate font-medium text-foreground">
                            {item.displayName}
                          </span>
                          {item.installedVersion && (
                            <Badge
                              variant="secondary"
                              className="text-[10px] font-normal"
                            >
                              v{item.installedVersion}
                            </Badge>
                          )}
                        </div>
                        <p className="text-[11px] text-muted-foreground">
                          Already installed locally
                          {item.versionDiffers && item.requiredVersion
                            ? ` (backup used v${item.requiredVersion})`
                            : ""}
                        </p>
                      </div>
                    </div>
                    <CheckCircle2 className="mt-0.5 size-4 shrink-0 text-emerald-400" />
                  </div>
                ))}
              </div>

              {error && <p className="text-xs text-destructive">{error}</p>}
            </div>

            <div className="flex shrink-0 flex-wrap items-center justify-end gap-2 border-t border-[#27272a] bg-[#18181b]/40 px-5 py-3.5">
              {restorePreview.missingAvailable.length > 0 ||
              restorePreview.missingUnavailable.length > 0 ? (
                <>
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    disabled={isBusy}
                    onClick={() => void handleRestoreWithoutInstalling()}
                  >
                    Restore Without Installing
                  </Button>
                  <Button
                    type="button"
                    size="sm"
                    disabled={
                      isBusy || restorePreview.missingAvailable.length === 0
                    }
                    onClick={() => void handleInstallPluginsAndRestore()}
                  >
                    {isBusy && (
                      <Loader2 className="mr-1.5 size-3.5 animate-spin" />
                    )}
                    Install Plugins &amp; Restore
                  </Button>
                </>
              ) : (
                <>
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    disabled={isBusy}
                    onClick={() => setRestorePreview(null)}
                  >
                    Cancel
                  </Button>
                  <Button
                    type="button"
                    size="sm"
                    disabled={isBusy}
                    onClick={() => void handleRestoreWithoutInstalling()}
                  >
                    {isBusy && (
                      <Loader2 className="mr-1.5 size-3.5 animate-spin" />
                    )}
                    Restore Anyway
                  </Button>
                </>
              )}
            </div>
          </>
        ) : (
        <>
        <DialogHeader>
          <div className="flex items-center justify-between gap-4">
            <DialogTitle className="flex items-center gap-2 text-base">
              {mode === "export" ? (
                <>
                  <Download className="size-4 text-primary" />
                  <span>Export Profile</span>
                </>
              ) : (
                <>
                  <Upload className="size-4 text-primary" />
                  <span>Import Profile</span>
                </>
              )}
            </DialogTitle>
            <DialogClose asChild>
              <Button
                type="button"
                variant="ghost"
                size="icon-sm"
                aria-label="Close"
              >
                <X className="size-4" />
              </Button>
            </DialogClose>
          </div>
        </DialogHeader>

        {mode === "export" ? (
          <form onSubmit={(e) => void handleExportSubmit(e)} className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="export-passphrase">Passphrase</Label>
              <Input
                id="export-passphrase"
                type="password"
                value={passphrase}
                onChange={(e) => setPassphrase(e.target.value)}
                placeholder="Minimum 8 characters"
                disabled={isBusy}
                autoFocus
              />
            </div>

            <div className="space-y-2">
              <Label htmlFor="export-passphrase-confirm">Confirm Passphrase</Label>
              <Input
                id="export-passphrase-confirm"
                type="password"
                value={confirmPassphrase}
                onChange={(e) => setConfirmPassphrase(e.target.value)}
                placeholder="Re-enter passphrase"
                disabled={isBusy}
              />
            </div>

            {error && <p className="text-xs text-destructive">{error}</p>}

            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                onClick={onClose}
                disabled={isBusy}
              >
                Cancel
              </Button>
              <Button type="submit" disabled={isBusy || !activeProfile}>
                {isBusy && <Loader2 className="mr-2 size-4 animate-spin" />}
                Choose File & Export
              </Button>
            </DialogFooter>
          </form>
        ) : (
          <form onSubmit={(e) => void handleImportSubmit(e)} className="space-y-4">
            <div className="space-y-2">
              <Button
                type="button"
                variant="outline"
                className="w-full justify-start gap-2"
                onClick={() => void handlePickImportFile()}
                disabled={isBusy}
              >
                <FileArchive className="size-4 text-muted-foreground" />
                <span className="truncate">
                  {metadata
                    ? `${metadata.profileName} · ${formatExportDate(metadata.exportedAt)}`
                    : "Choose Backup File..."}
                </span>
              </Button>
            </div>

            {metadata && (
              <>
                <div className="space-y-2">
                  <Label htmlFor="import-profile-name">Profile Name</Label>
                  <Input
                    id="import-profile-name"
                    value={customName}
                    onChange={(e) => setCustomName(e.target.value)}
                    placeholder={metadata.profileName}
                    disabled={isBusy}
                  />
                </div>

                <div className="space-y-2">
                  <Label htmlFor="import-passphrase">Passphrase</Label>
                  <Input
                    id="import-passphrase"
                    type="password"
                    value={passphrase}
                    onChange={(e) => setPassphrase(e.target.value)}
                    placeholder="Enter backup passphrase"
                    disabled={isBusy}
                    autoFocus
                  />
                </div>
              </>
            )}

            {error && <p className="text-xs text-destructive">{error}</p>}

            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                onClick={onClose}
                disabled={isBusy}
              >
                Cancel
              </Button>
              <Button type="submit" disabled={isBusy || !metadata}>
                {isBusy && <Loader2 className="mr-2 size-4 animate-spin" />}
                Import
              </Button>
            </DialogFooter>
          </form>
        )}
        </>
        )}
      </DialogContent>
    </Dialog>
  );
}
