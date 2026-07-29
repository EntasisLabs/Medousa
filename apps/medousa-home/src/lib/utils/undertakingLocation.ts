/** Canonical navigation intent for a location inside governed work. */

import { undertakings } from "$lib/stores/undertakings.svelte";
import { shellTabs } from "$lib/stores/shellTabs.svelte";

export type UndertakingLocationIntent = {
  workId: string;
  path: string;
  line?: number | null;
  entityId?: string | null;
};

export async function openUndertakingLocation(
  intent: UndertakingLocationIntent,
): Promise<void> {
  const workId = intent.workId.trim();
  const rawPath = intent.path.trim().replaceAll("\\", "/");
  if (
    !workId ||
    !rawPath ||
    rawPath.startsWith("/") ||
    /^[a-z]:\//i.test(rawPath) ||
    rawPath.split("/").includes("..")
  ) {
    throw new Error("Invalid undertaking location");
  }
  const path = rawPath.replace(/^\.\//, "");

  undertakings.setWorkTab("undertakings");
  shellTabs.openSurface("work", { activate: true });
  await undertakings.select(workId);
  undertakings.setSelection({
    path,
    line: intent.line && intent.line > 0 ? Math.floor(intent.line) : null,
    entityId: intent.entityId?.trim() || null,
  });
}
