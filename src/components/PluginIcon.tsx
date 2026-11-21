import { useMemo } from "react";
import { Puzzle } from "lucide-react";

interface PluginIconProps {
  svg: string | null | undefined;
  colour: string;
  size?: "sm" | "lg";
}

const ALLOWED_TAGS = new Set([
  "svg",
  "path",
  "circle",
  "rect",
  "ellipse",
  "polygon",
  "polyline",
  "line",
  "g",
]);

const ALLOWED_ATTRS = new Set([
  "viewbox",
  "fill",
  "stroke",
  "stroke-width",
  "stroke-linecap",
  "stroke-linejoin",
  "fill-rule",
  "clip-rule",
  "d",
  "cx",
  "cy",
  "r",
  "rx",
  "ry",
  "x",
  "y",
  "width",
  "height",
  "points",
  "x1",
  "y1",
  "x2",
  "y2",
  "transform",
  "opacity",
]);

function sanitiseSvg(rawSvg: string | null | undefined): string | null {
  if (!rawSvg) return null;
  const trimmed = rawSvg.trim();
  if (!trimmed.toLowerCase().startsWith("<svg")) return null;
  if (typeof DOMParser === "undefined") return null;

  try {
    const parser = new DOMParser();
    const doc = parser.parseFromString(trimmed, "image/svg+xml");
    if (doc.querySelector("parsererror")) return null;

    const root = doc.documentElement;
    if (!root || root.tagName.toLowerCase() !== "svg") return null;

    const cleanNode = (node: Element) => {
      const children = Array.from(node.children);
      for (const child of children) {
        const tag = child.tagName.toLowerCase();
        if (!ALLOWED_TAGS.has(tag)) {
          child.remove();
          continue;
        }
        cleanNode(child);
      }

      for (const attr of Array.from(node.attributes)) {
        const name = attr.name.toLowerCase();
        const val = attr.value.trim().toLowerCase();
        if (
          name.startsWith("on") ||
          !ALLOWED_ATTRS.has(name) ||
          val.includes("javascript:")
        ) {
          node.removeAttribute(attr.name);
        }
      }
    };

    cleanNode(root);
    root.setAttribute("width", "100%");
    root.setAttribute("height", "100%");
    root.setAttribute("class", "w-full h-full block");
    root.setAttribute("aria-hidden", "true");
    if (!root.getAttribute("fill")) {
      root.setAttribute("fill", "currentColor");
    }

    return new XMLSerializer().serializeToString(root);
  } catch {
    return null;
  }
}

export function PluginIcon({ svg, colour, size = "sm" }: PluginIconProps) {
  const safeSvg = useMemo(() => sanitiseSvg(svg), [svg]);
  const boxClass = size === "lg" ? "size-12 rounded-xl" : "size-9 rounded-lg";
  const innerClass = size === "lg" ? "size-6" : "size-4";

  return (
    <div
      className={`flex shrink-0 items-center justify-center border ${boxClass}`}
      style={{
        borderColor: `${colour}55`,
        backgroundColor: `${colour}18`,
        color: colour,
      }}
    >
      {safeSvg ? (
        <span
          className={`inline-flex items-center justify-center ${innerClass}`}
          // Sanitised above via strict DOMParser tag & attribute allowlist.
          dangerouslySetInnerHTML={{ __html: safeSvg }}
        />
      ) : (
        <Puzzle className={innerClass} />
      )}
    </div>
  );
}
