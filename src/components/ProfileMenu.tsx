import { useEffect, useRef, useState } from "react";
import { Check, ChevronDown, Lock, Settings, User } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import type { ProfileInfo } from "@/lib/tauri";

interface ProfileMenuProps {
  profiles: ProfileInfo[];
  activeProfile: ProfileInfo | null;
  onSelectProfile: (profileId: string) => void;
  onManageProfiles: () => void;
}

export function ProfileMenu({
  profiles,
  activeProfile,
  onSelectProfile,
  onManageProfiles,
}: ProfileMenuProps) {
  const [menuOpen, setMenuOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!menuOpen) return;
    const handlePointerDown = (e: MouseEvent) => {
      if (
        containerRef.current &&
        !containerRef.current.contains(e.target as Node)
      ) {
        setMenuOpen(false);
      }
    };
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") setMenuOpen(false);
    };
    window.addEventListener("mousedown", handlePointerDown);
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("mousedown", handlePointerDown);
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [menuOpen]);

  return (
    <div ref={containerRef} className="relative">
      <Button
        type="button"
        variant="outline"
        size="sm"
        onClick={() => setMenuOpen((prev) => !prev)}
        aria-expanded={menuOpen}
        className="gap-1.5"
      >
        {activeProfile?.hasPassword ? (
          <Lock className="size-3.5 text-muted-foreground" />
        ) : (
          <User className="size-3.5 text-muted-foreground" />
        )}
        <span className="max-w-28 truncate">
          {activeProfile?.name ?? "Default"}
        </span>
        <ChevronDown
          className={`size-3.5 text-muted-foreground transition-transform duration-150 ${
            menuOpen ? "rotate-180" : ""
          }`}
        />
      </Button>

      {menuOpen && (
        <div className="absolute left-0 z-50 mt-1.5 w-56 rounded-xl border border-border/60 bg-popover p-1.5 text-popover-foreground shadow-lg ring-1 ring-foreground/10">
          <div className="max-h-56 overflow-y-auto py-0.5">
            {profiles.map((profile) => {
              const isActive = profile.id === activeProfile?.id;
              return (
                <button
                  key={profile.id}
                  type="button"
                  onClick={() => {
                    setMenuOpen(false);
                    onSelectProfile(profile.id);
                  }}
                  className={cn(
                    "flex w-full cursor-pointer items-center justify-between gap-2 rounded-md px-2.5 py-1.5 text-left text-xs transition-colors hover:bg-muted/60 hover:text-foreground",
                    isActive && "font-medium",
                  )}
                >
                  <span className="flex min-w-0 items-center gap-2">
                    <Check
                      className={cn(
                        "size-3.5 shrink-0 text-primary",
                        isActive ? "opacity-100" : "opacity-0",
                      )}
                    />
                    <span className="truncate">{profile.name}</span>
                  </span>
                  {profile.hasPassword && (
                    <Lock className="size-3 shrink-0 text-muted-foreground" />
                  )}
                </button>
              );
            })}
          </div>

          <div className="my-1 h-px bg-border/60" />

          <button
            type="button"
            onClick={() => {
              setMenuOpen(false);
              onManageProfiles();
            }}
            className="flex w-full cursor-pointer items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-xs text-muted-foreground transition-colors hover:bg-muted/60 hover:text-foreground"
          >
            <Settings className="size-3.5" />
            <span>Manage Profiles</span>
          </button>
        </div>
      )}
    </div>
  );
}
