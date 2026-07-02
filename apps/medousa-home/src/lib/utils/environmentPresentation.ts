import type { ComponentDef, SurfaceLayout } from "$lib/types/environment";
import type { ArtifactEmbedMode } from "$lib/utils/artifactPrepareHtml";

/** How a canvas presentation should embed in Home for a given surface layout. */
export function presentationEmbedMode(
  surfaceLayout: SurfaceLayout | undefined,
  component: ComponentDef,
): ArtifactEmbedMode {
  if (surfaceLayout === "dashboard" && component.slot === "main") {
    return "panel";
  }
  if (component.presentation === "panel") return "panel";
  if (component.presentation === "fullscreen") return "fullscreen";
  return "inline";
}

export function presentationBare(
  surfaceLayout: SurfaceLayout | undefined,
  mode: ArtifactEmbedMode,
): boolean {
  if (surfaceLayout === "dashboard") return true;
  return mode !== "inline";
}

export function surfaceUsesDashboardFill(
  surfaceLayout: SurfaceLayout | undefined,
): boolean {
  return surfaceLayout === "dashboard";
}
