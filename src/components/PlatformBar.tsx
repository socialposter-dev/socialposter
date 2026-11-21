import { useMemo, useState } from "react";
import { AlertTriangle, Check, Settings2 } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import type { PlatformMetadata } from "@/lib/tauri";

interface PlatformBarProps {
  platforms: PlatformMetadata[];
  enabledIds: Set<string>;
  onToggle: (platformId: string) => void;
  onSelectAll: () => void;
  onClearAll: () => void;
  onOpenSettings: (platform: PlatformMetadata) => void;
  onOpenPlugins?: () => void;
}

export function PlatformBar({
  platforms,
  enabledIds,
  onToggle,
  onSelectAll,
  onClearAll,
  onOpenSettings,
  onOpenPlugins,
}: PlatformBarProps) {
  const [searchQuery, setSearchQuery] = useState("");

  const visiblePlatforms = useMemo(() => {
    const query = searchQuery.trim().toLowerCase();
    if (!query) return platforms;
    return platforms.filter((platform) =>
      platform.name.toLowerCase().includes(query),
    );
  }, [platforms, searchQuery]);

  const eligibleCount = platforms.filter(
    (p) => p.isCompatible && p.isConfigured,
  ).length;

  return (
    <section
      aria-label="Target platforms"
      className="rounded-xl border bg-card p-4 shadow-xs"
    >
      <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
        <div className="flex items-center gap-2">
          <h2 className="text-sm font-semibold tracking-tight">Platforms</h2>
          <Badge variant="secondary" className="text-xs font-normal">
            {enabledIds.size} of {platforms.length} active
          </Badge>
        </div>

        <div className="flex flex-wrap items-center gap-1.5">
          {platforms.length > 8 && (
            <Input
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder="Filter..."
              aria-label="Filter platforms"
              className="h-7 w-36 text-xs"
            />
          )}

          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="h-7 px-2 text-xs"
            onClick={onSelectAll}
            disabled={eligibleCount === 0 || enabledIds.size === eligibleCount}
          >
            Select All
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="h-7 px-2 text-xs"
            onClick={onClearAll}
            disabled={enabledIds.size === 0}
          >
            Clear
          </Button>
        </div>
      </div>

      {platforms.length === 0 ? (
        <div className="flex flex-col items-center gap-2 py-4 text-center">
          <p className="text-sm text-muted-foreground">
            No platforms installed. Open Plugins in the top bar to install a
            platform.
          </p>
          {onOpenPlugins && (
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="h-7 px-3 text-xs"
              onClick={onOpenPlugins}
            >
              Open Plugins
            </Button>
          )}
        </div>
      ) : visiblePlatforms.length === 0 ? (
        <p className="py-4 text-center text-xs text-muted-foreground">
          No matching platforms.
        </p>
      ) : (
        <div className="flex flex-wrap gap-2">
          {visiblePlatforms.map((platform) => {
            const isIncompatible = !platform.isCompatible;
            const isConfigured = platform.isConfigured;
            const isEnabled =
              !isIncompatible && isConfigured && enabledIds.has(platform.id);

            return (
              <div
                key={platform.id}
                style={
                  isEnabled
                    ? {
                        borderColor: platform.accentColour,
                        boxShadow: `0 0 0 1px ${platform.accentColour}33`,
                      }
                    : undefined
                }
                className={cn(
                  "group flex items-center gap-1.5 rounded-lg border px-2.5 py-1 text-xs transition-all select-none",
                  isIncompatible
                    ? "border-destructive/40 bg-destructive/5 text-muted-foreground opacity-80"
                    : isEnabled
                      ? "bg-background text-foreground shadow-xs"
                      : "border-border/70 bg-muted/40 text-muted-foreground hover:bg-muted/70",
                )}
              >
                <button
                  type="button"
                  onClick={() => {
                    if (isIncompatible) return;
                    if (!isConfigured) {
                      onOpenSettings(platform);
                    } else {
                      onToggle(platform.id);
                    }
                  }}
                  disabled={isIncompatible}
                  className={cn(
                    "flex items-center gap-1.5 text-left font-medium focus:outline-none",
                    isIncompatible ? "cursor-not-allowed" : "cursor-pointer",
                  )}
                  aria-pressed={isEnabled}
                  title={
                    isIncompatible
                      ? (platform.compatibilityError ?? "Incompatible version")
                      : !isConfigured
                        ? `Configure ${platform.name}`
                        : `Toggle ${platform.name}`
                  }
                >
                  <span
                    className={cn(
                      "flex size-3 items-center justify-center rounded-full border transition-transform",
                      !isConfigured && !isIncompatible && "opacity-40",
                    )}
                    style={{
                      backgroundColor: isEnabled
                        ? platform.accentColour
                        : "transparent",
                      borderColor: platform.accentColour,
                    }}
                  >
                    {isEnabled && (
                      <Check className="size-2 text-white stroke-[3]" />
                    )}
                  </span>
                  <span>{platform.name}</span>
                </button>

                {isIncompatible && (
                  <button
                    type="button"
                    onClick={() => onOpenSettings(platform)}
                    title={
                      platform.compatibilityError ??
                      `Requires SocialPoster ${platform.compatibleAppVersion}`
                    }
                    className="inline-flex cursor-pointer items-center gap-1 rounded bg-destructive/10 px-1 py-0.5 text-[10px] font-medium text-destructive hover:bg-destructive/20"
                  >
                    <AlertTriangle className="size-3" />
                    <span>Incompatible</span>
                  </button>
                )}

                <button
                  type="button"
                  onClick={() => onOpenSettings(platform)}
                  title={`Configure ${platform.name}`}
                  aria-label={`Configure ${platform.name}`}
                  className="inline-flex cursor-pointer items-center rounded p-1 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
                >
                  <Settings2 className="size-3" />
                </button>
              </div>
            );
          })}
        </div>
      )}
    </section>
  );
}
