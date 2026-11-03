/**
 * Browser stand-in for `@tauri-apps/api/core`.
 *
 * Vite aliases that specifier here. A plain page has no `__TAURI_INTERNALS__`,
 * and the real `invoke` throws `Cannot read properties of undefined (reading 'invoke')`.
 * Native commands resolve without calling into Tauri. Tauri webviews still use
 * the real IPC.
 */
import {
  invoke as tauriInvoke,
  Channel,
  PluginListener,
  Resource,
  SERIALIZE_TO_IPC_FN,
  addPluginListener,
  checkPermissions,
  isTauri as tauriIsTauri,
  requestPermissions,
  transformCallback,
} from "../../../node_modules/@tauri-apps/api/core.js";

type InvokeArgs = Record<string, unknown> | number[] | ArrayBuffer | Uint8Array;
interface InvokeOptions {
  headers: HeadersInit;
}

function tauriInternalsPresent(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function browserCommand(cmd: string): unknown {
  switch (cmd) {
    case "daemon_url":
      return "";
    case "daemon_health":
      return { ok: false, message: "Personal workshop health is read from the page" };
    case "vault_list_notes":
      return { notes: [] };
    case "vault_list_roots":
      return {
        activeRootId: "personal",
        roots: [
          {
            id: "personal",
            label: "Personal",
            path: "",
            isDefault: true,
            active: true,
          },
        ],
      };
    case "vault_list_changes":
      return { vault_generation: 0, changes: [], reset_required: false };
    case "vault_list_tags":
      return { tags: [], count: 0 };
    default:
      return null;
  }
}

let nodeDispatch:
  | (import("$lib/appChat/client").AppChatBackend & { dispose: () => void })
  | null
  | undefined;

async function ensureNodeDispatch() {
  if (nodeDispatch !== undefined) return nodeDispatch;
  if (typeof process === "undefined" || !(process as { versions?: { node?: string } }).versions?.node) {
    nodeDispatch = null;
    return null;
  }
  const { createNodeDispatchBackend } = await import("$lib/appChat/nodeDispatchBackend");
  nodeDispatch = createNodeDispatchBackend();
  return nodeDispatch;
}

export async function invoke<T>(
  cmd: string,
  args?: InvokeArgs,
  options?: InvokeOptions,
): Promise<T> {
  if (tauriInternalsPresent()) {
    return tauriInvoke<T>(cmd, args, options);
  }
  if (
    cmd === "app_chat_dispatch" ||
    cmd === "app_chat_open_session" ||
    cmd === "app_chat_snapshot"
  ) {
    const backend = await ensureNodeDispatch();
    if (!backend) {
      throw new Error("Application chat core requires the Tauri host or Node dispatch binary");
    }
    const command =
      cmd === "app_chat_dispatch"
        ? (args as { command: import("$lib/types/generated/app_api").ChatCommand }).command
        : cmd === "app_chat_open_session"
          ? {
              type: "open_session" as const,
              session_id: String((args as { sessionId?: string }).sessionId ?? ""),
              authority_id: String((args as { authorityId?: string }).authorityId ?? ""),
            }
          : {
              type: "snapshot" as const,
              session_id: String((args as { sessionId?: string }).sessionId ?? ""),
            };
    return (await backend(command)) as T;
  }
  return browserCommand(cmd) as T;
}

export {
  Channel,
  PluginListener,
  Resource,
  SERIALIZE_TO_IPC_FN,
  addPluginListener,
  checkPermissions,
  tauriIsTauri as isTauri,
  requestPermissions,
  transformCallback,
};
