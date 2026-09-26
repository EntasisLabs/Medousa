const NATIVE_LIVE_PREVIEW_KEY = "medousa-live-native-transport-preview-v1";

let memoryEnabled = false;

export function nativeLivePreviewEnabled(): boolean {
  if (typeof localStorage === "undefined") return memoryEnabled;
  return localStorage.getItem(NATIVE_LIVE_PREVIEW_KEY) === "1";
}

export function setNativeLivePreviewEnabled(enabled: boolean): void {
  memoryEnabled = enabled;
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(NATIVE_LIVE_PREVIEW_KEY, enabled ? "1" : "0");
}
