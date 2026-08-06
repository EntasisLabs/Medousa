/** Host surfaces persisted on session catalog `origin_surface`. */
export type SessionChannelSurface = "vscode" | "neovim" | "obsidian" | "browser";

export interface SessionChannelMarks {
  channel: SessionChannelSurface | null;
  hasCodeWork: boolean;
}

const CHANNEL_SURFACES = new Set<string>(["vscode", "neovim", "obsidian", "browser"]);

export function normalizeSessionChannelSurface(
  value: string | null | undefined,
): SessionChannelSurface | null {
  const key = value?.trim().toLowerCase();
  if (!key || !CHANNEL_SURFACES.has(key)) return null;
  return key as SessionChannelSurface;
}

export function resolveSessionChannelMarks(session: {
  origin_surface?: string | null;
  has_code_work?: boolean;
}): SessionChannelMarks {
  return {
    channel: normalizeSessionChannelSurface(session.origin_surface),
    hasCodeWork: Boolean(session.has_code_work),
  };
}

export function sessionChannelTitle(surface: SessionChannelSurface): string {
  switch (surface) {
    case "vscode":
      return "VS Code";
    case "neovim":
      return "Neovim";
    case "obsidian":
      return "Obsidian";
    case "browser":
      return "Browser";
  }
}
