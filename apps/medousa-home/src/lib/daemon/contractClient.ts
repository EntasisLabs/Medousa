import { invoke } from "@tauri-apps/api/core";
import { isBrowserWorkshop } from "$lib/platform";
import { runBrowserGrapheme } from "$lib/wasm/browserDaemon";
import { browserPortalActive, portalOperation } from "$lib/wasm/browserPortal";
import { OPERATIONS, type OperationId } from "./generatedOps";
import { activeWorkshopId } from "$lib/utils/workshopLocality";

export { OPERATIONS, type OperationId } from "./generatedOps";

export async function daemonUnary<T>(
  id: OperationId,
  pathParams: Record<string, string> = {},
  body?: unknown,
  executionRuntimeId?: string | null,
  query?: Record<string, string>,
  workshopScopeId?: string,
): Promise<T> {
  if (isBrowserWorkshop() && workshopScopeId && workshopScopeId !== activeWorkshopId()) {
    throw new Error("The project's workshop is no longer active");
  }
  const operation = OPERATIONS[id];
  if (operation.streaming) {
    throw new Error(`use daemonStreamStart for ${id}`);
  }
  if (isBrowserWorkshop() && browserPortalActive()) {
    return portalOperation<T>(id, pathParams, body, query);
  }
  if (isBrowserWorkshop() && id === "grapheme.run.post") {
    const source =
      body && typeof body === "object" && "source" in body
        ? String((body as { source?: unknown }).source ?? "")
        : "";
    return runBrowserGrapheme(source) as Promise<T>;
  }
  if (isBrowserWorkshop()) {
    throw new Error(`${id} is not available in the browser workshop`);
  }
  return invoke<T>("daemon_unary", {
    operation: id,
    pathParams,
    body,
    executionRuntimeId: executionRuntimeId?.trim() || null,
    query,
    ...(workshopScopeId ? { workshopScopeId } : {}),
  });
}

export async function daemonStreamStart(
  id: OperationId,
  pathParams: Record<string, string> = {},
  query?: Record<string, string>,
  clientHandle?: string,
  executionRuntimeId?: string | null,
): Promise<string> {
  if (isBrowserWorkshop()) {
    throw new Error(`${id} is not available in the browser workshop`);
  }
  return invoke<string>("daemon_stream_start", {
    operation: id,
    pathParams,
    query,
    clientHandle,
    executionRuntimeId: executionRuntimeId?.trim() || null,
  });
}

export async function daemonStreamCancel(handle: string): Promise<void> {
  if (isBrowserWorkshop()) return;
  return invoke("daemon_stream_cancel", { handle });
}
