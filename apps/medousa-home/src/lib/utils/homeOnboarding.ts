import type {
  ActivityRailMode,
  EnvironmentSpec,
  ShellChromeDesktop,
} from "$lib/types/environment";
import { SAFETY_SURFACE_RUNTIME, SAFETY_SURFACE_SETTINGS } from "$lib/types/environment";

export type HomeOnboardingFocus = "code" | "messaging" | "notes" | "plan" | "ai";
export type HomeOnboardingLayout = "focused" | "split" | "dashboard";
export type HomeOnboardingChannel = "discord" | "slack" | "telegram" | "whatsapp";
export type HomeOnboardingStage = "focus" | "layout" | "style" | "brain" | "ready";

const STORAGE_KEY = "medousa-home-onboarding-draft-v1";

export interface HomeOnboardingDraft {
  stage: HomeOnboardingStage;
  focus: HomeOnboardingFocus[];
  layout: HomeOnboardingLayout;
  channels: HomeOnboardingChannel[];
}

export interface HomeOnboardingTask {
  label: string;
  run: () => Promise<void>;
}

/** First-run customization must never become a connection gate. */
export async function runHomeOnboardingTasks(
  tasks: readonly HomeOnboardingTask[],
): Promise<string[]> {
  const results = await Promise.allSettled(tasks.map((task) => task.run()));
  return results.flatMap((result, index) =>
    result.status === "rejected" ? [tasks[index]!.label] : [],
  );
}

const DEFAULT_DRAFT: HomeOnboardingDraft = {
  stage: "focus",
  focus: [],
  layout: "split",
  channels: [],
};

export function loadHomeOnboardingDraft(): HomeOnboardingDraft {
  if (typeof localStorage === "undefined") return { ...DEFAULT_DRAFT };
  try {
    const parsed = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "null") as
      | Partial<HomeOnboardingDraft>
      | null;
    const focus = (parsed?.focus ?? []).filter(isHomeOnboardingFocus);
    const channels = (parsed?.channels ?? []).filter(isHomeOnboardingChannel);
    return {
      stage: isHomeOnboardingStage(parsed?.stage) ? parsed.stage : "focus",
      focus,
      layout: isHomeOnboardingLayout(parsed?.layout) ? parsed.layout : "split",
      channels,
    };
  } catch {
    return { ...DEFAULT_DRAFT };
  }
}

export function saveHomeOnboardingDraft(draft: HomeOnboardingDraft): void {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(STORAGE_KEY, JSON.stringify(draft));
}

export function resetHomeOnboardingDraft(): void {
  if (typeof localStorage === "undefined") return;
  localStorage.removeItem(STORAGE_KEY);
}

function isHomeOnboardingFocus(value: unknown): value is HomeOnboardingFocus {
  return ["code", "messaging", "notes", "plan", "ai"].includes(String(value));
}

function isHomeOnboardingChannel(value: unknown): value is HomeOnboardingChannel {
  return ["discord", "slack", "telegram", "whatsapp"].includes(String(value));
}

function isHomeOnboardingLayout(value: unknown): value is HomeOnboardingLayout {
  return ["focused", "split", "dashboard"].includes(String(value));
}

function isHomeOnboardingStage(value: unknown): value is HomeOnboardingStage {
  return ["focus", "layout", "style", "brain", "ready"].includes(String(value));
}

const FOCUS_SURFACES: Record<HomeOnboardingFocus, string[]> = {
  code: ["code"],
  messaging: ["messaging", "chat"],
  notes: ["notes", "files", "artifacts", "web"],
  plan: ["work", "calendar", "automations"],
  ai: ["chat", "map"],
};

export function onboardingSurfaceOrder(focus: readonly HomeOnboardingFocus[]): string[] {
  const ordered = ["home"];
  for (const item of focus) {
    for (const surfaceId of FOCUS_SURFACES[item]) {
      if (!ordered.includes(surfaceId)) ordered.push(surfaceId);
    }
  }
  for (const required of [SAFETY_SURFACE_SETTINGS, SAFETY_SURFACE_RUNTIME]) {
    if (!ordered.includes(required)) ordered.push(required);
  }
  return ordered;
}

export function onboardingWorkspaceSurfaces(
  focus: readonly HomeOnboardingFocus[],
): string[] {
  const order = onboardingSurfaceOrder(focus).filter(
    (id) => id !== "home" && id !== SAFETY_SURFACE_SETTINGS && id !== SAFETY_SURFACE_RUNTIME,
  );
  return order.length > 0 ? order : ["notes"];
}

export function onboardingPackageIds(
  focus: readonly HomeOnboardingFocus[],
  channels: readonly HomeOnboardingChannel[],
): string[] {
  const ids: string[] = [];
  if (focus.includes("code")) {
    ids.push("coding-engine", "langservers", "shell-session");
  }
  if (focus.includes("messaging")) {
    ids.push(...channels.map((channel) => `adapter-${channel}`));
  }
  return ids;
}

function shellChromeForLayout(layout: HomeOnboardingLayout): ShellChromeDesktop {
  const activityRail: ActivityRailMode =
    layout === "dashboard" ? "visible" : layout === "split" ? "collapsed" : "hidden";
  return {
    navStyle: layout === "focused" ? "compact" : "rail",
    activityRail,
    vaultChatFab: true,
    vaultSidebar: layout === "focused" ? "hidden" : "visible",
  };
}

export function applyHomeOnboardingEnvironment(
  spec: EnvironmentSpec,
  focus: readonly HomeOnboardingFocus[],
  layout: HomeOnboardingLayout,
): EnvironmentSpec {
  const surfaces = onboardingSurfaceOrder(focus);
  const desktop = shellChromeForLayout(layout);
  const presets = spec.layoutPresets ?? [];
  const active =
    presets.find((preset) => preset.active) ??
    presets.find((preset) => preset.id === spec.activePresetId) ??
    presets[0];

  if (active) {
    active.surfaces = surfaces;
    active.shellChrome = {
      ...(active.shellChrome ?? {}),
      desktop,
    };
  }
  spec.shellChrome = {
    ...(spec.shellChrome ?? {}),
    desktop,
  };
  spec.updatedAt = new Date().toISOString();
  spec.updatedBy = "operator";
  return spec;
}
