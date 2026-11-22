import { useEffect, useMemo, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  CheckCircle2,
  Clock,
  ExternalLink,
  History,
  Image as ImageIcon,
  Loader2,
  RotateCcw,
  Send,
  Trash2,
  X,
  XCircle,
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
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import type {
  HistoryEntry,
  PlatformMetadata,
  ScheduledPost,
} from "@/lib/tauri";

interface QueueHistoryDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  posts: ScheduledPost[];
  history: HistoryEntry[];
  platforms: PlatformMetadata[];
  onPublishNow: (jobId: string) => Promise<void>;
  onRetry: (jobId: string) => Promise<void>;
  onCancel: (jobId: string) => Promise<void>;
  onLoadIntoComposer: (entry: HistoryEntry, onlyFailed?: boolean) => void;
  onClearHistory: () => void;
  onRecallPost: (historyId: string) => Promise<void>;
}

function formatTimestamp(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  return date.toLocaleString(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  });
}

export function QueueHistoryDialog({
  open,
  onOpenChange,
  posts,
  history,
  platforms,
  onPublishNow,
  onRetry,
  onCancel,
  onLoadIntoComposer,
  onClearHistory,
  onRecallPost,
}: QueueHistoryDialogProps) {
  const [activeTab, setActiveTab] = useState<"queue" | "history">("queue");
  const [busyJobId, setBusyJobId] = useState<string | null>(null);
  const [confirmingHistoryId, setConfirmingHistoryId] = useState<string | null>(
    null,
  );
  const [recallingId, setRecallingId] = useState<string | null>(null);

  const platformMap = useMemo(
    () => new Map(platforms.map((p) => [p.id, p])),
    [platforms],
  );

  const pendingCount = useMemo(
    () => posts.filter((p) => p.status === "pending").length,
    [posts],
  );

  useEffect(() => {
    if (!confirmingHistoryId) return;
    const timer = window.setTimeout(() => {
      setConfirmingHistoryId(null);
    }, 4000);
    return () => window.clearTimeout(timer);
  }, [confirmingHistoryId]);

  useEffect(() => {
    if (!open) {
      setConfirmingHistoryId(null);
      setBusyJobId(null);
    }
  }, [open]);

  const handleJobAction = async (
    jobId: string,
    fn: (id: string) => Promise<void>,
  ) => {
    setBusyJobId(jobId);
    try {
      await fn(jobId);
    } finally {
      setBusyJobId(null);
    }
  };

  const handleOpenPost = async (url: string) => {
    if (!url.startsWith("https://") && !url.startsWith("http://")) return;
    try {
      await openUrl(url);
    } catch (err) {
      toast.error(`Unable to open post URL: ${String(err)}`);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        showCloseButton={false}
        className="flex h-[640px] max-h-[85vh] w-full min-w-0 max-w-3xl flex-col gap-0 overflow-hidden rounded-2xl p-0 sm:max-w-3xl"
      >
        <DialogHeader className="shrink-0 border-b border-border/60 px-6 py-4">
          <div className="flex items-center justify-between gap-4">
            <DialogTitle className="flex items-center gap-2 text-base">
              <Clock className="size-4 text-primary" />
              <span>Queue &amp; History</span>
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

        <Tabs
          value={activeTab}
          onValueChange={(v) => setActiveTab(v as "queue" | "history")}
          className="flex min-h-0 flex-1 flex-col gap-0"
        >
          <div className="flex shrink-0 items-center justify-between gap-3 border-b border-border/60 bg-muted/20 px-6 py-2.5">
            <TabsList>
              <TabsTrigger value="queue" className="gap-1.5 text-xs">
                <Clock className="size-3.5" />
                <span>Scheduled Queue</span>
                {pendingCount > 0 && (
                  <Badge
                    variant="secondary"
                    className="h-4 px-1.5 text-[10px] font-normal"
                  >
                    {pendingCount}
                  </Badge>
                )}
              </TabsTrigger>
              <TabsTrigger value="history" className="gap-1.5 text-xs">
                <History className="size-3.5" />
                <span>Post History</span>
                {history.length > 0 && (
                  <Badge
                    variant="secondary"
                    className="h-4 px-1.5 text-[10px] font-normal"
                  >
                    {history.length}
                  </Badge>
                )}
              </TabsTrigger>
            </TabsList>

            {activeTab === "history" && history.length > 0 && (
              <Button
                type="button"
                variant="ghost"
                size="sm"
                className="h-7 px-2 text-xs text-muted-foreground hover:text-destructive"
                onClick={onClearHistory}
              >
                <Trash2 className="mr-1 size-3.5" />
                Clear History
              </Button>
            )}
          </div>

          <TabsContent
            value="queue"
            className="min-h-0 flex-1 overflow-y-auto p-6"
          >
            {posts.length === 0 ? (
              <div className="flex flex-col items-center justify-center gap-2 py-14 text-center text-sm text-muted-foreground">
                <Clock className="size-8 opacity-40" />
                <p>No scheduled posts.</p>
              </div>
            ) : (
              <div className="space-y-3">
                {posts.map((job) => {
                  const isBusy =
                    busyJobId === job.id || job.status === "publishing";
                  const preview = job.text.trim();

                  return (
                    <article
                      key={job.id}
                      className="w-full min-w-0 space-y-2.5 overflow-hidden rounded-xl border border-border/60 bg-card p-3.5 shadow-2xs"
                    >
                      <div className="flex flex-wrap items-center justify-between gap-2 text-xs">
                        <span className="inline-flex items-center gap-1.5 font-medium text-foreground">
                          <Clock className="size-3.5 text-muted-foreground" />
                          {formatTimestamp(job.scheduledFor)}
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
                              className="h-7 px-2 text-xs"
                              disabled={isBusy}
                              onClick={() => void handleJobAction(job.id, onRetry)}
                            >
                              {isBusy ? (
                                <Loader2 className="mr-1 size-3 animate-spin" />
                              ) : (
                                <RotateCcw className="mr-1 size-3" />
                              )}
                              Retry
                            </Button>
                          )}

                          <Button
                            type="button"
                            variant="secondary"
                            size="sm"
                            className="h-7 px-2 text-xs"
                            disabled={isBusy}
                            onClick={() =>
                              void handleJobAction(job.id, onPublishNow)
                            }
                          >
                            {isBusy ? (
                              <Loader2 className="mr-1 size-3 animate-spin" />
                            ) : (
                              <Send className="mr-1 size-3" />
                            )}
                            Publish Now
                          </Button>

                          <Button
                            type="button"
                            variant="ghost"
                            size="icon-sm"
                            className="size-7 text-muted-foreground hover:text-destructive"
                            disabled={isBusy}
                            aria-label="Cancel scheduled post"
                            onClick={() =>
                              void handleJobAction(job.id, onCancel)
                            }
                          >
                            <Trash2 className="size-3.5" />
                          </Button>
                        </div>
                      </div>
                    </article>
                  );
                })}
              </div>
            )}
          </TabsContent>

          <TabsContent
            value="history"
            className="min-h-0 flex-1 overflow-y-auto p-6"
          >
            {history.length === 0 ? (
              <div className="flex flex-col items-center justify-center gap-2 py-14 text-center text-sm text-muted-foreground">
                <History className="size-8 opacity-40" />
                <p>No posts yet.</p>
              </div>
            ) : (
              <div className="space-y-3">
                {history.map((entry) => {
                  const hasFailures = entry.outcomes.some((o) => !o.success);
                  const hasSuccesses = entry.outcomes.some((o) => o.success);
                  const isPartialFailure = hasFailures && hasSuccesses;
                  const canRecall = entry.outcomes.some((o) => {
                    if (!o.success || o.recalled) return false;
                    const hasIds =
                      (o.postIds?.length ?? 0) > 0 || Boolean(o.postId);
                    const supportsDelete = Boolean(
                      platformMap.get(o.platformId)?.capabilities
                        .supportsDelete,
                    );
                    return hasIds && supportsDelete;
                  });
                  const recallErrors = entry.outcomes.filter((o) =>
                    Boolean(o.recallError),
                  );
                  const isConfirming = confirmingHistoryId === entry.id;
                  const isRecalling = recallingId === entry.id;
                  const preview = entry.masterText.trim();

                  return (
                    <article
                      key={entry.id}
                      className="w-full min-w-0 space-y-3 overflow-hidden rounded-xl border border-border/60 bg-card p-3.5 shadow-2xs"
                    >
                      <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
                        <span className="inline-flex items-center gap-1.5">
                          <Clock className="size-3.5" />
                          {formatTimestamp(entry.createdAt)}
                        </span>

                        {entry.media.length > 0 && (
                          <Badge
                            variant="outline"
                            className="gap-1 text-[11px] font-normal"
                          >
                            <ImageIcon className="size-3" />
                            {entry.media.length} attachment
                            {entry.media.length === 1 ? "" : "s"}
                          </Badge>
                        )}
                      </div>

                      <p className="line-clamp-3 w-full min-w-0 whitespace-pre-wrap [overflow-wrap:anywhere] text-sm leading-relaxed text-foreground">
                        {preview.length > 0 ? (
                          preview
                        ) : (
                          <span className="italic text-muted-foreground">
                            (No master text)
                          </span>
                        )}
                      </p>

                      <div className="flex flex-wrap gap-1.5">
                        {entry.outcomes.map((outcome) =>
                          outcome.recalled ? (
                            <Badge
                              key={outcome.platformId}
                              variant="outline"
                              className="gap-1 border-border bg-muted/60 text-xs font-normal text-muted-foreground"
                            >
                              <span className="line-through">
                                {outcome.platformName}
                              </span>
                              <span>Recalled</span>
                            </Badge>
                          ) : outcome.success ? (
                            outcome.postUrl ? (
                              <button
                                key={outcome.platformId}
                                type="button"
                                onClick={() =>
                                  void handleOpenPost(outcome.postUrl!)
                                }
                                title={`Open ${outcome.platformName} post`}
                                className="inline-flex cursor-pointer items-center gap-1 rounded-md border border-emerald-500/30 bg-emerald-500/10 px-2 py-0.5 text-xs font-medium text-emerald-700 transition-colors hover:bg-emerald-500/20 dark:text-emerald-300"
                              >
                                <CheckCircle2 className="size-3" />
                                <span>{outcome.platformName}</span>
                                <ExternalLink className="size-2.5" />
                              </button>
                            ) : (
                              <Badge
                                key={outcome.platformId}
                                variant="outline"
                                className="gap-1 border-emerald-500/30 bg-emerald-500/10 text-xs font-medium text-emerald-700 dark:text-emerald-300"
                              >
                                <CheckCircle2 className="size-3" />
                                <span>{outcome.platformName}</span>
                              </Badge>
                            )
                          ) : (
                            <Badge
                              key={outcome.platformId}
                              variant="outline"
                              title={outcome.error ?? "Publish failed"}
                              className="max-w-full gap-1 border-destructive/30 bg-destructive/10 text-xs font-normal text-destructive"
                            >
                              <XCircle className="size-3 shrink-0" />
                              <span className="font-medium">
                                {outcome.platformName}
                              </span>
                              {outcome.error && (
                                <span className="truncate opacity-85">
                                  · {outcome.error}
                                </span>
                              )}
                            </Badge>
                          ),
                        )}
                      </div>

                      {recallErrors.length > 0 && (
                        <div className="space-y-1">
                          {recallErrors.map((outcome) => (
                            <p
                              key={`${outcome.platformId}-recall-err`}
                              className="text-xs break-words [overflow-wrap:anywhere] text-destructive"
                            >
                              {outcome.platformName}: {outcome.recallError}
                            </p>
                          ))}
                        </div>
                      )}

                      <div className="flex flex-wrap items-center justify-end gap-2 border-t border-border/60 pt-2.5">
                        {isPartialFailure && (
                          <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            className="h-7 px-2.5 text-xs"
                            onClick={() => {
                              onLoadIntoComposer(entry, true);
                              onOpenChange(false);
                            }}
                          >
                            <RotateCcw className="mr-1 size-3" />
                            Retry Failed
                          </Button>
                        )}
                        <Button
                          type="button"
                          variant="ghost"
                          size="sm"
                          className="h-7 px-2.5 text-xs"
                          onClick={() => {
                            onLoadIntoComposer(entry, false);
                            onOpenChange(false);
                          }}
                        >
                          <RotateCcw className="mr-1 size-3" />
                          Load into Composer
                        </Button>
                        {canRecall && (
                          <Button
                            type="button"
                            variant={isConfirming ? "destructive" : "outline"}
                            size="sm"
                            className="h-7 px-2.5 text-xs"
                            disabled={isRecalling}
                            onClick={async () => {
                              if (!isConfirming) {
                                setConfirmingHistoryId(entry.id);
                                return;
                              }
                              setRecallingId(entry.id);
                              try {
                                await onRecallPost(entry.id);
                                setConfirmingHistoryId(null);
                              } finally {
                                setRecallingId(null);
                              }
                            }}
                          >
                            {isRecalling ? (
                              <Loader2 className="mr-1 size-3 animate-spin" />
                            ) : (
                              <Trash2 className="mr-1 size-3" />
                            )}
                            {isConfirming
                              ? "Confirm Delete Everywhere"
                              : "Delete Everywhere"}
                          </Button>
                        )}
                      </div>
                    </article>
                  );
                })}
              </div>
            )}
          </TabsContent>
        </Tabs>
      </DialogContent>
    </Dialog>
  );
}
