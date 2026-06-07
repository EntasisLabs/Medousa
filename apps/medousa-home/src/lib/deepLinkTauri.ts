import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/** Thin bindings matching @tauri-apps/plugin-deep-link (avoids extra npm dep). */

export async function getCurrentDeepLinks(): Promise<string[] | null> {
  return invoke<string[] | null>("plugin:deep-link|get_current");
}

export async function onDeepLinkOpen(
  handler: (urls: string[]) => void,
): Promise<() => void> {
  const unlisten = await listen<string[]>("deep-link://new-url", (event) => {
    handler(event.payload);
  });
  return unlisten;
}
