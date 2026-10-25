import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import type { SpotlightHostRequest } from "$lib/spotlight/host";

/** Lightweight root listener; the shared catalog loads on the first request. */
export function bindSpotlightShellHost(): () => void {
  let disposed = false;
  let stop: (() => void) | undefined;
  let owner: import("$lib/spotlight/shellHost").SpotlightShellHost | undefined;
  let loading: Promise<typeof owner> | undefined;
  const seen = new Set<number>();
  void getCurrentWindow().listen<{ id: number; workshopId: string; request: SpotlightHostRequest }>("desktop-spotlight-request", async event => {
    const { id, workshopId, request } = event.payload;
    if (seen.has(id)) return;
    seen.add(id);
    if (seen.size > 512) seen.delete(seen.values().next().value!);
    let result: unknown = null, error: string | null = null;
    try {
      if (disposed) throw new Error("The main shell closed");
      loading ??= import("$lib/spotlight/shellHost").then(({ createSpotlightShellHost }) => {
        if (disposed) return undefined;
        owner = createSpotlightShellHost(); return owner;
      }).catch(cause => { loading = undefined; throw cause; });
      const service = await loading;
      if (!service || disposed) throw new Error("The main shell closed");
      result = await service.handle(workshopId, request);
    } catch (cause) { error = cause instanceof Error ? cause.message : String(cause); }
    await invoke("desktop_spotlight_reply", { id, result, error }).catch(() => {});
  }).then(unlisten => { if (disposed) unlisten(); else stop = unlisten; });
  return () => { disposed = true; stop?.(); owner?.dispose(); seen.clear(); };
}
