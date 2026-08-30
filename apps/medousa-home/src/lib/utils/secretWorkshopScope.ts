import {
  PERSONAL_WORKSHOP_ID,
  activeWorkshop,
  type WorkshopRegistry,
} from "$lib/types/workshopRegistry";

export type SecretWorkshopScope = "embedded" | "local-transport" | "remote";

/**
 * Secret writes follow the selected workshop. Native mobile Personal uses the
 * native embedded credential port; portal workshops must never fall back to it.
 */
export function secretWorkshopScope(
  registry: WorkshopRegistry,
  nativeEmbeddedMobile: boolean,
): SecretWorkshopScope {
  const workshop = activeWorkshop(registry);
  if (
    nativeEmbeddedMobile &&
    workshop?.id === PERSONAL_WORKSHOP_ID &&
    workshop.kind === "local"
  ) {
    return "embedded";
  }
  return workshop?.kind === "local" ? "local-transport" : "remote";
}
