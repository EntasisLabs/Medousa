/**
 * Resolve a ready task-run URL into a Browser-openable address.
 * Co-located workshops open loopback directly; remote workshops use the
 * tokenized daemon preview proxy (no public bind of the app service).
 */

import { getDaemonUrl } from "$lib/daemon";
import {
  createProjectTaskRunPreview,
  type ProjectTaskRun,
} from "$lib/forge";
import { isCoLocatedWorkshop } from "$lib/utils/workshopLocality";

export function isHttpDaemonBase(base: string): boolean {
  return /^https?:\/\//i.test(base.trim());
}

export async function resolveTaskPreviewOpenUrl(
  workId: string,
  run: ProjectTaskRun,
): Promise<{ url: string; via: "direct" | "proxy" }> {
  const readyUrl = run.ready_url?.trim();
  if (!readyUrl) {
    throw new Error("This run is not ready for Browser preview yet");
  }
  if (isCoLocatedWorkshop()) {
    return { url: readyUrl, via: "direct" };
  }
  const retainedPath = run.preview_path?.trim();
  const preview = retainedPath
    ? { preview_path: retainedPath }
    : await createProjectTaskRunPreview(workId, run.run_id);
  const base = (await getDaemonUrl()).replace(/\/$/, "");
  if (!isHttpDaemonBase(base)) {
    throw new Error(
      "Browser preview needs an HTTP workshop URL. Open Connection settings or run Medousa on the workshop machine.",
    );
  }
  const path = preview.preview_path.startsWith("/")
    ? preview.preview_path
    : `/${preview.preview_path}`;
  return { url: `${base}${path}`, via: "proxy" };
}
