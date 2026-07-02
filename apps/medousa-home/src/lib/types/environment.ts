export type ComponentType =
  | "artifact"
  | "medousa_view"
  | "builtin_panel"
  | "presentation"
  | "chrome_action";

export type SurfaceKind = "builtin" | "custom";

export type SurfaceLayout = "single" | "split" | "dashboard";

export type UiPresentation = "inline" | "panel" | "fullscreen";

export type MobileAskEntry = "inline" | "fab" | "tab_only";

export type MobileTabBar = "full" | "minimal";

export type DesktopNavStyle = "rail" | "compact";

export type ActivityRailMode = "visible" | "collapsed" | "hidden";

export interface EnvironmentTheme {
  colorThemeId?: string | null;
  brandColor?: string | null;
  tagline?: string | null;
}

export interface ShellChromeMobile {
  defaultHome?: string | null;
  askEntry?: MobileAskEntry | null;
  tabBar?: MobileTabBar | null;
}

export interface ShellChromeDesktop {
  navStyle?: DesktopNavStyle | null;
  activityRail?: ActivityRailMode | null;
}

export interface ShellChromeDef {
  mobile?: ShellChromeMobile | null;
  desktop?: ShellChromeDesktop | null;
}

export interface SlotDef {
  id: string;
  zone: string;
}

export interface SurfaceDef {
  id: string;
  label: string;
  icon: string;
  kind: SurfaceKind;
  builtinId?: string | null;
  layout: SurfaceLayout;
  slots: SlotDef[];
  mobileTab?: string | null;
}

export interface ComponentDef {
  id: string;
  type: ComponentType;
  surfaceId: string;
  slot: string;
  label?: string | null;
  config: Record<string, unknown>;
  presentation?: UiPresentation | null;
  feeds: string[];
  updatedAt?: string | null;
}

export interface LayoutPreset {
  id: string;
  label: string;
  active: boolean;
  surfaces: string[];
  shellChrome?: ShellChromeDef | null;
}

export interface EnvironmentSpec {
  version: number;
  profileId: string;
  surfaces: SurfaceDef[];
  components: ComponentDef[];
  layoutPresets?: LayoutPreset[] | null;
  activePresetId?: string | null;
  shellChrome?: ShellChromeDef | null;
  theme?: EnvironmentTheme | null;
  updatedAt: string;
  updatedBy: string;
}

export interface EnvironmentSpecResponse {
  spec: EnvironmentSpec;
  revision: number;
}

export interface EnvironmentSpecPutRequest {
  spec: EnvironmentSpec;
}

export interface EnvironmentStreamEvent {
  revision: number;
  eventType: string;
  emittedAtUtc: string;
  spec?: EnvironmentSpec | null;
  componentPatches?: ComponentFeedPatch[] | null;
  feedEvent?: FeedEvent | null;
}

export interface FeedRef {
  refType: string;
  refId: string;
}

export interface FeedEvent {
  id: string;
  feedId: string;
  emittedAtUtc: string;
  source: string;
  summary: string;
  refs?: FeedRef[];
  payload?: Record<string, unknown> | null;
}

export interface ComponentFeedPatch {
  componentId: string;
  feedId: string;
  patch: Record<string, unknown>;
  seq: number;
}

export interface EnvironmentPendingProposal {
  proposedSpec: EnvironmentSpec;
  diffSummary: string;
  errors: string[];
  proposedAt: string;
  proposedBy: string;
}

export interface EnvironmentPendingResponse {
  pending?: EnvironmentPendingProposal | null;
}

export const SAFETY_SURFACE_SETTINGS = "settings";
export const SAFETY_SURFACE_RUNTIME = "runtime";
