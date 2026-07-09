const CATEGORY_LABELS: Record<string, string> = {
  core: "Core",
  adapter: "Messaging",
  model: "AI models",
  expansion: "Expansions",
};

/** Benefit-focused blurbs — what the user gets, not what we ship. */
export const PACKAGE_BLURBS: Record<string, string> = {
  desktop: "The Medousa app on your computer.",
  engine: "Runs in the background and powers your workspace.",
  "local-brain": "On-device AI — works without the cloud.",
  cli: "Terminal tools for scripting and automation.",
  "adapter-telegram": "Chat with Medousa from Telegram.",
  "adapter-discord": "Use Medousa inside Discord servers.",
  "adapter-slack": "Bring Medousa into your Slack workspace.",
  "adapter-whatsapp": "Message Medousa from WhatsApp.",
  "mcp-gateway": "Connect external tools via the MCP protocol.",
  "model-gemma-e2b": "Lightweight — good for quick replies on modest hardware.",
  "model-gemma-e4b": "Balanced speed and quality for everyday use.",
  "model-gemma-12b": "Best quality — needs more disk space and RAM.",
  "skill-hub": "Extra capabilities from the Medousa catalog.",
  "grapheme-module-starter": "Starter pack for Grapheme modules.",
};

export const PRESET_CHIPS: {
  id: string;
  label: string;
  description: string;
  profileId: string;
}[] = [
  {
    id: "offline",
    label: "Offline AI",
    description: "On-device brain + a Gemma model",
    profileId: "offline-workstation",
  },
  {
    id: "developer",
    label: "Developer tools",
    description: "CLI and MCP gateway",
    profileId: "developer",
  },
];

export function categoryLabel(category: string): string {
  return CATEGORY_LABELS[category] ?? category;
}

export function packageBlurb(packageId: string, fallbackName: string): string {
  return PACKAGE_BLURBS[packageId] ?? fallbackName;
}

export function humanizeWarning(warning: string): string {
  let text = warning.replace(/https?:\/\/\S+/g, "").trim();
  text = text.replace(/\s{2,}/g, " ");

  if (text.includes("not available for")) {
    return "Some selected items aren't available on your platform.";
  }
  if (text.includes("differs from release")) {
    return "An update is available for your installation.";
  }
  if (text.includes("release package not found")) {
    return "A selected item isn't in the current release.";
  }

  return text || "Review your selection before continuing.";
}

/** Turn backend / network errors into something a human can act on. */
export function humanizeError(raw: string): string {
  const text = raw.replace(/https?:\/\/\S+/g, "").trim();

  if (/404|not found/i.test(text) && /manifest|bootstrap|release/i.test(text)) {
    return "We couldn't reach the download server. Check your internet connection and try again.";
  }
  if (/failed to fetch|network|connection|timed out|dns/i.test(text)) {
    return "We couldn't connect to the download server. Check your internet connection and try again.";
  }
  if (/403|401|forbidden|unauthorized/i.test(text)) {
    return "Access to the download server was denied. Try again later.";
  }
  if (/disk|space|enospc/i.test(text)) {
    return "Not enough disk space. Free up space and try again.";
  }
  if (/permission|access denied/i.test(text)) {
    return "Medousa doesn't have permission to write to that folder. Choose a different location.";
  }

  const cleaned = text.replace(/\s{2,}/g, " ").replace(/:\s*$/, "");
  if (!cleaned || cleaned.length > 200) {
    return "Something went wrong. Please try again.";
  }
  return cleaned;
}

export function truncatePath(path: string, max = 42): string {
  if (path.length <= max) return path;
  const head = Math.ceil((max - 3) / 2);
  const tail = Math.floor((max - 3) / 2);
  return `${path.slice(0, head)}…${path.slice(-tail)}`;
}

export const INSTALLER_TAGLINE =
  "Private AI for your life — on your computer, under your control.";

export const INSTALLER_SUBLINE =
  "Desktop app and background service included. Add messaging channels or offline AI if you want them.";
