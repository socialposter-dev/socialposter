import { useEffect, useMemo, useState } from "react";
import {
  Clock,
  Image as ImageIcon,
  Loader2,
  RotateCcw,
  Send,
  Trash2,
  X,
} from "lucide-react";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import {
  getDesktopSettings,
  updateDesktopSettings,
  type DesktopSettings,
  type PlatformMetadata,
  type ScheduledPost,
} from "@/lib/tauri";
import { cn } from "@/lib/utils";

interface QueueDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  posts: ScheduledPost[];
  platforms: PlatformMetadata[];
  onPublishNow: (jobId: string) => Promise<void>;
  onRetry: (jobId: string) => Promise<void>;
  onCancel: (jobId: string) => Promise<void>;
}

const MODIFIER_KEYS = new Set([
  "Control",
  "Shift",
  "Alt",
  "Meta",
  "OS",
  "Super",
  "Dead",
  "Unidentified",
]);

function isMacOS(): boolean {
  if (typeof navigator === "undefined") return false;
  return /Mac|iPod|iPhone|iPad/i.test(navigator.platform || navigator.userAgent);
}

function formatShortcutDisplay(shortcut: string | null | undefined): string {
  if (!shortcut) return "Not set";
  const mac = isMacOS();
  return shortcut
    .split("+")
    .map((part) => part.trim())
    .filter(Boolean)
    .map((part) => {
      switch (part.toLowerCase()) {
        case "commandorcontrol":
        case "cmdorctrl":
          return mac ? "⌘" : "Ctrl";
        case "command":
        case "cmd":
        case "super":
          return mac ? "⌘" : "Win";
        case "control":
        case "ctrl":
          return mac ? "⌃" : "Ctrl";
        case "alt":
        case "option":
          return mac ? "⌥" : "Alt";
        case "shift":
          return "Shift";
        default:
          return part;
      }
    })
    .join(" + ");
}

function normalizePrimaryKey(e: React.KeyboardEvent): string | null {
  if (e.code === "Space" || e.key === " ") return "Space";
  if (e.code.startsWith("Key")) return e.code.slice(3).toUpperCase();
  if (e.code.startsWith("Digit")) return e.code.slice(5);
  if (/^F([1-9]|1[0-2])$/i.test(e.key)) return e.key.toUpperCase();

  switch (e.key) {
    case "ArrowUp":
      return "Up";
    case "ArrowDown":
      return "Down";
    case "ArrowLeft":
      return "Left";
    case "ArrowRight":
      return "Right";
    case "Enter":
    case "Tab":
    case "Backspace":
    case "Delete":
    case "Home":
    case "End":
    case "PageUp":
    case "PageDown":
      return e.key;
    default:
      return e.key.length === 1 ? e.key.toUpperCase() : null;
  }
}

function keyEventToTauriShortcut(e: React.KeyboardEvent): string | null {
  if (MODIFIER_KEYS.has(e.key)) return null;

  const modifiers: string[] = [];
  if (e.ctrlKey || e.metaKey) modifiers.push("CommandOrControl");
  if (e.altKey) modifiers.push("Alt");
  if (e.shiftKey) modifiers.push("Shift");

  const primary = normalizePrimaryKey(e);
  if (!primary) return null;

  const isFunctionKey = /^F([1-9]|1[0-2])$/i.test(primary);
  if (modifiers.length === 0 && !isFunctionKey) return null;

  return [...modifiers, primary].join("+");
}

function formatScheduledTime(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  return date.toLocaleString(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  });
}

export function QueueDialog({
  open,
  onOpenChange,
  posts,
  platforms,
  onPublishNow,
  onRetry,
  onCancel,
}: QueueDialogProps) {
  const [busyId, setBusyId] = useState<string | null>(null);
  const [settings, setSettings] = useState<DesktopSettings>({
    minimizeToTray: true,
    globalShortcut: "CommandOrControl+Shift+Space",
    developerMode: false,
    launchAtStartup: false,
  });
  const [recordingShortcut, setRecordingShortcut] = useState(false);
  const [savingSettings, setSavingSettings] = useState(false);

  const platformMap = useMemo(
    () => new Map(platforms.map((p) => [p.id, p])),
    [platforms],
  );

  useEffect(() => {
    if (!open) {
      setRecordingShortcut(false);
      return;
    }
    let active = true;
    void getDesktopSettings()
      .then((loaded) => {
        if (!active) return;
        setSettings(loaded);
      })
      .catch(() => {
        // Ignore when running outside Tauri shell.
      });
    return () => {
      active = false;
    };
  }, [open]);

  const handleToggleTray = async (checked: boolean) => {
    const next: DesktopSettings = { ...settings, minimizeToTray: checked };
    setSavingSettings(true);
    try {
      const saved = await updateDesktopSettings(next);
      setSettings(saved);
    } catch (err) {
      toast.error(`Failed to update tray setting: ${String(err)}`);
    } finally {
      setSavingSettings(false);
    }
  };

  const persistShortcut = async (shortcut: string | null) => {
    const next: DesktopSettings = {
      ...settings,
      globalShortcut: shortcut,
    };
    setSavingSettings(true);
    try {
      const saved = await updateDesktopSettings(next);
      setSettings(saved);
      toast.success(shortcut ? "Shortcut updated." : "Shortcut cleared.");
    } catch (err) {
      toast.error(String(err));
    } finally {
      setSavingSettings(false);
    }
  };

  const handleRecorderKeyDown = (e: React.KeyboardEvent<HTMLButtonElement>) => {
    if (!recordingShortcut) return;
    e.preventDefault();
    e.stopPropagation();

    if (e.key === "Escape") {
      setRecordingShortcut(false);
      return;
    }

    const accelerator = keyEventToTauriShortcut(e);
    if (!accelerator) return;

    setRecordingShortcut(false);
    void persistShortcut(accelerator);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        showCloseButton={false}
        className="w-full min-w-0 max-w-xl gap-0 overflow-hidden p-0 sm:max-w-xl"
      >
        <DialogHeader className="border-b border-border/60 p-5">
          <div className="flex items-center justify-between gap-4">
            <DialogTitle className="flex items-center gap-2 text-base">
              <Clock className="size-4 text-primary" />
              <span>Scheduled Queue</span>
              <Badge variant="secondary" className="text-xs font-normal">
                {posts.length}
              </Badge>
            </DialogTitle>
            <div className="flex items-center gap-2">
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

        <div className="w-full min-w-0 overflow-hidden p-5">
          {posts.length === 0 ? (
            <p className="py-8 text-center text-sm text-muted-foreground">
              No scheduled posts.
            </p>
          ) : (
            <div className="max-h-[320px] w-full min-w-0 space-y-3 overflow-x-hidden overflow-y-auto pr-1">
              {posts.map((job) => {
                const isBusy = busyId === job.id || job.status === "publishing";
                const preview = job.text.trim();

                return (
                  <article
                    key={job.id}
                    className="w-full min-w-0 space-y-2.5 overflow-hidden rounded-xl border border-border/60 bg-card p-3.5 shadow-2xs"
                  >
                    <div className="flex flex-wrap items-center justify-between gap-2 text-xs">
                      <span className="inline-flex items-center gap-1.5 font-medium text-foreground">
                        <Clock className="size-3.5 text-muted-foreground" />
                        {formatScheduledTime(job.scheduledFor)}
                      </span>

                      <div className="flex items-center gap-1.5">
                        {job.mediaRefs.length > 0 && (
                          <Badge
                            variant="outline"
                            className="gap-1 text-[11px] font-normal"
                          >
                            <ImageIcon className="size-3" />
                            {job.mediaRefs.length}
                          </Badge>
                        )}
                        <Badge
                          variant={
                            job.status === "failed"
                              ? "destructive"
                              : "secondary"
                          }
                          className="text-[11px] font-normal capitalize"
                        >
                          {job.status}
                        </Badge>
                      </div>
                    </div>

                    <p className="line-clamp-2 text-sm break-words [overflow-wrap:anywhere] whitespace-pre-wrap text-foreground">
                      {preview.length > 0 ? (
                        preview
                      ) : (
                        <span className="italic text-muted-foreground">
                          (Media post)
                        </span>
                      )}
                    </p>

                    {job.error && (
                      <p className="text-xs break-words [overflow-wrap:anywhere] text-destructive">
                        {job.error}
                      </p>
                    )}

                    <div className="flex flex-wrap items-center justify-between gap-2 pt-1">
                      <div className="flex flex-wrap items-center gap-1.5">
                        {job.platformIds.map((pid) => {
                          const meta = platformMap.get(pid);
                          return (
                            <span
                              key={pid}
                              className="inline-flex items-center gap-1 rounded border border-border/60 bg-muted/40 px-1.5 py-0.5 text-[11px]"
                            >
                              <span
                                className="size-2 rounded-full"
                                style={{
                                  backgroundColor:
                                    meta?.accentColour ?? "#64748B",
                                }}
                              />
                              <span>{meta?.name ?? pid}</span>
                            </span>
                          );
                        })}
                      </div>

                      <div className="flex items-center gap-1.5">
                        {job.status === "failed" && (
                          <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            className="h-7 px-2.5 text-xs"
                            disabled={isBusy}
                            onClick={() => {
                              setBusyId(job.id);
                              void onRetry(job.id).finally(() =>
                                setBusyId(null),
                              );
                            }}
                          >
                            <RotateCcw className="mr-1 size-3" />
                            Retry
                          </Button>
                        )}
                        <Button
                          type="button"
                          variant="secondary"
                          size="sm"
                          className="h-7 px-2.5 text-xs"
                          disabled={isBusy}
                          onClick={() => {
                            setBusyId(job.id);
                            void onPublishNow(job.id).finally(() =>
                              setBusyId(null),
                            );
                          }}
                        >
                          {isBusy ? (
                            <Loader2 className="mr-1 size-3 animate-spin" />
                          ) : (
                            <Send className="mr-1 size-3" />
                          )}
                          Post Now
                        </Button>
                        <Button
                          type="button"
                          variant="ghost"
                          size="sm"
                          className="h-7 px-2 text-xs text-muted-foreground hover:text-destructive"
                          disabled={isBusy}
                          onClick={() => {
                            setBusyId(job.id);
                            void onCancel(job.id).finally(() => setBusyId(null));
                          }}
                        >
                          <Trash2 className="mr-1 size-3" />
                          Cancel
                        </Button>
                      </div>
                    </div>
                  </article>
                );
              })}
            </div>
          )}
        </div>

        <div className="w-full min-w-0 space-y-3 overflow-hidden border-t border-border/60 bg-muted/40 p-5">
          <div className="flex w-full min-w-0 items-center justify-between gap-4">
            <Label htmlFor="minimize-to-tray" className="text-xs font-medium">
              Minimise to system tray on close
            </Label>
            <Switch
              id="minimize-to-tray"
              className="shrink-0"
              checked={settings.minimizeToTray}
              onCheckedChange={(checked) => void handleToggleTray(checked)}
              disabled={savingSettings}
            />
          </div>

          <div className="flex w-full min-w-0 items-center justify-between gap-4">
            <Label className="text-xs font-medium">
              Quick-Compose Shortcut
            </Label>
            <div className="flex shrink-0 items-center gap-1.5">
              <button
                type="button"
                disabled={savingSettings}
                onClick={() => setRecordingShortcut((prev) => !prev)}
                onBlur={() => setRecordingShortcut(false)}
                onKeyDown={handleRecorderKeyDown}
                className={cn(
                  "inline-flex h-8 min-w-[180px] cursor-pointer items-center justify-center rounded-lg border px-3 text-xs font-medium transition-colors focus:outline-none",
                  recordingShortcut
                    ? "border-primary bg-primary/10 text-primary"
                    : "border-border bg-background text-foreground hover:bg-accent/50",
                  savingSettings && "cursor-not-allowed opacity-50",
                )}
              >
                {recordingShortcut
                  ? "Press shortcut..."
                  : formatShortcutDisplay(settings.globalShortcut)}
              </button>

              {settings.globalShortcut && (
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-sm"
                  className="h-8 w-8 shrink-0 text-muted-foreground hover:text-foreground"
                  disabled={savingSettings}
                  onClick={() => {
                    setRecordingShortcut(false);
                    void persistShortcut(null);
                  }}
                  aria-label="Clear shortcut"
                  title="Clear shortcut"
                >
                  <X className="size-3.5" />
                </Button>
              )}
            </div>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
