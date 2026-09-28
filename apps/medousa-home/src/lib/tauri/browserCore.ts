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
  convertFileSrc as tauriConvertFileSrc,
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

export async function invoke<T>(
  cmd: string,
  args?: InvokeArgs,
  options?: InvokeOptions,
): Promise<T> {
  if (!tauriInternalsPresent()) {
    return browserCommand(cmd) as T;
  }
  return tauriInvoke<T>(cmd, args, options);
}

export function convertFileSrc(filePath: string, protocol = "asset"): string {
  if (!tauriInternalsPresent()) return filePath;
  return tauriConvertFileSrc(filePath, protocol);
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
