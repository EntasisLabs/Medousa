/** Canonical navigation intent for a location inside governed work. */

import { undertakings } from "$lib/stores/undertakings.svelte";
import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";

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

  const known = undertakings.items.find((item) => item.id === workId);
  await lmeWorkspace.openCodeWorkspace(workId, known?.title);
  undertakings.setSelection({
    path,
    line: intent.line && intent.line > 0 ? Math.floor(intent.line) : null,
    entityId: intent.entityId?.trim() || null,
  });
}
