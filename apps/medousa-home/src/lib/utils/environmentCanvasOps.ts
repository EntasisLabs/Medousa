import type {
  ComponentDef,
  EnvironmentSpec,
  LayoutNode,
  SurfaceDef,
  SurfaceLayout,
} from "$lib/types/environment";
import { cloneLayoutNode } from "$lib/utils/layoutEditOps";
import { isAllowedSurfaceIcon } from "$lib/utils/environmentIconCatalog";
import {
  isAllowedMediaEmbedUrl,
  normalizeAppleMusicEmbedUrl,
  normalizeSpotifyEmbedUrl,
  parseMediaEmbedProvider,
  type MediaEmbedProvider,
} from "$lib/utils/mediaEmbed";

/** Plain JSON clone — structuredClone fails on Svelte $state proxies. */
export function cloneEnvironmentSpec(spec: EnvironmentSpec): EnvironmentSpec {
  return JSON.parse(JSON.stringify(spec)) as EnvironmentSpec;
}

export function slugifyCanvasId(value: string): string {
  return value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 48);
}

export function uniqueComponentId(spec: EnvironmentSpec, base: string): string {
  const root = slugifyCanvasId(base) || "widget";
  if (!spec.components.some((component) => component.id === root)) return root;
  let index = 2;
  while (spec.components.some((component) => component.id === `${root}-${index}`)) {
    index += 1;
  }
  return `${root}-${index}`;
}

export function surfaceExists(spec: EnvironmentSpec, surfaceId: string): boolean {
  return spec.surfaces.some((surface) => surface.id === surfaceId);
}

export function makeCustomSurfaceDef(input: {
  id: string;
  label: string;
  icon: string;
  layout?: SurfaceLayout;
}): SurfaceDef {
  return {
    id: input.id,
    label: input.label,
    icon: input.icon,
    kind: "custom",
    layout: input.layout ?? "dashboard",
    slots: [],
    mobileTab: null,
    layoutRoot: null,
  };
}

export function insertSurfaceInPreset(
  spec: EnvironmentSpec,
  presetId: string,
  surfaceId: string,
  afterSurfaceId: string | null,
): void {
  const presets = spec.layoutPresets ?? [];
  const preset = presets.find((entry) => entry.id === presetId);
  if (!preset) return;
  if (preset.surfaces.includes(surfaceId)) return;
  if (afterSurfaceId == null || afterSurfaceId === "") {
    preset.surfaces.push(surfaceId);
    return;
  }
  const index = preset.surfaces.indexOf(afterSurfaceId);
  if (index === -1) {
    preset.surfaces.push(surfaceId);
    return;
  }
  preset.surfaces.splice(index + 1, 0, surfaceId);
}

export function addCustomSurfaceToSpec(
  spec: EnvironmentSpec,
  input: {
    id: string;
    label: string;
    icon: string;
    layout?: SurfaceLayout;
    presetId?: string | null;
    afterSurfaceId?: string | null;
  },
): void {
  const id = slugifyCanvasId(input.id);
  const label = input.label.trim();
  const icon = input.icon.trim();
  if (!id || !label) {
    throw new Error("View name and id are required.");
  }
  if (!isAllowedSurfaceIcon(icon)) {
    throw new Error(`Icon "${icon}" is not allowed.`);
  }
  if (surfaceExists(spec, id)) {
    throw new Error(`A view with id "${id}" already exists.`);
  }
  spec.surfaces.push(
    makeCustomSurfaceDef({ id, label, icon, layout: input.layout ?? "dashboard" }),
  );
  const presetId =
    input.presetId ??
    spec.layoutPresets?.find((preset) => preset.active)?.id ??
    spec.activePresetId ??
    "default";
  insertSurfaceInPreset(spec, presetId, id, input.afterSurfaceId ?? null);
  spec.updatedAt = new Date().toISOString();
  spec.updatedBy = "operator";
}

export function addPresentationComponentToSpec(
  spec: EnvironmentSpec,
  input: {
    surfaceId: string;
    artifactId: string;
    label: string;
    componentId?: string | null;
    presentation?: ComponentDef["presentation"];
  },
): ComponentDef {
  const surface = spec.surfaces.find((entry) => entry.id === input.surfaceId);
  if (!surface || surface.kind !== "custom") {
    throw new Error("Widgets can only be added to custom views.");
  }
  const componentId =
    input.componentId?.trim() ||
    uniqueComponentId(spec, `${input.surfaceId}-${slugifyCanvasId(input.label)}`);
  if (spec.components.some((component) => component.id === componentId)) {
    throw new Error(`Component "${componentId}" already exists.`);
  }
  const component: ComponentDef = {
    id: componentId,
    type: "presentation",
    surfaceId: input.surfaceId,
    slot: "main",
    label: input.label.trim() || "Widget",
    config: { artifactId: input.artifactId },
    presentation: input.presentation ?? "inline",
    feeds: [],
    updatedAt: new Date().toISOString(),
  };
  spec.components.push(component);
  spec.updatedAt = new Date().toISOString();
  spec.updatedBy = "operator";
  return component;
}

export function addMediaEmbedComponentToSpec(
  spec: EnvironmentSpec,
  input: {
    surfaceId: string;
    provider: MediaEmbedProvider;
    embedUrl: string;
    label: string;
    componentId?: string | null;
  },
): ComponentDef {
  const surface = spec.surfaces.find((entry) => entry.id === input.surfaceId);
  if (!surface || surface.kind !== "custom") {
    throw new Error("Media widgets can only be added to custom views.");
  }
  const provider = parseMediaEmbedProvider(input.provider);
  if (!provider) {
    throw new Error("Provider must be spotify or apple_music.");
  }
  const normalized =
    provider === "spotify"
      ? normalizeSpotifyEmbedUrl(input.embedUrl)
      : normalizeAppleMusicEmbedUrl(input.embedUrl);
  if (!normalized || !isAllowedMediaEmbedUrl(provider, normalized)) {
    throw new Error("Enter a valid Spotify or Apple Music share/embed URL.");
  }
  const componentId =
    input.componentId?.trim() ||
    uniqueComponentId(spec, `${input.surfaceId}-${provider}`);
  if (spec.components.some((component) => component.id === componentId)) {
    throw new Error(`Component "${componentId}" already exists.`);
  }
  const component: ComponentDef = {
    id: componentId,
    type: "media_embed",
    surfaceId: input.surfaceId,
    slot: "main",
    label: input.label.trim() || (provider === "spotify" ? "Spotify" : "Apple Music"),
    config: { provider, embedUrl: normalized },
    presentation: "inline",
    feeds: [],
    updatedAt: new Date().toISOString(),
  };
  spec.components.push(component);
  spec.updatedAt = new Date().toISOString();
  spec.updatedBy = "operator";
  return component;
}

export function addMedousaViewComponentToSpec(
  spec: EnvironmentSpec,
  input: {
    surfaceId: string;
    notePath: string;
    label?: string | null;
    componentId?: string | null;
  },
): ComponentDef {
  const surface = spec.surfaces.find((entry) => entry.id === input.surfaceId);
  if (!surface || surface.kind !== "custom") {
    throw new Error("Vault note widgets can only be added to custom views.");
  }
  const notePath = input.notePath.trim();
  if (!notePath) {
    throw new Error("A vault note path is required.");
  }
  if (notePath.includes("..")) {
    throw new Error("Invalid note path.");
  }
  const label = input.label?.trim() || notePath.split("/").pop()?.replace(/\.md$/i, "") || "Note";
  const componentId =
    input.componentId?.trim() ||
    uniqueComponentId(spec, `${input.surfaceId}-${slugifyCanvasId(label)}`);
  if (spec.components.some((component) => component.id === componentId)) {
    throw new Error(`Component "${componentId}" already exists.`);
  }
  const component: ComponentDef = {
    id: componentId,
    type: "medousa_view",
    surfaceId: input.surfaceId,
    slot: "main",
    label,
    config: { notePath },
    presentation: "inline",
    feeds: [],
    updatedAt: new Date().toISOString(),
  };
  spec.components.push(component);
  spec.updatedAt = new Date().toISOString();
  spec.updatedBy = "operator";
  return component;
}

export function configString(
  config: Record<string, unknown>,
  key: string,
): string | null {
  const camel = config[key];
  if (typeof camel === "string" && camel.trim()) return camel.trim();
  const snake = config[key.replace(/[A-Z]/g, (char) => `_${char.toLowerCase()}`)];
  return typeof snake === "string" && snake.trim() ? snake.trim() : null;
}

export function componentArtifactId(component: ComponentDef): string | null {
  return configString(component.config, "artifactId");
}

export function pruneLayoutNode(
  node: LayoutNode | null | undefined,
  componentId: string,
): LayoutNode | null {
  if (!node) return null;
  if (node.type === "component") {
    return node.id === componentId ? null : node;
  }
  if (node.type === "slot") return node;
  const children = node.children
    .map((child) => pruneLayoutNode(child, componentId))
    .filter((child): child is LayoutNode => child !== null);
  if (children.length === 0) return null;
  return { ...node, children };
}

export function pruneSurfaceLayoutRoots(spec: EnvironmentSpec, componentId: string): void {
  for (const surface of spec.surfaces) {
    if (!surface.layoutRoot) continue;
    const pruned = pruneLayoutNode(surface.layoutRoot, componentId);
    surface.layoutRoot = pruned;
  }
}

export function removeCustomSurfaceFromSpec(spec: EnvironmentSpec, surfaceId: string): void {
  spec.surfaces = spec.surfaces.filter((surface) => surface.id !== surfaceId);
  for (const preset of spec.layoutPresets ?? []) {
    preset.surfaces = preset.surfaces.filter((id) => id !== surfaceId);
    if (preset.shellChrome?.mobile?.defaultHome === surfaceId) {
      preset.shellChrome = {
        ...preset.shellChrome,
        mobile: {
          ...preset.shellChrome.mobile,
          defaultHome: "home",
        },
      };
    }
  }
  if (spec.shellChrome?.mobile?.defaultHome === surfaceId) {
    setMobileDefaultHome(spec, "home");
  }
  spec.components = spec.components.filter((component) => component.surfaceId !== surfaceId);
}

/** Persist which surface the mobile Home tab shows (`home` = native HomePanel). */
export function setMobileDefaultHome(spec: EnvironmentSpec, surfaceId: string): void {
  const id = surfaceId.trim() || "home";
  if (id !== "home" && !spec.surfaces.some((surface) => surface.id === id)) {
    throw new Error(`Unknown surface '${id}'.`);
  }
  if (id !== "home") {
    const surface = spec.surfaces.find((entry) => entry.id === id);
    if (surface?.kind !== "custom") {
      throw new Error("Mobile home must be native Home or a custom view.");
    }
  }
  const mobile = {
    ...(spec.shellChrome?.mobile ?? {}),
    defaultHome: id,
  };
  spec.shellChrome = {
    ...(spec.shellChrome ?? {}),
    mobile,
  };
  const active =
    spec.layoutPresets?.find((preset) => preset.active) ??
    spec.layoutPresets?.find((preset) => preset.id === spec.activePresetId);
  if (active) {
    active.shellChrome = {
      ...(active.shellChrome ?? {}),
      mobile: {
        ...(active.shellChrome?.mobile ?? {}),
        defaultHome: id,
      },
    };
  }
  spec.updatedAt = new Date().toISOString();
  spec.updatedBy = "operator";
}

export function updateCustomSurfaceInSpec(
  spec: EnvironmentSpec,
  surfaceId: string,
  input: { label?: string; icon?: string },
): void {
  const surface = spec.surfaces.find((entry) => entry.id === surfaceId);
  if (!surface) {
    throw new Error(`Unknown surface '${surfaceId}'.`);
  }
  if (surface.kind !== "custom") {
    throw new Error("Only custom views can be edited.");
  }
  if (input.label !== undefined) {
    const label = input.label.trim();
    if (!label) {
      throw new Error("View name is required.");
    }
    surface.label = label;
  }
  if (input.icon !== undefined) {
    if (!isAllowedSurfaceIcon(input.icon)) {
      throw new Error(`Icon '${input.icon}' is not allowed.`);
    }
    surface.icon = input.icon;
  }
  spec.updatedAt = new Date().toISOString();
  spec.updatedBy = "operator";
}

export function removeComponentFromSpec(spec: EnvironmentSpec, componentId: string): void {
  spec.components = spec.components.filter((component) => component.id !== componentId);
  pruneSurfaceLayoutRoots(spec, componentId);
}

export function removeComponentsReferencingArtifacts(
  spec: EnvironmentSpec,
  artifactIds: string[],
): string[] {
  const idSet = new Set(artifactIds);
  const removed: string[] = [];
  for (const component of [...spec.components]) {
    const artifactId = componentArtifactId(component);
    if (artifactId && idSet.has(artifactId)) {
      removeComponentFromSpec(spec, component.id);
      removed.push(component.id);
    }
  }
  return removed;
}

export function updateComponentArtifactId(
  spec: EnvironmentSpec,
  componentId: string,
  artifactId: string,
): void {
  const component = spec.components.find((entry) => entry.id === componentId);
  if (!component) return;
  component.config = { ...component.config, artifactId };
}

export function applyLayoutRoot(
  spec: EnvironmentSpec,
  surfaceId: string,
  layoutRoot: LayoutNode | null,
): EnvironmentSpec {
  const next = cloneEnvironmentSpec(spec);
  const surface = next.surfaces.find((entry) => entry.id === surfaceId);
  if (!surface) return next;
  surface.layoutRoot = layoutRoot;
  return next;
}

export function ensureSurfaceLayoutRoot(
  spec: EnvironmentSpec,
  surfaceId: string,
  fallback: LayoutNode,
): LayoutNode {
  const surface = spec.surfaces.find((entry) => entry.id === surfaceId);
  if (surface?.layoutRoot) return cloneLayoutNode(surface.layoutRoot);
  return cloneLayoutNode(fallback);
}
