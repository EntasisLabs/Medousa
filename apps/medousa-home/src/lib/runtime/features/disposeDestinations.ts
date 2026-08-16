import type { DisposeReason, FeatureId } from "./types";
import { FEATURE_CATALOG } from "./catalog";
import { disposeFeature } from "./loader";

export const DESTINATION_FEATURE_IDS: FeatureId[] = FEATURE_CATALOG.map(
  (entry) => entry.id,
).filter((id) => id !== "shell-desktop" && id !== "shell-mobile");

export async function disposeDestinationFeatures(
  reason: DisposeReason,
): Promise<void> {
  await Promise.all(
    DESTINATION_FEATURE_IDS.map((id) => disposeFeature(id, reason)),
  );
}
