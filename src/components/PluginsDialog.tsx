import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  AlertTriangle,
  ArrowLeft,
  ArrowRight,
  BadgeCheck,
  Check,
  CheckCircle2,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Download,
  ExternalLink,
  FolderUp,
  Globe,
  Loader2,
  Puzzle,
  RefreshCw,
  Search,
  Shield,
  ShieldAlert,
  ShieldCheck,
  Trash2,
  X,
} from "lucide-react";
import ReactMarkdown from "react-markdown";
import { toast } from "sonner";
import { PluginIcon } from "@/components/PluginIcon";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import {
  getPluginManifest,
  getRegistryPlugins,
  installLocalPlugin,
  installRegistryPlugin,
  setPluginEnabled,
  uninstallPlugin,
  type PluginManifestDetail,
  type RegistryPluginView,
} from "@/lib/tauri";

interface PluginsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onPluginsChanged: () => Promise<void>;
  developerMode?: boolean;
}

type FilterTab = "all" | "official" | "installed" | "updates";
const PAGE_SIZE = 6;

function formatDateLabel(iso: string | null | undefined): string | null {
  if (!iso) return null;
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return null;
  return date.toLocaleDateString(undefined, { dateStyle: "medium" });
}

function formatBytesKb(bytes: number | null | undefined): string {
  if (bytes === null || bytes === undefined) return "Unknown";
  return `${Math.max(1, Math.round(bytes / 1024))} KB`;
}

export function PluginsDialog({
  open,
  onOpenChange,
  onPluginsChanged,
  developerMode = false,
}: PluginsDialogProps) {
  const [plugins, setPlugins] = useState<RegistryPluginView[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [mutatingPluginIds, setMutatingPluginIds] = useState<Set<string>>(
    () => new Set(),
  );
  const [sideloading, setSideloading] = useState(false);

  const [searchQuery, setSearchQuery] = useState("");
  const [filterTab, setFilterTab] = useState<FilterTab>("all");
  const [page, setPage] = useState(1);

  const [selectedPluginId, setSelectedPluginId] = useState<string | null>(null);
  const [manifestDetail, setManifestDetail] =
    useState<PluginManifestDetail | null>(null);
  const [manifestLoading, setManifestLoading] = useState(false);
  const [manifestError, setManifestError] = useState<string | null>(null);
  const [selectedVersion, setSelectedVersion] = useState<string | null>(null);
  const [versionMenuOpen, setVersionMenuOpen] = useState(false);
  const [confirmUpdateVersion, setConfirmUpdateVersion] = useState<
    string | null
  >(null);
  const versionMenuRef = useRef<HTMLDivElement>(null);

  const loadPlugins = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const list = await getRegistryPlugins();
      setPlugins(list);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  const loadManifest = useCallback(async (pluginId: string) => {
    setManifestLoading(true);
    setManifestError(null);
    try {
      const detail = await getPluginManifest(pluginId);
      setManifestDetail(detail);
      setSelectedVersion(detail.installedVersion ?? detail.latestVersion);
    } catch (err) {
      setManifestError(String(err));
    } finally {
      setManifestLoading(false);
    }
  }, []);

  useEffect(() => {
    if (!open) {
      setSelectedPluginId(null);
      setManifestDetail(null);
      setManifestError(null);
      setVersionMenuOpen(false);
      return;
    }
    void loadPlugins();
  }, [open, loadPlugins]);

  useEffect(() => {
    if (!versionMenuOpen) return;
    const handlePointerDown = (e: MouseEvent) => {
      if (
        versionMenuRef.current &&
        !versionMenuRef.current.contains(e.target as Node)
      ) {
        setVersionMenuOpen(false);
      }
    };
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        setVersionMenuOpen(false);
      }
    };
    window.addEventListener("mousedown", handlePointerDown);
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("mousedown", handlePointerDown);
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [versionMenuOpen]);

  useEffect(() => {
    setPage(1);
  }, [searchQuery, filterTab]);

  const counts = useMemo(() => {
    let official = 0;
    let installed = 0;
    let updates = 0;
    for (const p of plugins) {
      if (p.isOfficial) official += 1;
      if (p.isInstalled) installed += 1;
      if (p.updateAvailable) updates += 1;
    }
    return { all: plugins.length, official, installed, updates };
  }, [plugins]);

  const filteredPlugins = useMemo(() => {
    const q = searchQuery.trim().toLowerCase();
    return plugins.filter((p) => {
      if (filterTab === "official" && !p.isOfficial) return false;
      if (filterTab === "installed" && !p.isInstalled) return false;
      if (filterTab === "updates" && !p.updateAvailable) return false;
      if (!q) return true;
      return (
        p.name.toLowerCase().includes(q) ||
        p.id.toLowerCase().includes(q) ||
        (p.description ?? "").toLowerCase().includes(q) ||
        p.authors.some((a) => a.toLowerCase().includes(q))
      );
    });
  }, [plugins, filterTab, searchQuery]);

  const totalPages = Math.max(1, Math.ceil(filteredPlugins.length / PAGE_SIZE));
  const safePage = Math.min(page, totalPages);
  const pagedPlugins = useMemo(() => {
    const start = (safePage - 1) * PAGE_SIZE;
    return filteredPlugins.slice(start, start + PAGE_SIZE);
  }, [filteredPlugins, safePage]);

  const openManifestView = (pluginId: string) => {
    setSelectedPluginId(pluginId);
    setManifestDetail(null);
    void loadManifest(pluginId);
  };

  const addMutatingId = useCallback((id: string) => {
    setMutatingPluginIds((prev) => {
      const next = new Set(prev);
      next.add(id);
      return next;
    });
  }, []);

  const removeMutatingId = useCallback((id: string) => {
    setMutatingPluginIds((prev) => {
      const next = new Set(prev);
      next.delete(id);
      return next;
    });
  }, []);

  const handleInstallOrUpdate = async (
    pluginId: string,
    pluginName: string,
    version?: string | null,
    isUpdate?: boolean,
  ) => {
    addMutatingId(pluginId);
    try {
      const record = await installRegistryPlugin(pluginId, version);
      // Update local state in-place so neither the card grid nor the overview unmounts or flickers.
      setPlugins((prev) =>
        prev.map((p) => {
          if (p.id !== pluginId) return p;
          const nextUpdateAvailable =
            p.latestVersion !== null && record.version !== p.latestVersion;
          return {
            ...p,
            installedVersion: record.version,
            isInstalled: true,
            updateAvailable: nextUpdateAvailable,
            enabled: record.enabled,
            verified: record.verified,
            source: record.source,
          };
        }),
      );
      setManifestDetail((prev) => {
        if (!prev || prev.id !== pluginId) return prev;
        return {
          ...prev,
          installedVersion: record.version,
          isInstalled: true,
          updateAvailable: record.version !== prev.latestVersion,
          enabled: record.enabled,
          verified: record.verified,
        };
      });
      if (selectedPluginId === pluginId) {
        setSelectedVersion(record.version);
      }
      void onPluginsChanged();
      toast.success(
        isUpdate ? `Updated ${pluginName}.` : `Installed ${pluginName}.`,
      );
    } catch (err) {
      toast.error(String(err));
    } finally {
      removeMutatingId(pluginId);
    }
  };

  const handleToggleEnabled = async (
    pluginId: string,
    enabled: boolean,
  ) => {
    addMutatingId(pluginId);
    try {
      await setPluginEnabled(pluginId, enabled);
      setPlugins((prev) =>
        prev.map((p) => (p.id === pluginId ? { ...p, enabled } : p)),
      );
      setManifestDetail((prev) =>
        prev && prev.id === pluginId ? { ...prev, enabled } : prev,
      );
      void onPluginsChanged();
    } catch (err) {
      toast.error(String(err));
    } finally {
      removeMutatingId(pluginId);
    }
  };

  const handleUninstall = async (pluginId: string, pluginName: string) => {
    addMutatingId(pluginId);
    try {
      await uninstallPlugin(pluginId);
      setPlugins((prev) =>
        prev
          .filter((p) => !(p.id === pluginId && p.source === "local"))
          .map((p) =>
            p.id === pluginId
              ? {
                  ...p,
                  installedVersion: null,
                  isInstalled: false,
                  updateAvailable: false,
                  enabled: false,
                }
              : p,
          ),
      );
      setManifestDetail((prev) =>
        prev && prev.id === pluginId
          ? {
              ...prev,
              installedVersion: null,
              isInstalled: false,
              updateAvailable: false,
              enabled: false,
            }
          : prev,
      );
      void onPluginsChanged();
      toast.success(`Uninstalled ${pluginName}.`);
    } catch (err) {
      toast.error(String(err));
    } finally {
      removeMutatingId(pluginId);
    }
  };

  const handleSideload = async () => {
    try {
      const selected = await openFileDialog({
        multiple: false,
        filters: [{ name: "WebAssembly Plugin", extensions: ["wasm"] }],
      });
      if (!selected || Array.isArray(selected)) return;

      setSideloading(true);
      const record = await installLocalPlugin(selected);
      const list = await getRegistryPlugins();
      setPlugins(list);
      void onPluginsChanged();
      toast.success(`Loaded local plugin ${record.name} (v${record.version}).`);
    } catch (err) {
      toast.error(String(err));
    } finally {
      setSideloading(false);
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

  const isDetailMode = selectedPluginId !== null;
  const orderedVersions = useMemo(() => {
    if (!manifestDetail) return [];
    // Keep manifest order (newest first), ensuring latestVersion is prioritised if unordered.
    return [...manifestDetail.versions].sort((a, b) => {
      if (a.version === manifestDetail.latestVersion) return -1;
      if (b.version === manifestDetail.latestVersion) return 1;
      return 0;
    });
  }, [manifestDetail]);

  const activeVersionMeta = useMemo(() => {
    if (!manifestDetail) return null;
    return (
      orderedVersions.find((v) => v.version === selectedVersion) ??
      orderedVersions[0] ??
      null
    );
  }, [manifestDetail, orderedVersions, selectedVersion]);

  const targetVersion =
    activeVersionMeta?.version ??
    selectedVersion ??
    manifestDetail?.latestVersion ??
    "";
  const isSelectedVersionInstalled = Boolean(
    manifestDetail?.isInstalled &&
      targetVersion &&
      manifestDetail.installedVersion === targetVersion,
  );

  const selectedIsCompatible =
    activeVersionMeta?.is_compatible ?? manifestDetail?.isCompatible ?? true;
  const selectedCompatReq =
    activeVersionMeta?.compatible_app_version ??
    manifestDetail?.compatibleAppVersion ??
    ">=1.0.0";
  const selectedCompatMsg =
    activeVersionMeta?.compatibility_message ??
    manifestDetail?.compatibilityError ??
    null;

  const latestVersionMeta = useMemo(() => {
    if (!manifestDetail) return null;
    return (
      orderedVersions.find((v) => v.version === manifestDetail.latestVersion) ??
      null
    );
  }, [manifestDetail, orderedVersions]);
  const latestIsCompatible =
    latestVersionMeta?.is_compatible ?? manifestDetail?.isCompatible ?? true;

  const confirmVersionMeta = useMemo(() => {
    if (!manifestDetail || !confirmUpdateVersion) return null;
    return (
      orderedVersions.find((v) => v.version === confirmUpdateVersion) ?? null
    );
  }, [manifestDetail, orderedVersions, confirmUpdateVersion]);
  const confirmIsCompatible =
    confirmVersionMeta?.is_compatible ?? manifestDetail?.isCompatible ?? true;

  const isTargetUpdate = Boolean(
    manifestDetail?.isInstalled &&
      targetVersion &&
      targetVersion !== manifestDetail.installedVersion &&
      (targetVersion === manifestDetail.latestVersion ||
        (manifestDetail.installedVersion &&
          targetVersion.localeCompare(
            manifestDetail.installedVersion,
            undefined,
            { numeric: true, sensitivity: "base" },
          ) > 0)),
  );

  return (
    <>
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        showCloseButton={false}
        className="flex max-h-[85vh] w-full min-w-0 max-w-3xl flex-col gap-0 overflow-hidden p-0 sm:max-w-3xl"
      >
        {isDetailMode ? (
          <>
            <DialogHeader className="shrink-0 border-b border-border/60 p-5">
              <div className="flex items-center justify-between gap-4">
                <div className="flex min-w-0 items-center gap-2.5">
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    className="h-8 px-2.5 text-xs"
                    onClick={() => {
                      setSelectedPluginId(null);
                      setManifestDetail(null);
                      setManifestError(null);
                      setVersionMenuOpen(false);
                    }}
                  >
                    <ArrowLeft className="mr-1.5 size-3.5" />
                    Back to Plugins
                  </Button>
                  <span className="h-4 w-px bg-border" />
                  <DialogTitle className="truncate text-base">
                    {manifestDetail?.name ?? selectedPluginId}
                  </DialogTitle>
                </div>

                <div className="flex shrink-0 items-center gap-2">
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    className="h-8 px-2.5 text-xs"
                    onClick={() =>
                      void handleOpenExternal(
                        manifestDetail?.hubUrl ??
                          `https://socialposter.dev/plugins/${selectedPluginId}`,
                      )
                    }
                  >
                    <span>socialposter.dev</span>
                    <ExternalLink className="ml-1.5 size-3" />
                  </Button>
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
              </div>
            </DialogHeader>

            <div className="min-h-0 flex-1 overflow-y-auto p-5">
              {manifestLoading ? (
                <div className="flex items-center justify-center py-16 text-sm text-muted-foreground">
                  <Loader2 className="mr-2 size-4 animate-spin" />
                  <span>Loading plugin manifest...</span>
                </div>
              ) : manifestError ? (
                <div className="rounded-xl border border-destructive/40 bg-destructive/10 p-4">
                  <p className="text-xs text-destructive">{manifestError}</p>
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    className="mt-3 h-7 border-destructive/40 px-2.5 text-xs text-destructive hover:bg-destructive/15"
                    onClick={() =>
                      selectedPluginId && void loadManifest(selectedPluginId)
                    }
                  >
                    Retry
                  </Button>
                </div>
              ) : manifestDetail ? (
                <div className="space-y-5">
                  {manifestDetail.isInstalled &&
                    manifestDetail.updateAvailable && (
                      <div className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-primary/30 bg-primary/10 px-4 py-3 text-xs">
                        <div className="flex min-w-0 items-center gap-2">
                          <Download className="size-4 shrink-0 text-primary" />
                          <span className="font-medium text-foreground">
                            Update available: v{manifestDetail.installedVersion} →
                            v{manifestDetail.latestVersion}
                          </span>
                        </div>
                        <Button
                          type="button"
                          size="sm"
                          variant="secondary"
                          className="h-7 px-2.5 text-xs"
                          onClick={() => {
                            setSelectedVersion(manifestDetail.latestVersion);
                            setConfirmUpdateVersion(
                              manifestDetail.latestVersion,
                            );
                          }}
                        >
                          View Update &amp; Release Notes
                        </Button>
                      </div>
                    )}

                  <div className="flex flex-wrap items-start justify-between gap-4 rounded-xl border border-border/60 bg-card p-4">
                    <div className="flex min-w-0 items-start gap-3.5">
                      <PluginIcon
                        svg={manifestDetail.iconSvg}
                        colour={manifestDetail.brandColour}
                        size="lg"
                      />

                      <div className="min-w-0 space-y-1.5">
                        <div className="flex flex-wrap items-center gap-1.5">
                          <h2 className="text-base font-semibold">
                            {manifestDetail.name}
                          </h2>
                          <Badge
                            variant="secondary"
                            className="text-xs font-normal"
                          >
                            v{manifestDetail.latestVersion}
                          </Badge>
                          {manifestDetail.isOfficial && (
                            <Badge
                              variant="outline"
                              className="gap-1 border-indigo-500/30 bg-indigo-500/10 text-[11px] font-medium text-indigo-600 dark:text-indigo-300"
                            >
                              <BadgeCheck className="size-3" />
                              Official
                            </Badge>
                          )}
                          {manifestDetail.verified ? (
                            <Badge
                              variant="outline"
                              className="gap-1 border-emerald-500/30 bg-emerald-500/10 text-[11px] font-normal text-emerald-700 dark:text-emerald-300"
                            >
                              <ShieldCheck className="size-3" />
                              Verified
                            </Badge>
                          ) : (
                            <Badge
                              variant="outline"
                              className="gap-1 border-amber-500/30 bg-amber-500/10 text-[11px] font-normal text-amber-700 dark:text-amber-300"
                            >
                              <ShieldAlert className="size-3" />
                              Local
                            </Badge>
                          )}
                          {selectedIsCompatible ? (
                            <Badge
                              variant="outline"
                              title={`Requires SocialPoster ${selectedCompatReq}`}
                              className="gap-1 border-emerald-500/30 bg-emerald-500/10 text-[11px] font-normal text-emerald-700 dark:text-emerald-300"
                            >
                              <Check className="size-3" />
                              Compatible with v{manifestDetail.appVersion}
                            </Badge>
                          ) : (
                            <Badge
                              variant="outline"
                              title={
                                selectedCompatMsg ??
                                `Requires SocialPoster ${selectedCompatReq}`
                              }
                              className="gap-1 border-destructive/40 bg-destructive/10 text-[11px] font-medium text-destructive"
                            >
                              <AlertTriangle className="size-3" />
                              Incompatible with v{manifestDetail.appVersion}
                            </Badge>
                          )}
                        </div>

                        {manifestDetail.author && (
                          <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                            <span>
                              by {manifestDetail.author.display_name}
                            </span>
                            {manifestDetail.author.url?.startsWith(
                              "https://",
                            ) && (
                              <button
                                type="button"
                                onClick={() =>
                                  void handleOpenExternal(
                                    manifestDetail.author!.url!,
                                  )
                                }
                                className="inline-flex cursor-pointer items-center gap-1 text-foreground underline underline-offset-2 hover:opacity-80"
                              >
                                <span>Author</span>
                                <ExternalLink className="size-3" />
                              </button>
                            )}
                          </div>
                        )}
                      </div>
                    </div>

                    <div className="flex flex-wrap items-center gap-2">
                      {manifestDetail.isInstalled && (
                        <div className="flex items-center gap-2 pr-1">
                          <span className="text-xs text-muted-foreground">
                            {manifestDetail.enabled ? "Enabled" : "Disabled"}
                          </span>
                          <Switch
                            checked={manifestDetail.enabled}
                            disabled={mutatingPluginIds.has(manifestDetail.id)}
                            onCheckedChange={(checked) =>
                              void handleToggleEnabled(
                                manifestDetail.id,
                                checked,
                              )
                            }
                          />
                        </div>
                      )}

                      {isSelectedVersionInstalled ? (
                        <>
                          <Button
                            type="button"
                            size="sm"
                            variant="secondary"
                            disabled
                            className="gap-1.5"
                          >
                            <CheckCircle2 className="size-3.5 text-emerald-400" />
                            Installed
                          </Button>
                          {manifestDetail.updateAvailable && (
                            <Button
                              type="button"
                              size="sm"
                              disabled={
                                mutatingPluginIds.has(manifestDetail.id) ||
                                !latestIsCompatible
                              }
                              onClick={() => {
                                setSelectedVersion(
                                  manifestDetail.latestVersion,
                                );
                                setConfirmUpdateVersion(
                                  manifestDetail.latestVersion,
                                );
                              }}
                            >
                              {mutatingPluginIds.has(manifestDetail.id) ? (
                                <Loader2 className="mr-1.5 size-3.5 animate-spin" />
                              ) : (
                                <Download className="mr-1.5 size-3.5" />
                              )}
                              Update to v{manifestDetail.latestVersion}
                            </Button>
                          )}
                        </>
                      ) : (
                        <Button
                          type="button"
                          size="sm"
                          disabled={
                            mutatingPluginIds.has(manifestDetail.id) ||
                            !selectedIsCompatible
                          }
                          onClick={() => {
                            if (manifestDetail.isInstalled && isTargetUpdate) {
                              setConfirmUpdateVersion(targetVersion);
                            } else {
                              void handleInstallOrUpdate(
                                manifestDetail.id,
                                manifestDetail.name,
                                targetVersion,
                                manifestDetail.isInstalled,
                              );
                            }
                          }}
                        >
                          {mutatingPluginIds.has(manifestDetail.id) ? (
                            <Loader2 className="mr-1.5 size-3.5 animate-spin" />
                          ) : (
                            <Download className="mr-1.5 size-3.5" />
                          )}
                          {manifestDetail.isInstalled
                            ? isTargetUpdate
                              ? `Update to v${targetVersion}`
                              : `Switch to v${targetVersion}`
                            : `Install v${targetVersion}`}
                        </Button>
                      )}

                      {manifestDetail.isInstalled && (
                        <Button
                          type="button"
                          size="sm"
                          variant="outline"
                          className="text-destructive hover:bg-destructive/10"
                          disabled={mutatingPluginIds.has(manifestDetail.id)}
                          onClick={() =>
                            void handleUninstall(
                              manifestDetail.id,
                              manifestDetail.name,
                            )
                          }
                        >
                          {mutatingPluginIds.has(manifestDetail.id) ? (
                            <Loader2 className="mr-1.5 size-3.5 animate-spin" />
                          ) : (
                            <Trash2 className="mr-1.5 size-3.5" />
                          )}
                          Uninstall
                        </Button>
                      )}
                    </div>
                  </div>

                  {!selectedIsCompatible && (
                    <div
                      role="alert"
                      className="flex items-start gap-2.5 rounded-xl border border-amber-500/40 bg-amber-500/10 px-4 py-3 text-xs text-amber-200"
                    >
                      <AlertTriangle className="mt-0.5 size-4 shrink-0 text-amber-400" />
                      <div className="space-y-0.5">
                        <p className="font-semibold text-amber-300">
                          Incompatible with SocialPoster v
                          {manifestDetail.appVersion} (requires{" "}
                          {selectedCompatReq})
                        </p>
                        <p className="text-amber-200/90">
                          You cannot install this plugin or update to this
                          version because it is incompatible with SocialPoster
                          v{manifestDetail.appVersion}.
                        </p>
                      </div>
                    </div>
                  )}

                  {manifestDetail.description && (
                    <section className="space-y-1.5 rounded-xl border border-border/60 bg-card p-4">
                      <h3 className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                        Overview
                      </h3>
                      <p className="text-sm whitespace-pre-wrap text-foreground">
                        {manifestDetail.description}
                      </p>
                    </section>
                  )}

                  {manifestDetail.setupInstructions && (
                    <section className="space-y-1.5 rounded-xl border border-border/60 bg-card p-4">
                      <h3 className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                        Setup
                      </h3>
                      <div className="prose prose-sm dark:prose-invert max-w-none text-sm text-muted-foreground">
                        <ReactMarkdown>
                          {manifestDetail.setupInstructions}
                        </ReactMarkdown>
                      </div>
                    </section>
                  )}

                  <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
                    <section className="space-y-3 rounded-xl border border-border/60 bg-card p-4">
                      <h3 className="text-sm font-semibold">Capabilities</h3>
                      <dl className="space-y-2 text-xs">
                        <div className="flex justify-between gap-2">
                          <dt className="text-muted-foreground">
                            Character limit
                          </dt>
                          <dd className="font-medium">
                            {manifestDetail.capabilities.max_characters ?? "—"}
                          </dd>
                        </div>
                        <div className="flex justify-between gap-2">
                          <dt className="text-muted-foreground">
                            Max attachments
                          </dt>
                          <dd className="font-medium">
                            {manifestDetail.capabilities.max_attachments ?? "—"}
                          </dd>
                        </div>
                        <div className="flex justify-between gap-2">
                          <dt className="text-muted-foreground">Alt text</dt>
                          <dd className="font-medium">
                            {manifestDetail.capabilities.supports_alt_text
                              ? "Supported"
                              : "No"}
                          </dd>
                        </div>
                        <div className="flex justify-between gap-2">
                          <dt className="text-muted-foreground">Threads</dt>
                          <dd className="font-medium">
                            {manifestDetail.capabilities.supports_threads
                              ? "Supported"
                              : "No"}
                          </dd>
                        </div>
                        <div className="flex justify-between gap-2">
                          <dt className="text-muted-foreground">
                            Post deletion
                          </dt>
                          <dd className="font-medium">
                            {manifestDetail.capabilities.supports_delete
                              ? "Supported"
                              : "No"}
                          </dd>
                        </div>
                      </dl>

                      {manifestDetail.capabilities.supported_media_types
                        .length > 0 && (
                        <div className="space-y-1.5 border-t border-border/60 pt-2.5">
                          <p className="text-[11px] font-medium text-muted-foreground">
                            Supported media types
                          </p>
                          <div className="flex flex-wrap gap-1">
                            {manifestDetail.capabilities.supported_media_types.map(
                              (mime) => (
                                <span
                                  key={mime}
                                  className="rounded border border-border/60 bg-muted/40 px-1.5 py-0.5 text-[11px]"
                                >
                                  {mime}
                                </span>
                              ),
                            )}
                          </div>
                        </div>
                      )}
                    </section>

                    <section className="space-y-3 rounded-xl border border-border/60 bg-card p-4">
                      <h3 className="text-sm font-semibold">
                        Permissions
                      </h3>

                      <div className="flex flex-wrap gap-1.5">
                        <Badge
                          variant="outline"
                          className="gap-1 text-[11px] font-normal"
                        >
                          <Shield className="size-3" />
                          No host filesystem access
                        </Badge>
                      </div>

                      <div className="space-y-1.5">
                        <p className="text-[11px] font-medium text-muted-foreground">
                          Allowed network domains
                        </p>
                        {manifestDetail.allowedDomains.length === 0 ? (
                          <p className="text-xs text-muted-foreground">None</p>
                        ) : (
                          <div className="flex flex-wrap gap-1">
                            {manifestDetail.allowedDomains.map((domain) => (
                              <span
                                key={domain}
                                className="inline-flex items-center gap-1 rounded border border-border/60 bg-muted/40 px-1.5 py-0.5 text-[11px]"
                              >
                                <Globe className="size-2.5 text-muted-foreground" />
                                {domain}
                              </span>
                            ))}
                          </div>
                        )}
                      </div>

                      {orderedVersions.length > 0 && (
                        <div className="space-y-2 border-t border-border/60 pt-2.5">
                          <div className="flex items-center justify-between gap-2">
                            <span className="text-[11px] font-medium text-muted-foreground">
                              Versions
                            </span>
                            <div ref={versionMenuRef} className="relative">
                              <button
                                type="button"
                                aria-haspopup="listbox"
                                aria-expanded={versionMenuOpen}
                                aria-label="Select plugin version"
                                onClick={() =>
                                  setVersionMenuOpen((prev) => !prev)
                                }
                                className="inline-flex h-7 cursor-pointer items-center gap-1.5 rounded-lg border border-[#27272a] bg-[#111113] px-2.5 text-xs font-medium text-foreground shadow-sm transition-colors hover:bg-[#18181b] focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                              >
                                <span>
                                  v{activeVersionMeta?.version ?? targetVersion}
                                </span>
                                {Boolean(
                                  activeVersionMeta?.ed25519_signature?.trim(),
                                ) && (
                                  <span
                                    title="Verified signature"
                                    aria-label="Verified signature"
                                    className="inline-flex items-center text-emerald-400"
                                  >
                                    <ShieldCheck className="size-3.5" />
                                  </span>
                                )}
                                {manifestDetail.installedVersion &&
                                  activeVersionMeta?.version ===
                                    manifestDetail.installedVersion && (
                                    <span
                                      title="Currently installed version"
                                      aria-label="Installed version"
                                      className="inline-flex items-center text-primary"
                                    >
                                      <CheckCircle2 className="size-3.5" />
                                    </span>
                                  )}
                                <ChevronDown
                                  className={`size-3.5 text-muted-foreground transition-transform duration-150 ${
                                    versionMenuOpen ? "rotate-180" : ""
                                  }`}
                                />
                              </button>

                              {versionMenuOpen && (
                                <div
                                  role="listbox"
                                  aria-label="Available plugin versions"
                                  className="absolute right-0 bottom-full z-50 mb-1.5 w-64 rounded-lg border border-[#27272a] bg-[#111113] p-1 text-foreground shadow-lg"
                                >
                                  <div className="max-h-52 overflow-y-auto py-0.5">
                                    {orderedVersions.map((ver) => {
                                      const isSelected =
                                        ver.version ===
                                        activeVersionMeta?.version;
                                      const isVerified = Boolean(
                                        ver.ed25519_signature?.trim(),
                                      );
                                      const isInstalled =
                                        manifestDetail.installedVersion ===
                                        ver.version;
                                      const isLatest =
                                        orderedVersions.length > 1 &&
                                        ver.version ===
                                          manifestDetail.latestVersion;

                                      return (
                                        <button
                                          key={ver.version}
                                          type="button"
                                          role="option"
                                          aria-selected={isSelected}
                                          onClick={() => {
                                            setSelectedVersion(ver.version);
                                            setVersionMenuOpen(false);
                                          }}
                                          className={`flex w-full cursor-pointer items-center justify-between gap-2 rounded-md px-2.5 py-1.5 text-left text-xs transition-colors hover:bg-[#18181b] ${
                                            isSelected
                                              ? "bg-[#18181b] font-medium text-foreground"
                                              : "text-muted-foreground hover:text-foreground"
                                          }`}
                                        >
                                          <span className="flex min-w-0 items-center gap-1.5">
                                            <Check
                                              className={`size-3.5 shrink-0 text-primary ${
                                                isSelected
                                                  ? "opacity-100"
                                                  : "opacity-0"
                                              }`}
                                            />
                                            <span className="truncate font-medium text-foreground">
                                              v{ver.version}
                                            </span>
                                            {isVerified && (
                                              <span
                                                title="Verified signature"
                                                aria-label="Verified"
                                                className="inline-flex shrink-0 items-center text-emerald-400"
                                              >
                                                <ShieldCheck className="size-3.5" />
                                              </span>
                                            )}
                                            {ver.is_compatible === false && (
                                              <span
                                                title={`Incompatible with SocialPoster v${manifestDetail.appVersion} (requires ${ver.compatible_app_version ?? ">=1.0.0"})`}
                                                aria-label="Incompatible version"
                                                className="inline-flex shrink-0 items-center text-amber-400"
                                              >
                                                <AlertTriangle className="size-3.5" />
                                              </span>
                                            )}
                                            {isLatest && (
                                              <span className="shrink-0 rounded border border-[#27272a] bg-[#18181b] px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground">
                                                Latest
                                              </span>
                                            )}
                                            {isInstalled && (
                                              <span className="inline-flex shrink-0 items-center gap-0.5 rounded border border-emerald-500/30 bg-emerald-500/10 px-1.5 py-0.5 text-[10px] font-medium text-emerald-400">
                                                <CheckCircle2 className="size-2.5" />
                                                Installed
                                              </span>
                                            )}
                                          </span>
                                          {ver.file_size_bytes != null && (
                                            <span className="shrink-0 text-[10px] text-muted-foreground">
                                              {formatBytesKb(
                                                ver.file_size_bytes,
                                              )}
                                            </span>
                                          )}
                                        </button>
                                      );
                                    })}
                                  </div>
                                </div>
                              )}
                            </div>
                          </div>

                          {activeVersionMeta && (
                            <div className="space-y-1.5 text-[11px] text-muted-foreground">
                              <div className="flex justify-between">
                                <span>Binary size</span>
                                <span className="font-medium text-foreground">
                                  {formatBytesKb(
                                    activeVersionMeta.file_size_bytes,
                                  )}
                                </span>
                              </div>
                              {formatDateLabel(
                                activeVersionMeta.published_at,
                              ) && (
                                <div className="flex justify-between">
                                  <span>Published</span>
                                  <span className="font-medium text-foreground">
                                    {formatDateLabel(
                                      activeVersionMeta.published_at,
                                    )}
                                  </span>
                                </div>
                              )}
                              <div className="space-y-0.5">
                                <span>SHA-256</span>
                                <p className="truncate font-mono text-[10px] text-foreground">
                                  {activeVersionMeta.sha256}
                                </p>
                              </div>
                              {activeVersionMeta.release_notes && (
                                <div className="space-y-0.5 pt-0.5">
                                  <span>Release notes</span>
                                  <p className="text-[11px] leading-relaxed text-foreground/90">
                                    {activeVersionMeta.release_notes}
                                  </p>
                                </div>
                              )}
                            </div>
                          )}
                        </div>
                      )}
                    </section>
                  </div>
                </div>
              ) : null}
            </div>
          </>
        ) : (
          <>
            <DialogHeader className="shrink-0 border-b border-border/60 p-5">
              <div className="flex items-center justify-between gap-4">
                <DialogTitle className="flex items-center gap-2 text-base">
                  <Puzzle className="size-4 text-primary" />
                  <span>Plugins</span>
                  <Badge variant="secondary" className="text-xs font-normal">
                    {plugins.length}
                  </Badge>
                </DialogTitle>

                <div className="flex items-center gap-2">
                  {developerMode && (
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      className="h-8 px-2.5 text-xs"
                      disabled={sideloading}
                      onClick={() => void handleSideload()}
                    >
                      {sideloading ? (
                        <Loader2 className="mr-1.5 size-3.5 animate-spin" />
                      ) : (
                        <FolderUp className="mr-1.5 size-3.5" />
                      )}
                      Sideload .wasm
                    </Button>
                  )}
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon-sm"
                    disabled={loading}
                    onClick={() => void loadPlugins()}
                    aria-label="Refresh plugin registry"
                    title="Refresh"
                  >
                    <RefreshCw
                      className={`size-4${loading ? " animate-spin" : ""}`}
                    />
                  </Button>
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
              </div>
            </DialogHeader>

            <div className="min-h-0 flex-1 space-y-4 overflow-y-auto p-5">
              <div className="flex flex-wrap items-center justify-between gap-2.5">
                <div className="relative min-w-44 flex-1">
                  <Search className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground" />
                  <Input
                    value={searchQuery}
                    onChange={(e) => setSearchQuery(e.target.value)}
                    placeholder="Search plugins..."
                    aria-label="Search plugins"
                    className="h-8 pl-8 text-xs"
                  />
                </div>

                <div className="flex items-center gap-1 rounded-lg border bg-muted/40 p-0.5 text-xs">
                  {(
                    [
                      ["all", `All (${counts.all})`],
                      ["official", `Official (${counts.official})`],
                      ["installed", `Installed (${counts.installed})`],
                      ["updates", `Updates (${counts.updates})`],
                    ] as const
                  ).map(([tab, label]) => (
                    <button
                      key={tab}
                      type="button"
                      onClick={() => setFilterTab(tab)}
                      className={`cursor-pointer rounded-md px-2.5 py-1 text-xs font-medium transition-colors ${
                        filterTab === tab
                          ? "bg-background text-foreground shadow-2xs"
                          : "text-muted-foreground hover:text-foreground"
                      }`}
                    >
                      {label}
                    </button>
                  ))}
                </div>
              </div>

              {error && (
                <div className="flex items-center justify-between gap-3 rounded-xl border border-destructive/40 bg-destructive/10 px-3.5 py-2.5 text-xs text-destructive">
                  <span className="min-w-0 flex-1 break-words [overflow-wrap:anywhere]">
                    {error}
                  </span>
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    className="h-7 shrink-0 border-destructive/40 px-2.5 text-xs text-destructive hover:bg-destructive/15"
                    disabled={loading}
                    onClick={() => void loadPlugins()}
                  >
                    Retry
                  </Button>
                </div>
              )}

              {loading && plugins.length === 0 ? (
                <div className="flex items-center justify-center py-12 text-sm text-muted-foreground">
                  <Loader2 className="mr-2 size-4 animate-spin" />
                  <span>Loading registry...</span>
                </div>
              ) : filteredPlugins.length === 0 ? (
                <p className="py-10 text-center text-sm text-muted-foreground">
                  No plugins match your filter.
                </p>
              ) : (
                <div className="grid grid-cols-1 gap-3.5 md:grid-cols-2">
                  {pagedPlugins.map((plugin) => {
                    const isBusy = mutatingPluginIds.has(plugin.id);
                    const versionText =
                      plugin.installedVersion ?? plugin.latestVersion;
                    const updatedLabel = formatDateLabel(plugin.updatedAt);

                    return (
                      <article
                        key={plugin.id}
                        role="button"
                        tabIndex={0}
                        onClick={() => openManifestView(plugin.id)}
                        onKeyDown={(e) => {
                          if (e.key === "Enter" || e.key === " ") {
                            e.preventDefault();
                            openManifestView(plugin.id);
                          }
                        }}
                        className="flex min-w-0 cursor-pointer flex-col justify-between gap-3 rounded-xl border border-border/60 bg-card p-3.5 shadow-2xs transition-colors hover:border-border"
                      >
                        <div className="space-y-2">
                          <div className="flex items-start gap-2.5">
                            <PluginIcon
                              svg={plugin.iconSvg}
                              colour={plugin.brandColour}
                              size="sm"
                            />

                            <div className="min-w-0 flex-1 space-y-1">
                              <div className="flex flex-wrap items-center gap-1.5">
                                <span className="truncate text-sm font-semibold">
                                  {plugin.name}
                                </span>
                                {versionText && (
                                  <Badge
                                    variant="secondary"
                                    className="text-[11px] font-normal"
                                  >
                                    v{versionText}
                                  </Badge>
                                )}
                                {plugin.isOfficial && (
                                  <Badge
                                    variant="outline"
                                    className="gap-1 border-indigo-500/30 bg-indigo-500/10 text-[10px] font-medium text-indigo-600 dark:text-indigo-300"
                                  >
                                    <BadgeCheck className="size-2.5" />
                                    Official
                                  </Badge>
                                )}
                                {plugin.verified ? (
                                  <Badge
                                    variant="outline"
                                    className="gap-1 border-emerald-500/30 bg-emerald-500/10 text-[10px] font-normal text-emerald-700 dark:text-emerald-300"
                                  >
                                    <ShieldCheck className="size-2.5" />
                                    Verified
                                  </Badge>
                                ) : (
                                  <Badge
                                    variant="outline"
                                    className="gap-1 border-amber-500/30 bg-amber-500/10 text-[10px] font-normal text-amber-700 dark:text-amber-300"
                                  >
                                    <ShieldAlert className="size-2.5" />
                                    Local
                                  </Badge>
                                )}
                                {plugin.isInstalled && (
                                  <Badge
                                    variant={
                                      plugin.enabled ? "secondary" : "outline"
                                    }
                                    className="text-[10px] font-normal"
                                  >
                                    {plugin.enabled ? "Installed" : "Disabled"}
                                  </Badge>
                                )}
                              </div>

                              {(plugin.authors.length > 0 || updatedLabel) && (
                                <p className="truncate text-[11px] text-muted-foreground">
                                  {plugin.authors.length > 0
                                    ? `by ${plugin.authors.join(", ")}`
                                    : ""}
                                  {plugin.authors.length > 0 && updatedLabel
                                    ? " · "
                                    : ""}
                                  {updatedLabel ? `Updated ${updatedLabel}` : ""}
                                </p>
                              )}
                            </div>
                          </div>

                          {plugin.description && (
                            <p className="line-clamp-2 text-xs text-muted-foreground">
                              {plugin.description}
                            </p>
                          )}

                          {developerMode && plugin.isInstalled && (
                            <div className="space-y-1 rounded-md border border-border/60 bg-muted/30 px-2.5 py-1.5 text-[11px]">
                              <p className="truncate font-mono text-muted-foreground">
                                id: {plugin.id}
                              </p>
                              <p className="truncate text-muted-foreground">
                                domains:{" "}
                                {plugin.allowedDomains.length > 0
                                  ? plugin.allowedDomains.join(", ")
                                  : "none"}
                              </p>
                            </div>
                          )}
                        </div>

                        <div className="flex items-center justify-between gap-2 border-t border-border/60 pt-2.5">
                          <span className="inline-flex items-center gap-1 text-[11px] text-muted-foreground">
                            View manifest
                            <ArrowRight className="size-3" />
                          </span>

                          {plugin.isInstalled && !plugin.updateAvailable ? (
                            <Button
                              type="button"
                              size="sm"
                              variant="outline"
                              disabled
                              className="h-7 px-2.5 text-xs"
                              onClick={(e) => e.stopPropagation()}
                            >
                              <Check className="mr-1 size-3" />
                              Installed
                            </Button>
                          ) : plugin.source === "registry" ? (
                            <Button
                              type="button"
                              size="sm"
                              variant={
                                plugin.updateAvailable ? "default" : "secondary"
                              }
                              className="h-7 px-2.5 text-xs"
                              disabled={isBusy || !plugin.isCompatible}
                              title={
                                !plugin.isCompatible
                                  ? (plugin.compatibilityError ??
                                    `Requires SocialPoster ${plugin.compatibleAppVersion}`)
                                  : undefined
                              }
                              onClick={(e) => {
                                e.stopPropagation();
                                void handleInstallOrUpdate(
                                  plugin.id,
                                  plugin.name,
                                  plugin.latestVersion,
                                  plugin.updateAvailable,
                                );
                              }}
                            >
                              {isBusy ? (
                                <Loader2 className="mr-1 size-3 animate-spin" />
                              ) : (
                                <Download className="mr-1 size-3" />
                              )}
                              {plugin.updateAvailable ? "Update" : "Install"}
                            </Button>
                          ) : null}
                        </div>
                      </article>
                    );
                  })}
                </div>
              )}
            </div>

            {filteredPlugins.length > PAGE_SIZE && (
              <div className="flex shrink-0 items-center justify-between border-t border-border/60 bg-muted/40 px-5 py-3 text-xs">
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  className="h-7 px-2.5 text-xs"
                  disabled={safePage <= 1}
                  onClick={() => setPage((p) => Math.max(1, p - 1))}
                >
                  <ChevronLeft className="mr-1 size-3.5" />
                  Previous
                </Button>
                <span className="text-muted-foreground">
                  Page {safePage} of {totalPages}
                </span>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  className="h-7 px-2.5 text-xs"
                  disabled={safePage >= totalPages}
                  onClick={() => setPage((p) => Math.min(totalPages, p + 1))}
                >
                  Next
                  <ChevronRight className="ml-1 size-3.5" />
                </Button>
              </div>
            )}
          </>
        )}
      </DialogContent>
    </Dialog>

    {manifestDetail && confirmUpdateVersion && (
      <Dialog
        open={confirmUpdateVersion !== null}
        onOpenChange={(nextOpen) => {
          if (!nextOpen && !mutatingPluginIds.has(manifestDetail.id)) {
            setConfirmUpdateVersion(null);
          }
        }}
      >
        <DialogContent
          showCloseButton={false}
          className="flex max-w-lg flex-col gap-0 overflow-hidden border-[#27272a] bg-[#111113] p-0 text-foreground sm:max-w-lg"
        >
          <DialogHeader className="border-b border-[#27272a] p-5">
            <div className="flex items-center justify-between gap-4">
              <DialogTitle className="text-base">
                Confirm Plugin Update
              </DialogTitle>
              <DialogClose asChild>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-sm"
                  disabled={mutatingPluginIds.has(manifestDetail.id)}
                  aria-label="Close update confirmation dialogue"
                >
                  <X className="size-4" />
                </Button>
              </DialogClose>
            </div>
          </DialogHeader>

          <div className="space-y-4 p-5 text-xs">
            <div className="flex items-center justify-between gap-3 rounded-lg border border-[#27272a] bg-[#18181b] px-3.5 py-2.5">
              <span className="font-medium text-foreground">
                {manifestDetail.name}
              </span>
              <span className="font-mono text-foreground">
                v{manifestDetail.installedVersion ?? "0.0.0"} → v
                {confirmUpdateVersion}
              </span>
            </div>

            <div className="grid grid-cols-2 gap-2.5">
              <div className="rounded-lg border border-[#27272a] bg-[#18181b]/60 px-3 py-2">
                <p className="text-[11px] text-muted-foreground">Binary size</p>
                <p className="mt-0.5 font-medium text-foreground">
                  {formatBytesKb(confirmVersionMeta?.file_size_bytes)}
                </p>
              </div>
              <div className="rounded-lg border border-[#27272a] bg-[#18181b]/60 px-3 py-2">
                <p className="text-[11px] text-muted-foreground">
                  Signature
                </p>
                <p className="mt-0.5 flex items-center gap-1 font-medium text-foreground">
                  {confirmVersionMeta?.ed25519_signature?.trim() ? (
                    <>
                      <ShieldCheck className="size-3.5 text-emerald-400" />
                      <span className="text-emerald-400">Verified</span>
                    </>
                  ) : (
                    <>
                      <ShieldAlert className="size-3.5 text-amber-400" />
                      <span className="text-amber-400">Unsigned</span>
                    </>
                  )}
                </p>
              </div>
            </div>

            <div className="rounded-lg border border-[#27272a] bg-[#18181b]/60 px-3 py-2">
              <p className="text-[11px] text-muted-foreground">
                Compatibility (SocialPoster v{manifestDetail.appVersion})
              </p>
              <p className="mt-0.5 flex items-center gap-1.5 font-medium">
                {confirmIsCompatible ? (
                  <>
                    <Check className="size-3.5 text-emerald-400" />
                    <span className="text-emerald-400">
                      Compatible (requires{" "}
                      {confirmVersionMeta?.compatible_app_version ??
                        manifestDetail.compatibleAppVersion}
                      )
                    </span>
                  </>
                ) : (
                  <>
                    <AlertTriangle className="size-3.5 text-amber-400" />
                    <span className="text-amber-400">
                      Incompatible (requires{" "}
                      {confirmVersionMeta?.compatible_app_version ??
                        manifestDetail.compatibleAppVersion}
                      )
                    </span>
                  </>
                )}
              </p>
            </div>

            <div className="space-y-1.5">
              <p className="text-[11px] font-medium text-muted-foreground">
                Release notes for v{confirmUpdateVersion}
              </p>
              <div className="max-h-36 overflow-y-auto rounded-lg border border-[#27272a] bg-[#18181b]/60 p-3 text-xs leading-relaxed text-foreground/90">
                {confirmVersionMeta?.release_notes?.trim() ? (
                  <ReactMarkdown>
                    {confirmVersionMeta.release_notes}
                  </ReactMarkdown>
                ) : (
                  <span className="text-muted-foreground">
                    No release notes provided for this version.
                  </span>
                )}
              </div>
            </div>
          </div>

          <div className="flex items-center justify-end gap-2 border-t border-[#27272a] bg-[#18181b]/40 px-5 py-3.5">
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={mutatingPluginIds.has(manifestDetail.id)}
              onClick={() => setConfirmUpdateVersion(null)}
            >
              Cancel
            </Button>
            <Button
              type="button"
              size="sm"
              disabled={
                mutatingPluginIds.has(manifestDetail.id) || !confirmIsCompatible
              }
              onClick={async () => {
                await handleInstallOrUpdate(
                  manifestDetail.id,
                  manifestDetail.name,
                  confirmUpdateVersion,
                  true,
                );
                setConfirmUpdateVersion(null);
              }}
            >
              {mutatingPluginIds.has(manifestDetail.id) ? (
                <Loader2 className="mr-1.5 size-3.5 animate-spin" />
              ) : (
                <Download className="mr-1.5 size-3.5" />
              )}
              Confirm Update
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    )}
    </>
  );
}
