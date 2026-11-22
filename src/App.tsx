import {
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { listen } from "@tauri-apps/api/event";
import { ThemeProvider } from "next-themes";
import {
  Clock,
  Download,
  Loader2,
  Puzzle,
  RotateCw,
  Settings,
  X,
} from "lucide-react";
import { toast } from "sonner";
import { AppLogo } from "@/components/AppLogo";
import { Composer } from "@/components/Composer";
import { DispatchResults } from "@/components/DispatchResults";
import { PlatformBar } from "@/components/PlatformBar";
import { ProfileMenu } from "@/components/ProfileMenu";
import { UnlockProfileCard } from "@/components/UnlockProfileCard";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Toaster } from "@/components/ui/sonner";

const AddProfileDialog = lazy(() =>
  import("@/components/AddProfileDialog").then((m) => ({
    default: m.AddProfileDialog,
  })),
);
const BackupDialog = lazy(() =>
  import("@/components/BackupDialog").then((m) => ({
    default: m.BackupDialog,
  })),
);
const QueueHistoryDialog = lazy(() =>
  import("@/components/QueueHistoryDialog").then((m) => ({
    default: m.QueueHistoryDialog,
  })),
);
const PluginsDialog = lazy(() =>
  import("@/components/PluginsDialog").then((m) => ({
    default: m.PluginsDialog,
  })),
);
const SettingsDialog = lazy(() =>
  import("@/components/SettingsDialog").then((m) => ({
    default: m.SettingsDialog,
  })),
);
import {
  SettingsOverlay,
  type SettingsSection,
} from "@/components/SettingsOverlay";
import {
  clearProfileStorage,
  clearStoredDraft,
  EMPTY_DRAFT,
  enabledPlatformsKey,
  getActiveProfileId,
  knownPlatformsKey,
  loadAutoThread,
  loadDraft,
  saveAutoThread,
  saveDraft,
  setActiveProfileId,
  type ComposerDraft,
} from "@/lib/storage";
import {
  broadcastPost,
  cancelScheduledPost,
  clearHistory,
  createProfile,
  deleteProfile,
  factoryReset,
  getDesktopSettings,
  getHistory,
  getPlatforms,
  listProfiles,
  listScheduledPosts,
  lockProfile,
  publishScheduledNow,
  recallPost,
  renameProfile,
  retryScheduledPost,
  schedulePost,
  setProfilePassword,
  unlinkAllProfileAccounts,
  unlockProfile,
  type DesktopSettings,
  type HistoryEntry,
  type PlatformDispatchRequest,
  type PlatformMetadata,
  type PostOutcome,
  type ProfileInfo,
  type ScheduledPost,
} from "@/lib/tauri";
import {
  checkForUpdateSilently,
  downloadAndInstallUpdate,
  getDismissedUpdateVersion,
  relaunchApp,
  setDismissedUpdateVersion,
  type Update,
} from "@/lib/updater";
import "./App.css";

function readStringArray(key: string): string[] | null {
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return null;
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return null;
    return parsed.filter((item): item is string => typeof item === "string");
  } catch {
    return null;
  }
}

function persistPlatformState(
  profileId: string,
  enabledIds: Set<string>,
  registeredPlatforms: PlatformMetadata[],
) {
  try {
    localStorage.setItem(
      enabledPlatformsKey(profileId),
      JSON.stringify(Array.from(enabledIds)),
    );
    localStorage.setItem(
      knownPlatformsKey(profileId),
      JSON.stringify(registeredPlatforms.map((p) => p.id)),
    );
  } catch {
    // Ignore storage quota / privacy mode failures.
  }
}

function reconcileEnabledPlatforms(
  profileId: string,
  registered: PlatformMetadata[],
): Set<string> {
  const eligibleIds = new Set(
    registered
      .filter((p) => p.isCompatible && p.isConfigured)
      .map((p) => p.id),
  );
  const storedEnabled = readStringArray(enabledPlatformsKey(profileId));
  const storedKnown = readStringArray(knownPlatformsKey(profileId));

  if (storedEnabled === null) {
    const initial = new Set(eligibleIds);
    persistPlatformState(profileId, initial, registered);
    return initial;
  }

  const reconciled = new Set(
    storedEnabled.filter((id) => eligibleIds.has(id)),
  );
  const knownSet = new Set(storedKnown ?? storedEnabled);

  for (const platform of registered) {
    if (
      !knownSet.has(platform.id) &&
      platform.isCompatible &&
      platform.isConfigured
    ) {
      reconciled.add(platform.id);
    }
  }

  persistPlatformState(profileId, reconciled, registered);
  return reconciled;
}

function SocialPosterApp() {
  const [profiles, setProfiles] = useState<ProfileInfo[]>([]);
  const [activeProfileId, setActiveProfileIdState] = useState<string>(() =>
    getActiveProfileId(),
  );
  const [addProfileOpen, setAddProfileOpen] = useState(false);
  const [backupMode, setBackupMode] = useState<"export" | "import" | null>(
    null,
  );

  const [draft, setDraft] = useState<ComposerDraft>(EMPTY_DRAFT);
  const [draftRestored, setDraftRestored] = useState(false);
  const [autoThread, setAutoThread] = useState(false);
  const [history, setHistory] = useState<HistoryEntry[]>([]);
  const [scheduledPosts, setScheduledPosts] = useState<ScheduledPost[]>([]);

  const [platforms, setPlatforms] = useState<PlatformMetadata[]>([]);
  const [enabledIds, setEnabledIds] = useState<Set<string>>(new Set());
  const [isLoading, setIsLoading] = useState(true);
  const [isInitialised, setIsInitialised] = useState(false);
  const [isPosting, setIsPosting] = useState(false);
  const [outcomes, setOutcomes] = useState<PostOutcome[]>([]);
  const [settingsPlatform, setSettingsPlatform] =
    useState<PlatformMetadata | null>(null);
  const [queueHistoryOpen, setQueueHistoryOpen] = useState(false);
  const [pluginsOpen, setPluginsOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsSection, setSettingsSection] =
    useState<SettingsSection>("general");
  const [desktopSettings, setDesktopSettings] = useState<DesktopSettings>({
    minimizeToTray: true,
    globalShortcut: "CommandOrControl+Shift+Space",
    developerMode: false,
    launchAtStartup: false,
  });

  const [availableUpdate, setAvailableUpdate] = useState<Update | null>(null);
  const [updatePhase, setUpdatePhase] = useState<
    "available" | "downloading" | "ready"
  >("available");
  const [downloadPercent, setDownloadPercent] = useState<number | null>(null);

  const hydratedProfileRef = useRef<string | null>(null);

  const activeProfile = useMemo(
    () => profiles.find((p) => p.id === activeProfileId) ?? profiles[0] ?? null,
    [profiles, activeProfileId],
  );

  const clearInMemoryProfileState = useCallback(() => {
    hydratedProfileRef.current = null;
    setPlatforms([]);
    setEnabledIds(new Set());
    setDraft(EMPTY_DRAFT);
    setDraftRestored(false);
    setAutoThread(false);
    setHistory([]);
    setScheduledPosts([]);
    setOutcomes([]);
    setSettingsPlatform(null);
    setQueueHistoryOpen(false);
  }, []);

  useEffect(() => {
    void getDesktopSettings()
      .then((s) => setDesktopSettings(s))
      .catch(() => {
        // Ignore outside Tauri shell.
      });
  }, []);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === ",") {
        e.preventDefault();
        setSettingsOpen((prev) => !prev);
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  const loadUnlockedProfileData = useCallback(
    async (
      profileId: string,
      isInitialForProfile: boolean,
      autoEnablePlatformId?: string,
    ) => {
      try {
        const [platformList, historyList, scheduleRes] = await Promise.all([
          getPlatforms(profileId),
          getHistory(profileId),
          listScheduledPosts(profileId),
        ]);

        setPlatforms(platformList);
        setHistory(historyList);
        setScheduledPosts(scheduleRes.posts);
        setSettingsPlatform((current) =>
          current
            ? (platformList.find((p) => p.id === current.id) ?? current)
            : null,
        );

        if (isInitialForProfile) {
          const restoredDraft = loadDraft(profileId);
          setDraft(restoredDraft ?? EMPTY_DRAFT);
          setDraftRestored(restoredDraft !== null);
          setAutoThread(loadAutoThread(profileId));
          setEnabledIds(reconcileEnabledPlatforms(profileId, platformList));
          hydratedProfileRef.current = profileId;
        } else {
          setEnabledIds((prev) => {
            const eligibleIds = new Set(
              platformList
                .filter((p) => p.isCompatible && p.isConfigured)
                .map((p) => p.id),
            );
            const filtered = new Set(
              Array.from(prev).filter((id) => eligibleIds.has(id)),
            );
            if (autoEnablePlatformId && eligibleIds.has(autoEnablePlatformId)) {
              filtered.add(autoEnablePlatformId);
            }
            persistPlatformState(profileId, filtered, platformList);
            return filtered;
          });
        }
      } catch (err) {
        toast.error(`Failed to load profile data: ${String(err)}`);
      } finally {
        setIsLoading(false);
      }
    },
    [],
  );

  const refreshProfilesAndActivate = useCallback(
    async (preferredId?: string) => {
      try {
        const list = await listProfiles();
        setProfiles(list);

        const candidateId = preferredId ?? getActiveProfileId();
        const target =
          list.find((p) => p.id === candidateId) ?? list[0] ?? null;
        if (!target) {
          clearInMemoryProfileState();
          setIsLoading(false);
          return;
        }

        setActiveProfileId(target.id);
        setActiveProfileIdState(target.id);

        if (target.isUnlocked) {
          await loadUnlockedProfileData(target.id, true);
        } else {
          clearInMemoryProfileState();
          setIsLoading(false);
        }
      } catch (err) {
        toast.error(`Failed to load profiles: ${String(err)}`);
        setIsLoading(false);
      }
    },
    [clearInMemoryProfileState, loadUnlockedProfileData],
  );

  useEffect(() => {
    void refreshProfilesAndActivate();
  }, [refreshProfilesAndActivate]);

  useEffect(() => {
    if (!isLoading) {
      setIsInitialised(true);
    }
  }, [isLoading]);

  useEffect(() => {
    const fallbackTimer = window.setTimeout(() => {
      setIsInitialised(true);
    }, 3000);
    return () => window.clearTimeout(fallbackTimer);
  }, []);

  useEffect(() => {
    if (!isInitialised) return;
    const splash = document.getElementById("startup-splash");
    if (!splash) return;

    splash.classList.add("splash-hidden");

    const removeSplash = () => {
      splash.remove();
    };
    splash.addEventListener("transitionend", removeSplash, { once: true });
    const cleanupTimer = window.setTimeout(removeSplash, 250);

    return () => {
      splash.removeEventListener("transitionend", removeSplash);
      window.clearTimeout(cleanupTimer);
    };
  }, [isInitialised]);

  useEffect(() => {
    if (
      !activeProfile ||
      !activeProfile.isUnlocked ||
      hydratedProfileRef.current !== activeProfile.id
    ) {
      return;
    }
    const profileId = activeProfile.id;
    const timer = window.setTimeout(() => {
      saveDraft(profileId, draft);
    }, 400);
    return () => window.clearTimeout(timer);
  }, [draft, activeProfile]);

  useEffect(() => {
    let active = true;
    void checkForUpdateSilently().then((update) => {
      if (!active || !update) return;
      const dismissed = getDismissedUpdateVersion();
      if (dismissed === update.version) return;
      setAvailableUpdate(update);
      setUpdatePhase("available");
    });
    return () => {
      active = false;
    };
  }, []);

  const handleSelectProfile = async (profileId: string) => {
    if (profileId === activeProfile?.id) return;
    clearInMemoryProfileState();
    setIsLoading(true);
    await refreshProfilesAndActivate(profileId);
  };

  const handleCreateProfile = async (name: string, password: string | null) => {
    try {
      const created = await createProfile(name, password);
      clearInMemoryProfileState();
      setIsLoading(true);
      await refreshProfilesAndActivate(created.id);
      toast.success(`Created profile "${created.name}".`);
    } catch (err) {
      toast.error(`Failed to create profile: ${String(err)}`);
      throw err;
    }
  };

  const handleUnlockProfile = async (profileId: string, password: string) => {
    const ok = await unlockProfile(profileId, password);
    if (ok) {
      setIsLoading(true);
      await refreshProfilesAndActivate(profileId);
    }
    return ok;
  };

  const handleLockProfile = useCallback(
    async (profileId: string) => {
      try {
        await lockProfile(profileId);
        clearInMemoryProfileState();
        const list = await listProfiles();
        setProfiles(list);
      } catch (err) {
        toast.error(`Failed to lock profile: ${String(err)}`);
      }
    },
    [clearInMemoryProfileState],
  );

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "l") {
        if (activeProfile && activeProfile.hasPassword && activeProfile.isUnlocked) {
          e.preventDefault();
          void handleLockProfile(activeProfile.id);
        }
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [activeProfile, handleLockProfile]);

  const handleSetProfilePassword = async (
    profileId: string,
    currentPassword: string | null,
    newPassword: string | null,
  ) => {
    const updated = await setProfilePassword(
      profileId,
      currentPassword,
      newPassword,
    );
    setProfiles((prev) =>
      prev.map((p) => (p.id === updated.id ? updated : p)),
    );
    toast.success(
      updated.hasPassword ? "Profile password saved." : "Profile password removed.",
    );
  };

  const handleRenameProfile = async (profileId: string, name: string) => {
    try {
      const updated = await renameProfile(profileId, name);
      setProfiles((prev) =>
        prev.map((p) => (p.id === updated.id ? updated : p)),
      );
      toast.success(`Renamed profile to "${updated.name}".`);
    } catch (err) {
      toast.error(`Failed to rename profile: ${String(err)}`);
      throw err;
    }
  };

  const handleDeleteProfile = async (profileId: string) => {
    try {
      await deleteProfile(profileId);
      clearProfileStorage(profileId);
      clearInMemoryProfileState();
      setIsLoading(true);
      await refreshProfilesAndActivate();
      toast.success("Profile deleted.");
    } catch (err) {
      toast.error(`Failed to delete profile: ${String(err)}`);
      throw err;
    }
  };

  const handleInstallHeaderUpdate = async () => {
    if (!availableUpdate) return;
    if (updatePhase === "ready") {
      try {
        await relaunchApp();
      } catch (err) {
        toast.error(`Restart failed: ${String(err)}`);
      }
      return;
    }

    setUpdatePhase("downloading");
    setDownloadPercent(null);
    try {
      await downloadAndInstallUpdate(availableUpdate, (progress) => {
        setDownloadPercent(progress.percent);
      });
      setUpdatePhase("ready");
    } catch (err) {
      setUpdatePhase("available");
      toast.error(`Update failed: ${String(err)}`);
    }
  };

  const handleDismissHeaderUpdate = () => {
    if (!availableUpdate) return;
    setDismissedUpdateVersion(availableUpdate.version);
    setAvailableUpdate(null);
  };

  const updateEnabledIds = useCallback(
    (updater: (prev: Set<string>) => Set<string>) => {
      if (!activeProfile) return;
      const profileId = activeProfile.id;
      setEnabledIds((prev) => {
        const next = updater(prev);
        persistPlatformState(profileId, next, platforms);
        return next;
      });
    },
    [activeProfile, platforms],
  );

  const handleToggle = (platformId: string) => {
    const target = platforms.find((p) => p.id === platformId);
    if (!target || !target.isCompatible) return;
    if (!target.isConfigured) {
      setSettingsPlatform(target);
      return;
    }
    updateEnabledIds((prev) => {
      const next = new Set(prev);
      if (next.has(platformId)) {
        next.delete(platformId);
      } else {
        next.add(platformId);
      }
      return next;
    });
  };

  const handleSelectAll = () => {
    updateEnabledIds(
      () =>
        new Set(
          platforms
            .filter((p) => p.isCompatible && p.isConfigured)
            .map((p) => p.id),
        ),
    );
  };

  const handleClearAll = () => {
    updateEnabledIds(() => new Set());
  };

  const handleClearDraft = () => {
    if (!activeProfile) return;
    clearStoredDraft(activeProfile.id);
    setDraft(EMPTY_DRAFT);
    setDraftRestored(false);
  };

  const handleBroadcast = async (targets: PlatformDispatchRequest[]) => {
    if (!activeProfile) return;
    const profileId = activeProfile.id;
    setIsPosting(true);
    try {
      const results = await broadcastPost(
        profileId,
        targets,
        draft.masterText,
        draft.overrides,
        draft.media,
        autoThread,
      );
      setOutcomes(results);

      const updatedHistory = await getHistory(profileId);
      setHistory(updatedHistory);

      const okCount = results.filter((r) => r.success).length;
      if (results.length > 0 && okCount === results.length) {
        clearStoredDraft(profileId);
        setDraft(EMPTY_DRAFT);
        setDraftRestored(false);
        toast.success(`Posted to ${okCount} platforms.`);
      } else if (okCount > 0) {
        const succeededIds = new Set(
          results.filter((r) => r.success).map((r) => r.platformId),
        );
        updateEnabledIds((prev) => {
          const next = new Set(prev);
          for (const id of succeededIds) {
            next.delete(id);
          }
          return next;
        });
        toast.warning(
          `Posted to ${okCount} of ${results.length}. Failed platforms remain selected.`,
        );
      } else {
        toast.error("All posts failed.");
      }
    } catch (err) {
      toast.error(`Broadcast failed: ${String(err)}`);
    } finally {
      setIsPosting(false);
    }
  };

  const handleLoadFromHistory = (entry: HistoryEntry, onlyFailed = false) => {
    setDraft({
      masterText: entry.masterText,
      overrides: { ...entry.overrides },
      media: entry.media.map((m) => ({ ...m })),
    });
    setDraftRestored(false);

    if (onlyFailed) {
      const compatibleIds = new Set(
        platforms
          .filter((p) => p.isCompatible && p.isConfigured)
          .map((p) => p.id),
      );
      const failedIds = entry.outcomes
        .filter((o) => !o.success && compatibleIds.has(o.platformId))
        .map((o) => o.platformId);
      updateEnabledIds(() => new Set(failedIds));
      toast.info("Loaded failed platforms.");
    } else {
      toast.info("Loaded into composer.");
    }
    setQueueHistoryOpen(false);
  };

  const handleClearHistory = async () => {
    if (!activeProfile) return;
    try {
      await clearHistory(activeProfile.id);
      setHistory([]);
      toast.success("History cleared.");
    } catch (err) {
      toast.error(`Failed to clear history: ${String(err)}`);
    }
  };

  const handleClearHistoryAndDrafts = async () => {
    if (!activeProfile) return;
    await clearHistory(activeProfile.id);
    clearStoredDraft(activeProfile.id);
    setDraft(EMPTY_DRAFT);
    setDraftRestored(false);
    setHistory([]);
    toast.success("Post history and drafts cleared for this profile.");
  };

  const handleUnlinkAllAccounts = async () => {
    if (!activeProfile) return;
    await unlinkAllProfileAccounts(activeProfile.id);
    updateEnabledIds(() => new Set());
    await loadUnlockedProfileData(activeProfile.id, false);
    toast.success("All accounts unlinked for this profile.");
  };

  const handleFactoryReset = async () => {
    const freshProfile = await factoryReset();
    localStorage.clear();
    setActiveProfileId(freshProfile.id);
    setActiveProfileIdState(freshProfile.id);
    clearInMemoryProfileState();
    setDesktopSettings({
      minimizeToTray: true,
      globalShortcut: "CommandOrControl+Shift+Space",
      developerMode: false,
      launchAtStartup: false,
    });
    setSettingsOpen(false);
    await refreshProfilesAndActivate(freshProfile.id);
    toast.success("SocialPoster has been reset to factory defaults.");
  };

  const handleRecallPost = async (historyId: string) => {
    if (!activeProfile) return;
    const profileId = activeProfile.id;
    try {
      const updated = await recallPost(profileId, historyId);
      setHistory((prev) =>
        prev.map((entry) => (entry.id === updated.id ? updated : entry)),
      );
      const hasErrors = updated.outcomes.some((o) => Boolean(o.recallError));
      if (hasErrors) {
        toast.warning("Some platforms failed to recall.");
      } else {
        toast.success("Post recalled.");
      }
    } catch (err) {
      toast.error(`Failed to recall post: ${String(err)}`);
    }
  };

  useEffect(() => {
    if (!activeProfile || !activeProfile.isUnlocked) return;
    const profileId = activeProfile.id;
    let active = true;
    const unlistenPromise = listen<string>("schedule-updated", (event) => {
      if (!active) return;
      if (event.payload && event.payload !== profileId) return;
      void Promise.all([
        listScheduledPosts(profileId),
        getHistory(profileId),
      ])
        .then(([scheduleRes, historyList]) => {
          if (!active) return;
          setScheduledPosts(scheduleRes.posts);
          setHistory(historyList);
        })
        .catch(() => {
          // Ignore transient refresh errors from background event.
        });
    });
    return () => {
      active = false;
      void unlistenPromise.then((unlisten) => unlisten());
    };
  }, [activeProfile]);

  const handleSchedulePost = async (
    scheduledForUtcIso: string,
    platformIds: string[],
  ) => {
    if (!activeProfile) return;
    const profileId = activeProfile.id;
    setIsPosting(true);
    try {
      await schedulePost(
        profileId,
        scheduledForUtcIso,
        draft.masterText,
        draft.overrides,
        draft.media.map((item) => ({
          path: item.path,
          mimeType: item.mimeType,
          altText: item.altText?.trim() ? item.altText.trim() : null,
        })),
        platformIds,
        autoThread,
      );
      const scheduleRes = await listScheduledPosts(profileId);
      setScheduledPosts(scheduleRes.posts);
      clearStoredDraft(profileId);
      setDraft(EMPTY_DRAFT);
      setDraftRestored(false);
      toast.success("Post scheduled.");
    } catch (err) {
      toast.error(`Schedule failed: ${String(err)}`);
    } finally {
      setIsPosting(false);
    }
  };

  const handlePublishScheduledNow = async (jobId: string) => {
    if (!activeProfile) return;
    const profileId = activeProfile.id;
    try {
      const results = await publishScheduledNow(profileId, jobId);
      setOutcomes(results);
      const [scheduleRes, historyList] = await Promise.all([
        listScheduledPosts(profileId),
        getHistory(profileId),
      ]);
      setScheduledPosts(scheduleRes.posts);
      setHistory(historyList);
      const okCount = results.filter((r) => r.success).length;
      if (results.length > 0 && okCount === results.length) {
        toast.success(`Posted to ${okCount} platforms.`);
      } else {
        toast.warning(`${okCount}/${results.length} succeeded.`);
      }
    } catch (err) {
      toast.error(`Failed to publish scheduled post: ${String(err)}`);
    }
  };

  const handleCancelScheduledPost = async (jobId: string) => {
    if (!activeProfile) return;
    const profileId = activeProfile.id;
    try {
      await cancelScheduledPost(profileId, jobId);
      const scheduleRes = await listScheduledPosts(profileId);
      setScheduledPosts(scheduleRes.posts);
      toast.success("Scheduled post cancelled.");
    } catch (err) {
      toast.error(`Failed to cancel scheduled post: ${String(err)}`);
    }
  };

  const handleRetryScheduledPost = async (jobId: string) => {
    if (!activeProfile) return;
    const profileId = activeProfile.id;
    try {
      await retryScheduledPost(profileId, jobId);
      const scheduleRes = await listScheduledPosts(profileId);
      setScheduledPosts(scheduleRes.posts);
      toast.success("Scheduled post queued for retry.");
    } catch (err) {
      toast.error(`Failed to retry scheduled post: ${String(err)}`);
    }
  };

  const pendingQueueCount = useMemo(
    () => scheduledPosts.filter((p) => p.status === "pending").length,
    [scheduledPosts],
  );

  const isLocked = Boolean(activeProfile && !activeProfile.isUnlocked);

  return (
    <div className="flex h-screen flex-col overflow-hidden bg-background text-foreground">
      <header className="relative z-40 w-full shrink-0 border-b bg-card/60 backdrop-blur-xs">
        <div className="mx-auto flex max-w-4xl items-center justify-between gap-3 px-4 py-3.5 sm:px-6">
          <div className="flex items-center gap-3">
            <div className="flex items-center gap-2.5">
              <AppLogo className="h-8 w-8 shrink-0" />
              <h1 className="text-base font-semibold tracking-tight">
                SocialPoster
              </h1>
            </div>

            <ProfileMenu
              profiles={profiles}
              activeProfile={activeProfile}
              onSelectProfile={(id) => void handleSelectProfile(id)}
              onManageProfiles={() => {
                setSettingsSection("profiles");
                setSettingsOpen(true);
              }}
            />
          </div>

          <div className="flex flex-wrap items-center gap-1.5">
            {availableUpdate && (
              <div className="inline-flex items-center rounded-full border bg-secondary/80 text-xs font-medium">
                <button
                  type="button"
                  onClick={() => void handleInstallHeaderUpdate()}
                  disabled={updatePhase === "downloading"}
                  className="inline-flex cursor-pointer items-center gap-1.5 px-2.5 py-1 text-foreground transition-opacity hover:opacity-80 disabled:cursor-default disabled:opacity-70"
                >
                  {updatePhase === "downloading" ? (
                    <>
                      <Loader2 className="size-3.5 animate-spin" />
                      <span>
                        {downloadPercent !== null
                          ? `Downloading ${downloadPercent}%`
                          : "Downloading..."}
                      </span>
                    </>
                  ) : updatePhase === "ready" ? (
                    <>
                      <RotateCw className="size-3.5" />
                      <span>Restart to update</span>
                    </>
                  ) : (
                    <>
                      <Download className="size-3.5" />
                      <span>Update v{availableUpdate.version}</span>
                    </>
                  )}
                </button>
                {updatePhase === "available" && (
                  <button
                    type="button"
                    onClick={handleDismissHeaderUpdate}
                    aria-label="Dismiss update"
                    className="inline-flex cursor-pointer items-center rounded-r-full py-1 pr-2 pl-1 text-muted-foreground hover:text-foreground"
                  >
                    <X className="size-3" />
                  </button>
                )}
              </div>
            )}

            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={isLocked || !activeProfile}
              onClick={() => setQueueHistoryOpen(true)}
            >
              <Clock className="mr-1.5 size-3.5" />
              <span>Queue &amp; History</span>
              {!isLocked && pendingQueueCount > 0 && (
                <Badge
                  variant="secondary"
                  className="ml-1.5 h-4 px-1.5 text-[10px] font-normal"
                >
                  {pendingQueueCount}
                </Badge>
              )}
            </Button>

            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => setPluginsOpen(true)}
            >
              <Puzzle className="mr-1.5 size-3.5" />
              Plugins
            </Button>

            <Button
              type="button"
              variant="outline"
              size="icon-sm"
              onClick={() => {
                setSettingsSection("general");
                setSettingsOpen(true);
              }}
              aria-label="Open settings"
              title="Settings (Ctrl+,)"
            >
              <Settings className="size-4" />
            </Button>
          </div>
        </div>
      </header>

      <main className="min-h-0 flex-1 overflow-x-hidden overflow-y-auto">
        <div className="box-border w-screen">
          <div className="mx-auto max-w-4xl space-y-5 px-4 py-6 sm:px-6">
            {isLoading ? (
              <div className="rounded-xl border bg-card p-10 text-center text-sm text-muted-foreground">
                Loading...
              </div>
            ) : isLocked && activeProfile ? (
              <UnlockProfileCard
                profile={activeProfile}
                onUnlock={handleUnlockProfile}
              />
            ) : (
              <>
                <PlatformBar
                  platforms={platforms}
                  enabledIds={enabledIds}
                  onToggle={handleToggle}
                  onSelectAll={handleSelectAll}
                  onClearAll={handleClearAll}
                  onOpenSettings={(platform) => setSettingsPlatform(platform)}
                  onOpenPlugins={() => setPluginsOpen(true)}
                />

                <Composer
                  profileId={activeProfile?.id ?? "default"}
                  platforms={platforms}
                  enabledIds={enabledIds}
                  draft={draft}
                  onDraftChange={setDraft}
                  draftRestored={draftRestored}
                  onClearDraft={handleClearDraft}
                  autoThread={autoThread}
                  onAutoThreadChange={(enabled) => {
                    setAutoThread(enabled);
                    if (activeProfile) {
                      saveAutoThread(activeProfile.id, enabled);
                    }
                  }}
                  isPosting={isPosting}
                  onBroadcast={handleBroadcast}
                  onSchedule={handleSchedulePost}
                  onOpenSettings={(platform) => setSettingsPlatform(platform)}
                />

                <DispatchResults outcomes={outcomes} />
              </>
            )}
          </div>
        </div>
      </main>

      <Suspense fallback={null}>
        {activeProfile && !isLocked && (
          <>
            {settingsPlatform !== null && (
              <SettingsDialog
                profileId={activeProfile.id}
                platform={settingsPlatform}
                open={true}
                onOpenChange={(open) => {
                  if (!open) setSettingsPlatform(null);
                }}
                onSaved={() =>
                  loadUnlockedProfileData(
                    activeProfile.id,
                    false,
                    settingsPlatform?.id,
                  )
                }
                onUnlinked={(platformId) =>
                  updateEnabledIds((prev) => {
                    const next = new Set(prev);
                    next.delete(platformId);
                    return next;
                  })
                }
              />
            )}

            {queueHistoryOpen && (
              <QueueHistoryDialog
                open={true}
                onOpenChange={setQueueHistoryOpen}
                posts={scheduledPosts}
                history={history}
                platforms={platforms}
                onPublishNow={handlePublishScheduledNow}
                onRetry={handleRetryScheduledPost}
                onCancel={handleCancelScheduledPost}
                onLoadIntoComposer={handleLoadFromHistory}
                onClearHistory={() => void handleClearHistory()}
                onRecallPost={handleRecallPost}
              />
            )}
          </>
        )}

        {pluginsOpen && (
          <PluginsDialog
            open={true}
            onOpenChange={setPluginsOpen}
            developerMode={desktopSettings.developerMode}
            onPluginsChanged={async () => {
              if (activeProfile && activeProfile.isUnlocked) {
                await loadUnlockedProfileData(activeProfile.id, false);
              }
            }}
          />
        )}

        {settingsOpen && (
          <SettingsOverlay
            open={true}
            initialSection={settingsSection}
            onClose={() => setSettingsOpen(false)}
            desktopSettings={desktopSettings}
            onDesktopSettingsChange={setDesktopSettings}
            profiles={profiles}
            activeProfile={activeProfile}
            platforms={platforms}
            onSelectProfile={(id) => void handleSelectProfile(id)}
            onOpenAddProfile={() => setAddProfileOpen(true)}
            onLockProfile={(id) => void handleLockProfile(id)}
            onRenameProfile={handleRenameProfile}
            onDeleteProfile={handleDeleteProfile}
            onSetProfilePassword={handleSetProfilePassword}
            onOpenBackup={(mode) => setBackupMode(mode)}
            onClearHistoryAndDrafts={handleClearHistoryAndDrafts}
            onUnlinkAllAccounts={handleUnlinkAllAccounts}
            onFactoryReset={handleFactoryReset}
          />
        )}

        {addProfileOpen && (
          <AddProfileDialog
            open={true}
            onOpenChange={setAddProfileOpen}
            onCreate={handleCreateProfile}
          />
        )}

        {backupMode !== null && (
          <BackupDialog
            mode={backupMode}
            activeProfile={activeProfile}
            onClose={() => setBackupMode(null)}
            onImported={async (result) => {
              setActiveProfileId(result.profile.id);
              setActiveProfileIdState(result.profile.id);
              clearInMemoryProfileState();
              await refreshProfilesAndActivate(result.profile.id);
            }}
          />
        )}
      </Suspense>

      <Toaster position="bottom-right" richColors />
    </div>
  );
}

export function App() {
  return (
    <ThemeProvider attribute="class" defaultTheme="system" enableSystem>
      <SocialPosterApp />
    </ThemeProvider>
  );
}

export default App;
