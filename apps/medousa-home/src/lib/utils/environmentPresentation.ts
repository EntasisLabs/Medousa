import type { ComponentDef, SurfaceLayout } from "$lib/types/environment";
import type { ArtifactEmbedMode } from "$lib/utils/artifactPrepareHtml";
import type { LayoutFillContext } from "$lib/utils/layoutPresentation";
import { shouldFillMainComponent } from "$lib/utils/layoutPresentation";

/** How a canvas presentation should embed in Home for a given surface layout. */
export function presentationEmbedMode(
  surfaceLayout: SurfaceLayout | undefined,
  component: ComponentDef,
  fillContext?: LayoutFillContext,
): ArtifactEmbedMode {
  const fills = fillContext ? shouldFillMainComponent(fillContext) : surfaceLayout === "dashboard" && component.slot === "main";
  if (fills) {
    return "panel";
  }
  if (component.presentation === "panel") return "panel";
  if (component.presentation === "fullscreen") return "fullscreen";
  return "inline";
}

export function presentationBare(
  surfaceLayout: SurfaceLayout | undefined,
  mode: ArtifactEmbedMode,
  fillContext?: LayoutFillContext,
): boolean {
  if (fillContext && shouldFillMainComponent(fillContext)) return true;
  if (surfaceLayout === "dashboard") return true;
  return mode !== "inline";
}

export function surfaceUsesDashboardFill(
  surfaceLayout: SurfaceLayout | undefined,
): boolean {
  return surfaceLayout === "dashboard";
}
