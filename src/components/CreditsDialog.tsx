import { useEffect, useMemo, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  Check,
  Download,
  ExternalLink,
  Loader2,
  RefreshCw,
  RotateCw,
  X,
} from "lucide-react";
import { toast } from "sonner";
import { AppLogo } from "@/components/AppLogo";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { ScrollArea } from "@/components/ui/scroll-area";
import type { AuthorLink, PlatformMetadata } from "@/lib/tauri";
import {
  checkForUpdateManual,
  downloadAndInstallUpdate,
  relaunchApp,
  type Update,
  type UpdateStatus,
} from "@/lib/updater";

interface CreditsDialogProps {
  platforms: PlatformMetadata[];
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

interface ContributorEntry {
  name: string;
  links: AuthorLink[];
  platforms: string[];
}

export function CreditsDialog({
  platforms,
  open,
  onOpenChange,
}: CreditsDialogProps) {
  const [appVersion, setAppVersion] = useState<string>("");
  const [updateStatus, setUpdateStatus] = useState<UpdateStatus>("idle");
  const [foundUpdate, setFoundUpdate] = useState<Update | null>(null);
  const [downloadPercent, setDownloadPercent] = useState<number | null>(null);

  useEffect(() => {
    let active = true;
    getVersion()
      .then((ver) => {
        if (active) setAppVersion(ver);
      })
      .catch(() => {
        // Ignore when running outside Tauri shell.
      });
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    if (updateStatus !== "up-to-date") return;
    const timer = window.setTimeout(() => {
      setUpdateStatus("idle");
    }, 3000);
    return () => window.clearTimeout(timer);
  }, [updateStatus]);

  const contributors = useMemo<ContributorEntry[]>(() => {
    const map = new Map<string, ContributorEntry>();

    for (const platform of platforms) {
      for (const author of platform.authors) {
        const key = author.name.trim().toLowerCase();
        if (!key) continue;

        const existing = map.get(key);
        if (existing) {
          if (!existing.platforms.includes(platform.name)) {
            existing.platforms.push(platform.name);
          }
          for (const link of author.links) {
            if (!existing.links.some((l) => l.url === link.url)) {
              existing.links.push(link);
            }
          }
        } else {
          map.set(key, {
            name: author.name,
            links: [...author.links],
            platforms: [platform.name],
          });
        }
      }
    }

    return Array.from(map.values()).sort((a, b) =>
      a.name.localeCompare(b.name),
    );
  }, [platforms]);

  const handleOpenUrl = async (url: string) => {
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
        setFoundUpdate(null);
        setUpdateStatus("up-to-date");
      }
    } catch (err) {
      setUpdateStatus("idle");
      toast.error(`Update check failed: ${String(err)}`);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent showCloseButton={false} className="sm:max-w-lg">
        <DialogHeader>
          <div className="flex items-center justify-between gap-4">
            <DialogTitle className="flex items-center gap-2.5">
              <AppLogo className="h-8 w-8 shrink-0" />
              <span>Credits</span>
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
          <DialogDescription>
            Platform integrations maintained by:
          </DialogDescription>
        </DialogHeader>

        <ScrollArea className="max-h-72 pr-2">
          {contributors.length === 0 ? (
            <p className="py-6 text-center text-sm text-muted-foreground">
              No contributor credits listed.
            </p>
          ) : (
            <ul className="space-y-2.5">
              {contributors.map((entry) => {
                const httpsLinks = entry.links.filter((l) =>
                  l.url.startsWith("https://"),
                );

                return (
                  <li
                    key={entry.name}
                    className="flex flex-col gap-2 rounded-lg border bg-card p-3"
                  >
                    <div className="flex flex-wrap items-center justify-between gap-2">
                      <p className="text-sm font-medium">{entry.name}</p>
                      {httpsLinks.length > 0 && (
                        <div className="flex flex-wrap gap-1.5">
                          {httpsLinks.map((link) => (
                            <button
                              key={link.url}
                              type="button"
                              onClick={() => void handleOpenUrl(link.url)}
                              className="inline-flex cursor-pointer items-center gap-1 rounded border bg-background px-2 py-0.5 text-xs text-muted-foreground transition-colors hover:text-foreground"
                            >
                              <span>{link.label}</span>
                              <ExternalLink className="size-3" />
                            </button>
                          ))}
                        </div>
                      )}
                    </div>

                    <div className="flex flex-wrap gap-1">
                      {entry.platforms.map((name) => (
                        <Badge
                          key={name}
                          variant="secondary"
                          className="text-[11px] font-normal"
                        >
                          {name}
                        </Badge>
                      ))}
                    </div>
                  </li>
                );
              })}
            </ul>
          )}
        </ScrollArea>

        <DialogFooter className="flex-row items-center justify-between gap-2 sm:justify-between">
          <div className="flex items-center gap-2">
            {appVersion && (
              <span className="text-xs text-muted-foreground">
                v{appVersion}
              </span>
            )}
            <Button
              type="button"
              variant="ghost"
              size="sm"
              className="h-7 px-2 text-xs"
              onClick={() => void handleUpdateAction()}
              disabled={
                updateStatus === "checking" || updateStatus === "downloading"
              }
            >
              {updateStatus === "checking" ? (
                <>
                  <Loader2 className="mr-1.5 size-3.5 animate-spin" />
                  Checking...
                </>
              ) : updateStatus === "downloading" ? (
                <>
                  <Loader2 className="mr-1.5 size-3.5 animate-spin" />
                  {downloadPercent !== null
                    ? `Downloading ${downloadPercent}%`
                    : "Downloading..."}
                </>
              ) : updateStatus === "available" && foundUpdate ? (
                <>
                  <Download className="mr-1.5 size-3.5" />
                  Install v{foundUpdate.version}
                </>
              ) : updateStatus === "ready" ? (
                <>
                  <RotateCw className="mr-1.5 size-3.5" />
                  Restart to update
                </>
              ) : updateStatus === "up-to-date" ? (
                <>
                  <Check className="mr-1.5 size-3.5" />
                  Up to date
                </>
              ) : (
                <>
                  <RefreshCw className="mr-1.5 size-3.5" />
                  Check for updates
                </>
              )}
            </Button>
          </div>

          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() => onOpenChange(false)}
          >
            Close
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
