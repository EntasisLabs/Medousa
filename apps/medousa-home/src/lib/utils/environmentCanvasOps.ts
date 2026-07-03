import type { ComponentDef, EnvironmentSpec, LayoutNode } from "$lib/types/environment";

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
  }
  spec.components = spec.components.filter((component) => component.surfaceId !== surfaceId);
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
