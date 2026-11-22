import type React from "react";

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

export function formatShortcutDisplay(
  shortcut: string | null | undefined,
): string {
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

export function keyEventToTauriShortcut(
  e: React.KeyboardEvent,
): string | null {
  if (MODIFIER_KEYS.has(e.key)) return null;

  const primary = normalizePrimaryKey(e);
  if (!primary) return null;

  const parts: string[] = [];
  if (e.ctrlKey || e.metaKey) parts.push("CommandOrControl");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");

  if (parts.length === 0) return null;
  parts.push(primary);
  return parts.join("+");
}
