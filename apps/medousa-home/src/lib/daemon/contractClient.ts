import { invoke } from "@tauri-apps/api/core";
import { OPERATIONS, type OperationId } from "./generatedOps";

export { OPERATIONS, type OperationId } from "./generatedOps";

export async function daemonUnary<T>(
  id: OperationId,
  pathParams: Record<string, string> = {},
  body?: unknown,
): Promise<T> {
  const operation = OPERATIONS[id];
  if (operation.streaming) {
    throw new Error(`use daemonStreamStart for ${id}`);
  }
  return invoke<T>("daemon_unary", { operation: id, pathParams, body });
}

export async function daemonStreamStart(
  id: OperationId,
  pathParams: Record<string, string> = {},
): Promise<string> {
  return invoke<string>("daemon_stream_start", { operation: id, pathParams });
}

export async function daemonStreamCancel(handle: string): Promise<void> {
  return invoke("daemon_stream_cancel", { handle });
}