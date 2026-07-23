import type { MobileTab, MoreDestination } from "$lib/types/mobile";
import type { Surface } from "$lib/types/ui";
import { shouldUseMobileShell } from "$lib/platform";
import { surfaceHasShellSidebarView } from "$lib/utils/navSurfaces";

const LAST_SURFACE_KEY = "medousa-home-last-surface";
const LANDING_SURFACES: Surface[] = [
  "chat",
  "work",
  "library",
  "web",
  "workshop",
  "peers",
  "messaging",
  "settings",
  "calendar",
  "context",
  "runtime",
  "profiles",
];

const ACTIVITY_WIDTH_KEY = "medousa-home-activity-width";
const SHELL_SIDEBAR_WIDTH_KEY = "medousa-home-shell-sidebar-width";
const VAULT_TREE_WIDTH_KEY = "medousa-home-vault-tree-width";

export const SHELL_SIDEBAR_WIDTH_MIN = 200;
export const SHELL_SIDEBAR_WIDTH_MAX = 420;
export const SHELL_SIDEBAR_WIDTH_DEFAULT = 248;
const VAULT_EDITOR_PANE_WIDTH_KEY = "medousa-home-vault-editor-pane-width";
const VAULT_SPLIT_ENABLED_KEY = "medousa-home-vault-split-enabled";
const VAULT_LINKS_PANEL_KEY = "medousa-home-vault-links-panel";
const WORK_INSPECTOR_WIDTH_KEY = "medousa-home-work-inspector-width";
const SESSION_DRAWER_KEY = "medousa-home-session-drawer";
const IDENTITY_DRAWER_KEY = "medousa-home-identity-drawer";
const ACTIVITY_COLLAPSED_KEY = "medousa-home-activity-collapsed";
const VAULT_SIDEBAR_COLLAPSED_KEY = "medousa-home-vault-sidebar-collapsed";
/** Persists left master rail visible/hidden (`navStyle` rail=visible, compact=hidden). */
const SHELL_SIDEBAR_EXPANDED_KEY = "medousa-home-shell-sidebar-expanded";
const NAV_EXPANDED_KEY = "medousa-home-nav-expanded";
const MOBILE_TAB_KEY = "medousa-home-mobile-tab";
const MORE_DESTINATION_KEY = "medousa-home-you-destination";
const LIBRARY_VIEW_KEY = "medousa-home-library-view";

export type LibraryView = "list" | "reader";
/** Master rail content: destination nav, or the active view’s list in the same rail. */
export type ShellSidebarMode = "nav" | "view";

function loadShellSidebarExpanded(): boolean {
  if (typeof localStorage === "undefined") return true;
  const shell = localStorage.getItem(SHELL_SIDEBAR_EXPANDED_KEY);
  if (shell === "1") return true;
  if (shell === "0") return false;
  // Migrate prior nav-labels flag.
  const legacy = localStorage.getItem(NAV_EXPANDED_KEY);
  if (legacy === "0") return false;
  if (legacy === "1") return true;
  // Prefer previous vault sidebar open state when present.
  const vault = localStorage.getItem(VAULT_SIDEBAR_COLLAPSED_KEY);
  if (vault === "1") return false;
  return true;
}

export class LayoutStore {
  isMobile = $state(
    typeof window !== "undefined" ? shouldUseMobileShell() : false,
  );
  /** Desktop primary panel (WorkshopShell). */
  desktopSurface = $state<Surface>(loadLastSurface());
  /** Bumped on every explicit nav action so keyed panels remount even when revisiting the same surface/tab. */
  navigationEpoch = $state(0);
  mobileTab = $state<MobileTab>(loadMobileTab());
  moreDestination = $state<MoreDestination>(loadMoreDestination());
  mobileSurfaceOverride = $state<string | null>(null);
  libraryView = $state<LibraryView>(loadLibraryView());
  libraryListScrollTop = $state(0);
  activitySheetOpen = $state(false);
  askSheetOpen = $state(false);
  activityWidth = $state(loadWidth(ACTIVITY_WIDTH_KEY, 288));
  /** Master left rail width when visible (px). */
  shellSidebarWidth = $state(
    loadWidth(SHELL_SIDEBAR_WIDTH_KEY, SHELL_SIDEBAR_WIDTH_DEFAULT),
  );
  vaultTreeWidth = $state(loadWidth(VAULT_TREE_WIDTH_KEY, 224));
  vaultEditorPaneWidth = $state(loadWidth(VAULT_EDITOR_PANE_WIDTH_KEY, 420));
  vaultSplitEnabled = $state(loadFlag(VAULT_SPLIT_ENABLED_KEY, true));
  vaultLinksPanelOpen = $state(loadFlag(VAULT_LINKS_PANEL_KEY, false));
  workInspectorWidth = $state(loadWidth(WORK_INSPECTOR_WIDTH_KEY, 360));
  sessionDrawerOpen = $state(loadFlag(SESSION_DRAWER_KEY, false));
  identityDrawerOpen = $state(loadFlag(IDENTITY_DRAWER_KEY, false));
  activityCollapsed = $state(loadFlag(ACTIVITY_COLLAPSED_KEY, false));
  /**
   * Master rail visible (true) vs fully hidden (false). No icon-strip intermediate.
   * Kept in sync with vaultSidebarCollapsed (inverted) and legacy navExpanded.
   */
  shellSidebarExpanded = $state(loadShellSidebarExpanded());
  /** nav = destinations in the rail; view = a list surface hosted in the same rail. */
  shellSidebarMode = $state<ShellSidebarMode>(
    surfaceHasShellSidebarView(loadLastSurface()) ? "view" : "nav",
  );
  /**
   * Which list the rail shows in view mode. Can differ from {@link desktopSurface}
   * so docking a popover doesn't remount / activate main content.
   */
  shellSidebarViewSurface = $state<string | null>(
    surfaceHasShellSidebarView(loadLastSurface()) ? loadLastSurface() : null,
  );
  /** Per-surface rail mode — survive chat ↔ Workspace switches. */
  private sidebarModeBySurface: Record<string, ShellSidebarMode> = {};
  /** @deprecated Use !shellSidebarExpanded — kept for LME call sites. */
  vaultSidebarCollapsed = $state(!loadShellSidebarExpanded());
  /** @deprecated Alias of shellSidebarExpanded (navStyle rail=visible / compact=hidden). */
  navExpanded = $state(loadShellSidebarExpanded());
  viewportWidth = $state(
    typeof window !== "undefined" ? window.innerWidth : 1280,
  );
  viewportHeight = $state(
    typeof window !== "undefined" ? window.innerHeight : 800,
  );
  /** True when the dedicated browser window is mounted (desktop). */
  browserWindowActive = $state(false);

  setBrowserWindowActive(active: boolean) {
    this.browserWindowActive = active;
  }

  attachViewportTracking(): () => void {
    if (typeof window === "undefined") return () => {};
    const update = () => {
      this.viewportWidth = window.innerWidth;
      this.viewportHeight = window.innerHeight;
    };
    update();
    window.addEventListener("resize", update);
    return () => window.removeEventListener("resize", update);
  }

  setActivityWidth(width: number) {
    this.activityWidth = clamp(width, 220, 520);
    localStorage.setItem(ACTIVITY_WIDTH_KEY, String(this.activityWidth));
  }

  setShellSidebarWidth(width: number) {
    this.shellSidebarWidth = clamp(
      width,
      SHELL_SIDEBAR_WIDTH_MIN,
      SHELL_SIDEBAR_WIDTH_MAX,
    );
    localStorage.setItem(SHELL_SIDEBAR_WIDTH_KEY, String(this.shellSidebarWidth));
  }

  setVaultTreeWidth(width: number) {
    this.vaultTreeWidth = clamp(width, 180, 420);
    localStorage.setItem(VAULT_TREE_WIDTH_KEY, String(this.vaultTreeWidth));
  }

  setVaultEditorPaneWidth(width: number) {
    this.vaultEditorPaneWidth = clamp(width, 280, 720);
    localStorage.setItem(VAULT_EDITOR_PANE_WIDTH_KEY, String(this.vaultEditorPaneWidth));
  }

  setVaultSplitEnabled(enabled: boolean) {
    this.vaultSplitEnabled = enabled;
    localStorage.setItem(VAULT_SPLIT_ENABLED_KEY, enabled ? "1" : "0");
  }

  toggleVaultSplitEnabled() {
    this.setVaultSplitEnabled(!this.vaultSplitEnabled);
  }

  setVaultLinksPanelOpen(open: boolean) {
    this.vaultLinksPanelOpen = open;
    localStorage.setItem(VAULT_LINKS_PANEL_KEY, open ? "1" : "0");
  }

  toggleVaultLinksPanel() {
    this.setVaultLinksPanelOpen(!this.vaultLinksPanelOpen);
  }

  setWorkInspectorWidth(width: number) {
    this.workInspectorWidth = clamp(width, 280, 560);
    localStorage.setItem(WORK_INSPECTOR_WIDTH_KEY, String(this.workInspectorWidth));
  }

  setSessionDrawerOpen(open: boolean) {
    this.sessionDrawerOpen = open;
    localStorage.setItem(SESSION_DRAWER_KEY, open ? "1" : "0");
  }

  toggleSessionDrawer() {
    this.setSessionDrawerOpen(!this.sessionDrawerOpen);
  }

  setIdentityDrawerOpen(open: boolean) {
    this.identityDrawerOpen = open;
    localStorage.setItem(IDENTITY_DRAWER_KEY, open ? "1" : "0");
  }

  toggleIdentityDrawer() {
    this.setIdentityDrawerOpen(!this.identityDrawerOpen);
  }

  setActivityCollapsed(collapsed: boolean) {
    this.activityCollapsed = collapsed;
    localStorage.setItem(ACTIVITY_COLLAPSED_KEY, collapsed ? "1" : "0");
  }

  toggleActivityCollapsed() {
    this.setActivityCollapsed(!this.activityCollapsed);
  }

  setShellSidebarExpanded(expanded: boolean) {
    this.shellSidebarExpanded = expanded;
    this.navExpanded = expanded;
    this.vaultSidebarCollapsed = !expanded;
    localStorage.setItem(SHELL_SIDEBAR_EXPANDED_KEY, expanded ? "1" : "0");
    localStorage.setItem(NAV_EXPANDED_KEY, expanded ? "1" : "0");
    localStorage.setItem(VAULT_SIDEBAR_COLLAPSED_KEY, expanded ? "0" : "1");
  }

  toggleShellSidebarExpanded() {
    this.setShellSidebarExpanded(!this.shellSidebarExpanded);
  }

  setShellSidebarMode(mode: ShellSidebarMode) {
    this.shellSidebarMode = mode;
    this.sidebarModeBySurface[this.desktopSurface] = mode;
    if (mode === "nav") {
      this.shellSidebarViewSurface = null;
    }
  }

  /** Leave view list → destination nav in the same expanded rail. */
  shellSidebarBackToNav() {
    this.setShellSidebarMode("nav");
    if (!this.shellSidebarExpanded) {
      this.setShellSidebarExpanded(true);
    }
  }

  /**
   * Host a list surface in the master rail (view mode).
   * Does not change {@link desktopSurface} / main content — callers that need a
   * tab switch should navigate separately.
   */
  openShellSidebarView(surfaceId: string) {
    if (!surfaceHasShellSidebarView(surfaceId)) {
      this.setShellSidebarMode("nav");
      this.setShellSidebarExpanded(true);
      return;
    }
    const viewId = surfaceId === "automations" ? "library" : surfaceId;
    this.shellSidebarViewSurface = viewId;
    this.setShellSidebarMode("view");
    this.setShellSidebarExpanded(true);
  }

  private restoreSidebarModeFor(surface: string): ShellSidebarMode {
    const remembered = this.sidebarModeBySurface[surface];
    if (remembered === "nav" || remembered === "view") return remembered;
    return surfaceHasShellSidebarView(surface) ? "view" : "nav";
  }

  private syncViewSurfaceForMode(surface: string, mode: ShellSidebarMode) {
    this.shellSidebarViewSurface =
      mode === "view" && surfaceHasShellSidebarView(surface) ? surface : null;
  }

  setVaultSidebarCollapsed(collapsed: boolean) {
    this.setShellSidebarExpanded(!collapsed);
  }

  toggleVaultSidebarCollapsed() {
    this.toggleShellSidebarExpanded();
  }

  setNavExpanded(expanded: boolean) {
    this.setShellSidebarExpanded(expanded);
  }

  toggleNavExpanded() {
    this.toggleShellSidebarExpanded();
  }

  setMobile(mobile: boolean) {
    this.isMobile = mobile;
  }

  private bumpNavigation() {
    this.navigationEpoch += 1;
  }

  /**
   * Update rail / last-surface hint without remounting the center column.
   * Used by the shell tab host when activating tabs.
   */
  focusDesktopSurface(surface: string) {
    let next = surface === "home" ? "chat" : surface;
    if (next === "automations" || next === "workshop") next = "library";
    if (this.desktopSurface === next) return;
    this.sidebarModeBySurface[this.desktopSurface] = this.shellSidebarMode;
    this.desktopSurface = next as Surface;
    saveLastSurface(next);
    const mode = this.restoreSidebarModeFor(next);
    this.shellSidebarMode = mode;
    this.syncViewSurfaceForMode(next, mode);
  }

  navigateDesktop(surface: string, options?: { bump?: boolean }) {
    // Legacy Automations surface → LME workspace (library). Callers that need a
    // specific explorer mode should set `lmeWorkspace` before navigating.
    let next = surface === "home" ? "chat" : surface;
    if (next === "automations") {
      next = "library";
      void import("$lib/stores/lmeWorkspace.svelte").then(({ lmeWorkspace }) => {
        const mode = lmeWorkspace.explorerMode;
        if (
          mode !== "scripts" &&
          mode !== "flows" &&
          mode !== "schedules" &&
          mode !== "history"
        ) {
          lmeWorkspace.setExplorerMode("scripts");
        }
      });
    }
    if (next !== "chat") {
      this.setSessionDrawerOpen(false);
      this.setIdentityDrawerOpen(false);
    }
    if (next === "work") {
      this.setActivityCollapsed(true);
    }
    const changed = this.desktopSurface !== next;
    if (changed) {
      this.sidebarModeBySurface[this.desktopSurface] = this.shellSidebarMode;
    }
    this.desktopSurface = next;
    saveLastSurface(next);
    const mode = this.restoreSidebarModeFor(next);
    this.shellSidebarMode = mode;
    this.syncViewSurfaceForMode(next, mode);
    if (changed || options?.bump) {
      this.bumpNavigation();
    }
  }

  setMobileTab(tab: MobileTab, options?: { bump?: boolean }) {
    const changed = this.mobileTab !== tab;
    this.mobileTab = tab;
    if (typeof localStorage !== "undefined") {
      localStorage.setItem(MOBILE_TAB_KEY, tab);
    }
    if (changed || options?.bump) {
      this.bumpNavigation();
    }
  }

  openWeb() {
    this.setMobileTab("web", { bump: true });
  }

  openNotes(options?: { view?: LibraryView }) {
    if (options?.view) {
      this.setLibraryView(options.view);
    }
    this.setMobileTab("notes", { bump: true });
  }

  openMore(destination: MoreDestination) {
    this.moreDestination = destination;
    this.mobileTab = "more";
    if (typeof localStorage !== "undefined") {
      localStorage.setItem(MOBILE_TAB_KEY, "more");
    }
    saveMoreDestination(destination);
    this.bumpNavigation();
  }

  openCustomSurface(surfaceId: string) {
    this.mobileSurfaceOverride = surfaceId;
    this.setMobileTab("home", { bump: true });
  }

  clearMobileSurfaceOverride() {
    this.mobileSurfaceOverride = null;
  }

  effectiveMobileHomeSurface(defaultHome: string): string {
    return this.mobileSurfaceOverride ?? defaultHome;
  }

  backToMoreHub() {
    this.moreDestination = "hub";
    saveMoreDestination("hub");
  }

  setLibraryView(view: LibraryView) {
    this.libraryView = view;
    saveLibraryView(view);
  }

  setLibraryListScrollTop(scrollTop: number) {
    this.libraryListScrollTop = Math.max(0, scrollTop);
  }

  setActivitySheetOpen(open: boolean) {
    this.activitySheetOpen = open;
  }

  toggleActivitySheet() {
    this.setActivitySheetOpen(!this.activitySheetOpen);
  }

  setAskSheetOpen(open: boolean) {
    this.askSheetOpen = open;
  }

  openAskSheet() {
    this.askSheetOpen = true;
  }
}

function loadMobileTab(): MobileTab {
  if (typeof localStorage === "undefined") return "home";
  const stored = localStorage.getItem(MOBILE_TAB_KEY);
  const migrated =
    stored === "pulse" || stored === "work"
      ? "home"
      : stored === "you"
        ? "more"
        : stored;
  if (
    migrated === "home" ||
    migrated === "chat" ||
    migrated === "notes" ||
    migrated === "web" ||
    migrated === "more"
  ) {
    return migrated;
  }
  return "home";
}

function loadMoreDestination(): MoreDestination {
  if (typeof localStorage === "undefined") return "hub";
  const stored = localStorage.getItem(MORE_DESTINATION_KEY);
  const valid: MoreDestination[] = [
    "hub",
    "profiles",
    "context",
    "workshop",
    "automations",
    "calendar",
    "messaging",
    "peers",
    "settings",
    "runtime",
  ];
  if (stored === "library") return "hub";
  if (stored === "web") return "hub";
  if (stored && valid.includes(stored as MoreDestination)) {
    return stored as MoreDestination;
  }
  return "hub";
}

function saveMoreDestination(destination: MoreDestination) {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(MORE_DESTINATION_KEY, destination);
}

function loadLibraryView(): LibraryView {
  if (typeof localStorage === "undefined") return "list";
  const stored = localStorage.getItem(LIBRARY_VIEW_KEY);
  return stored === "reader" ? "reader" : "list";
}

function saveLibraryView(view: LibraryView) {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(LIBRARY_VIEW_KEY, view);
}

function loadWidth(key: string, fallback: number): number {
  if (typeof localStorage === "undefined") return fallback;
  const raw = Number(localStorage.getItem(key));
  return Number.isFinite(raw) ? clamp(raw, 180, 520) : fallback;
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

function loadFlag(key: string, fallback: boolean): boolean {
  if (typeof localStorage === "undefined") return fallback;
  const stored = localStorage.getItem(key);
  if (stored === "1") return true;
  if (stored === "0") return false;
  return fallback;
}

export const layout = new LayoutStore();

export function loadLastSurface(): Surface {
  if (typeof localStorage === "undefined") return "chat";
  const stored = localStorage.getItem(LAST_SURFACE_KEY);
  if (stored === "home") return "chat";
  if (stored && isLandingSurface(stored)) {
    return stored;
  }
  return "chat";
}

export function saveLastSurface(surface: Surface) {
  if (typeof localStorage === "undefined") return;
  if (!isLandingSurface(surface)) return;
  localStorage.setItem(LAST_SURFACE_KEY, surface);
}

function isLandingSurface(value: string): value is Surface {
  return LANDING_SURFACES.includes(value as Surface);
}
