import { useEffect, useRef, useState } from "react";
import {
  Check,
  ChevronDown,
  Download,
  KeyRound,
  Lock,
  Pencil,
  Plus,
  Trash2,
  Upload,
  User,
  X,
} from "lucide-react";
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
import { cn } from "@/lib/utils";
import type { ProfileInfo } from "@/lib/tauri";

interface ProfileMenuProps {
  profiles: ProfileInfo[];
  activeProfile: ProfileInfo | null;
  onSelectProfile: (profileId: string) => void;
  onOpenAddProfile: () => void;
  onLockProfile: (profileId: string) => void;
  onRenameProfile: (profileId: string, name: string) => Promise<void>;
  onSetProfilePassword: (
    profileId: string,
    currentPassword: string | null,
    newPassword: string | null,
  ) => Promise<void>;
  onDeleteProfile: (profileId: string) => Promise<void>;
  onExportProfile: () => void;
  onImportProfile: () => void;
}

export function ProfileMenu({
  profiles,
  activeProfile,
  onSelectProfile,
  onOpenAddProfile,
  onLockProfile,
  onRenameProfile,
  onSetProfilePassword,
  onDeleteProfile,
  onExportProfile,
  onImportProfile,
}: ProfileMenuProps) {
  const [menuOpen, setMenuOpen] = useState(false);
  const [renameOpen, setRenameOpen] = useState(false);
  const [renameValue, setRenameValue] = useState("");
  const [isRenaming, setIsRenaming] = useState(false);
  const [passwordOpen, setPasswordOpen] = useState(false);
  const [currentPassword, setCurrentPassword] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [passwordError, setPasswordError] = useState<string | null>(null);
  const [isUpdatingPassword, setIsUpdatingPassword] = useState(false);
  const [deleteOpen, setDeleteOpen] = useState(false);
  const [isDeleting, setIsDeleting] = useState(false);
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

  const handleRenameSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!activeProfile || !renameValue.trim()) return;
    setIsRenaming(true);
    try {
      await onRenameProfile(activeProfile.id, renameValue.trim());
      setRenameOpen(false);
    } finally {
      setIsRenaming(false);
    }
  };

  const handleDeleteConfirm = async () => {
    if (!activeProfile) return;
    setIsDeleting(true);
    try {
      await onDeleteProfile(activeProfile.id);
      setDeleteOpen(false);
    } finally {
      setIsDeleting(false);
    }
  };

  const handlePasswordSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!activeProfile) return;
    setIsUpdatingPassword(true);
    setPasswordError(null);
    try {
      await onSetProfilePassword(
        activeProfile.id,
        activeProfile.hasPassword ? currentPassword : null,
        newPassword.length > 0 ? newPassword : null,
      );
      setCurrentPassword("");
      setNewPassword("");
      setPasswordOpen(false);
    } catch (err) {
      setPasswordError(String(err));
    } finally {
      setIsUpdatingPassword(false);
    }
  };

  const canDelete = profiles.length > 1 && activeProfile !== null;
  const canLock =
    activeProfile !== null &&
    activeProfile.hasPassword &&
    activeProfile.isUnlocked;
  const canEditPassword =
    activeProfile !== null && activeProfile.isUnlocked;

  return (
    <>
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
          <ChevronDown className="size-3.5 text-muted-foreground" />
        </Button>

        {menuOpen && (
          <div className="absolute right-0 z-50 mt-1.5 w-56 rounded-lg border bg-popover p-1 text-popover-foreground shadow-md">
            <div className="max-h-52 overflow-y-auto py-0.5">
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
                      "flex w-full cursor-pointer items-center justify-between gap-2 rounded-md px-2.5 py-1.5 text-left text-xs transition-colors hover:bg-accent hover:text-accent-foreground",
                      isActive && "font-medium",
                    )}
                  >
                    <span className="flex min-w-0 items-center gap-2">
                      <Check
                        className={cn(
                          "size-3.5 shrink-0",
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

            <div className="my-1 h-px bg-border" />

            <button
              type="button"
              onClick={() => {
                setMenuOpen(false);
                onOpenAddProfile();
              }}
              className="flex w-full cursor-pointer items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-xs transition-colors hover:bg-accent hover:text-accent-foreground"
            >
              <Plus className="size-3.5" />
              <span>Add Profile</span>
            </button>

            <button
              type="button"
              disabled={!activeProfile || !activeProfile.isUnlocked}
              onClick={() => {
                setMenuOpen(false);
                onExportProfile();
              }}
              className="flex w-full cursor-pointer items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-xs transition-colors hover:bg-accent hover:text-accent-foreground disabled:cursor-not-allowed disabled:opacity-50"
            >
              <Download className="size-3.5" />
              <span>Export Profile</span>
            </button>

            <button
              type="button"
              onClick={() => {
                setMenuOpen(false);
                onImportProfile();
              }}
              className="flex w-full cursor-pointer items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-xs transition-colors hover:bg-accent hover:text-accent-foreground"
            >
              <Upload className="size-3.5" />
              <span>Import Profile</span>
            </button>

            {canLock && activeProfile && (
              <button
                type="button"
                onClick={() => {
                  setMenuOpen(false);
                  onLockProfile(activeProfile.id);
                }}
                className="flex w-full cursor-pointer items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-xs transition-colors hover:bg-accent hover:text-accent-foreground"
              >
                <Lock className="size-3.5" />
                <span>Lock Profile</span>
              </button>
            )}

            {activeProfile && (
              <button
                type="button"
                onClick={() => {
                  setMenuOpen(false);
                  setRenameValue(activeProfile.name);
                  setRenameOpen(true);
                }}
                className="flex w-full cursor-pointer items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-xs transition-colors hover:bg-accent hover:text-accent-foreground"
              >
                <Pencil className="size-3.5" />
                <span>Rename Profile</span>
              </button>
            )}

            {canEditPassword && activeProfile && (
              <button
                type="button"
                onClick={() => {
                  setMenuOpen(false);
                  setCurrentPassword("");
                  setNewPassword("");
                  setPasswordError(null);
                  setPasswordOpen(true);
                }}
                className="flex w-full cursor-pointer items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-xs transition-colors hover:bg-accent hover:text-accent-foreground"
              >
                <KeyRound className="size-3.5" />
                <span>
                  {activeProfile.hasPassword
                    ? "Change Password"
                    : "Set Password"}
                </span>
              </button>
            )}

            {canDelete && (
              <button
                type="button"
                onClick={() => {
                  setMenuOpen(false);
                  setDeleteOpen(true);
                }}
                className="flex w-full cursor-pointer items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-xs text-destructive transition-colors hover:bg-destructive/10"
              >
                <Trash2 className="size-3.5" />
                <span>Delete Profile</span>
              </button>
            )}
          </div>
        )}
      </div>

      {/* Rename Profile Modal */}
      <Dialog open={renameOpen} onOpenChange={setRenameOpen}>
        <DialogContent showCloseButton={false} className="sm:max-w-sm">
          <DialogHeader>
            <div className="flex items-center justify-between gap-4">
              <DialogTitle>Rename Profile</DialogTitle>
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
          <form onSubmit={handleRenameSubmit} className="space-y-4">
            <div className="space-y-1.5">
              <Label htmlFor="rename-profile-name">Profile Name</Label>
              <Input
                id="rename-profile-name"
                value={renameValue}
                onChange={(e) => setRenameValue(e.target.value)}
                autoFocus
                disabled={isRenaming}
              />
            </div>
            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={() => setRenameOpen(false)}
                disabled={isRenaming}
              >
                Cancel
              </Button>
              <Button
                type="submit"
                size="sm"
                disabled={isRenaming || !renameValue.trim()}
              >
                Save
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>

      {/* Set / Change / Remove Password Modal */}
      <Dialog open={passwordOpen} onOpenChange={setPasswordOpen}>
        <DialogContent showCloseButton={false} className="sm:max-w-sm">
          <DialogHeader>
            <div className="flex items-center justify-between gap-4">
              <DialogTitle>
                {activeProfile?.hasPassword ? "Change Password" : "Set Password"}
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
          <form onSubmit={handlePasswordSubmit} className="space-y-4">
            {activeProfile?.hasPassword && (
              <div className="space-y-1.5">
                <Label htmlFor="current-profile-password">
                  Current Password
                </Label>
                <Input
                  id="current-profile-password"
                  type="password"
                  value={currentPassword}
                  onChange={(e) => setCurrentPassword(e.target.value)}
                  autoComplete="current-password"
                  autoFocus
                  required
                  disabled={isUpdatingPassword}
                />
              </div>
            )}

            <div className="space-y-1.5">
              <Label htmlFor="new-profile-password">New Password</Label>
              <Input
                id="new-profile-password"
                type="password"
                value={newPassword}
                onChange={(e) => setNewPassword(e.target.value)}
                placeholder={
                  activeProfile?.hasPassword
                    ? "Leave blank to remove password"
                    : "Enter password"
                }
                autoComplete="new-password"
                autoFocus={!activeProfile?.hasPassword}
                disabled={isUpdatingPassword}
              />
            </div>

            {passwordError && (
              <p className="text-xs text-destructive">{passwordError}</p>
            )}

            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={() => setPasswordOpen(false)}
                disabled={isUpdatingPassword}
              >
                Cancel
              </Button>
              <Button
                type="submit"
                size="sm"
                disabled={
                  isUpdatingPassword ||
                  (Boolean(activeProfile?.hasPassword) && !currentPassword) ||
                  (!activeProfile?.hasPassword && !newPassword)
                }
              >
                Save
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>

      {/* Delete Profile Confirmation Modal */}
      <Dialog open={deleteOpen} onOpenChange={setDeleteOpen}>
        <DialogContent showCloseButton={false} className="sm:max-w-sm">
          <DialogHeader>
            <div className="flex items-center justify-between gap-4">
              <DialogTitle>Delete {activeProfile?.name}?</DialogTitle>
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
          <p className="text-sm text-muted-foreground">
            Saved accounts, drafts, and post history for this profile will be
            permanently removed.
          </p>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => setDeleteOpen(false)}
              disabled={isDeleting}
            >
              Cancel
            </Button>
            <Button
              type="button"
              variant="destructive"
              size="sm"
              onClick={() => void handleDeleteConfirm()}
              disabled={isDeleting}
            >
              Delete Profile
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
