const CATEGORY_LABELS: Record<string, string> = {
  core: "Core",
  adapter: "Channels",
  model: "Offline models",
  expansion: "Expansions",
};

export function categoryLabel(category: string): string {
  return CATEGORY_LABELS[category] ?? category;
}

export function humanizeWarning(warning: string): string {
  let text = warning.replace(/https?:\/\/\S+/g, "").trim();
  text = text.replace(/\s{2,}/g, " ");

  if (text.includes("not available for")) {
    return "Some selected items are not available for your platform.";
  }
  if (text.includes("differs from release")) {
    return "An update is available for your installation.";
  }
  if (text.includes("release package not found")) {
    return "A selected component could not be found in the current release.";
  }

  return text || "Review your selection before continuing.";
}

export function truncatePath(path: string, max = 42): string {
  if (path.length <= max) return path;
  const head = Math.ceil((max - 3) / 2);
  const tail = Math.floor((max - 3) / 2);
  return `${path.slice(0, head)}…${path.slice(-tail)}`;
}

export const INSTALLER_TAGLINE =
  "Your continuity workspace — remembers what matters, recalls the rest on demand.";
