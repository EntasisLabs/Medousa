import type {
  ActivityRailMode,
  EnvironmentSpec,
  ShellChromeDesktop,
  VaultSidebarMode,
} from "$lib/types/environment";
import type { PreferredMode } from "$lib/utils/preferredMode";

export interface ResolvedDesktopShellChrome {
  navStyle: "rail" | "compact";
  activityRail: ActivityRailMode;
  vaultChatFab: boolean;
  vaultSidebar: VaultSidebarMode;
}

const DEFAULTS: ResolvedDesktopShellChrome = {
  /** rail = icon + labels (default); compact = icon-only power path */
  navStyle: "rail",
  activityRail: "visible",
  vaultChatFab: true,
  vaultSidebar: "visible",
};

/** Resolve desktop shell chrome with defaults for unset fields. */
export function resolveDesktopShellChrome(
  spec?: EnvironmentSpec | null,
): ResolvedDesktopShellChrome {
  const desktop = spec?.shellChrome?.desktop;
  return {
    navStyle: desktop?.navStyle ?? DEFAULTS.navStyle,
    activityRail: desktop?.activityRail ?? DEFAULTS.activityRail,
    vaultChatFab: desktop?.vaultChatFab ?? DEFAULTS.vaultChatFab,
    vaultSidebar: desktop?.vaultSidebar ?? DEFAULTS.vaultSidebar,
  };
}

/** Preferred-mode seed values — only applied when fields are still unset. */
export function preferredModeDesktopChromeSeed(
  mode: PreferredMode,
): Partial<ShellChromeDesktop> {
  if (mode === "workspace") {
    return {
      vaultChatFab: false,
      activityRail: "collapsed",
    };
  }
  return {
    vaultChatFab: true,
    activityRail: "visible",
  };
}

/** True when a desktop chrome field is still unset (null/undefined). */
export function isDesktopChromeFieldUnset(
  desktop: ShellChromeDesktop | null | undefined,
  field: keyof ShellChromeDesktop,
): boolean {
  const value = desktop?.[field];
  return value === undefined || value === null;
}

/**
 * Seed desktop chrome from preferred mode without clobbering existing user edits.
 * Returns true if any field was written.
 */
export function seedDesktopShellChromeFromPreferredMode(
  spec: EnvironmentSpec,
  mode: PreferredMode,
): boolean {
  const seed = preferredModeDesktopChromeSeed(mode);
  const desktop = spec.shellChrome?.desktop ?? null;
  const patch: Partial<ShellChromeDesktop> = {};
  for (const [key, value] of Object.entries(seed) as Array<
    [keyof ShellChromeDesktop, ShellChromeDesktop[keyof ShellChromeDesktop]]
  >) {
    if (isDesktopChromeFieldUnset(desktop, key) && value !== undefined) {
      (patch as Record<string, unknown>)[key] = value;
    }
  }
  if (Object.keys(patch).length === 0) return false;
  applyDesktopShellChromePatch(spec, patch);
  return true;
}

/** Shared patch used by canvas ops + preferred-mode seed. */
export function applyDesktopShellChromePatch(
  spec: EnvironmentSpec,
  patch: Partial<ShellChromeDesktop>,
): void {
  const desktop: ShellChromeDesktop = {
    ...(spec.shellChrome?.desktop ?? {}),
    ...patch,
  };
  spec.shellChrome = {
    ...(spec.shellChrome ?? {}),
    desktop,
  };
  const active =
    spec.layoutPresets?.find((preset) => preset.active) ??
    spec.layoutPresets?.find((preset) => preset.id === spec.activePresetId);
  if (active) {
    active.shellChrome = {
      ...(active.shellChrome ?? {}),
      desktop: {
        ...(active.shellChrome?.desktop ?? {}),
        ...patch,
      },
    };
  }
  spec.updatedAt = new Date().toISOString();
  spec.updatedBy = "operator";
}
