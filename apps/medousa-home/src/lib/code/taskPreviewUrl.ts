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
  outputUrl?: string,
): Promise<{ url: string; via: "direct" | "proxy" }> {
  const readyUrl = run.ready_url?.trim();
  let target: URL | null = null;
  if (outputUrl) {
    target = new URL(outputUrl);
    if (!["http:", "https:"].includes(target.protocol) || target.username || target.password) throw new Error("Use an HTTP or HTTPS output link.");
    const ready = readyUrl ? new URL(readyUrl) : null;
    const local = (host: string) => host === "localhost" || host === "[::1]" || host === "0.0.0.0" || /^127\./.test(host) || /^10\./.test(host) || /^192\.168\./.test(host) || /^172\.(1[6-9]|2\d|3[01])\./.test(host);
    const sameService = ready && target.port === ready.port && target.protocol === ready.protocol && (target.hostname === ready.hostname || local(target.hostname) && local(ready.hostname));
    if (!sameService) {
      if (!isCoLocatedWorkshop() && local(target.hostname)) throw new Error("This output link is a different workshop service. Open it from that service’s ready preview.");
      return { url: target.toString(), via: "direct" };
    }
  }
  if (!readyUrl) {
    throw new Error("This run is not ready for Browser preview yet");
  }
  if (isCoLocatedWorkshop()) {
    return { url: target?.toString() ?? readyUrl, via: "direct" };
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
  const url = new URL(`${base}${path}`);
  if (target) {
    url.pathname = `${url.pathname.replace(/\/$/, "")}/${target.pathname.replace(/^\//, "")}`;
    url.search = target.search; url.hash = target.hash;
  }
  return { url: url.toString(), via: "proxy" };
}
