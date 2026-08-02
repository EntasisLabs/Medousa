import type { BrowserPageSnapshot } from "./types.js";

const MAX_PAGE_TEXT = 24_000;

export interface CapturedPageDom {
  title: string;
  url: string;
  selection: string;
  text: string;
}

export interface CaptureActivePageOptions {
  /** Ask for the current origin when the first script attempt lacks access. */
  requestHostPermission?: boolean;
}

/** This function is serialized by chrome.scripting.executeScript. Keep it self-contained. */
export function collectPageDom(): CapturedPageDom {
  const selection = window.getSelection()?.toString().trim() ?? "";
  const rawText = document.body?.innerText || document.body?.textContent || "";
  const text = rawText.replace(/[ \t]+\n/g, "\n").replace(/\n{3,}/g, "\n\n").trim();
  return {
    title: document.title.trim(),
    url: window.location.href,
    selection,
    text,
  };
}

export function boundPageText(value: string): string {
  return value.replace(/\s+$/g, "").slice(0, MAX_PAGE_TEXT);
}

async function requestCurrentOriginPermission(tab: chrome.tabs.Tab | undefined): Promise<boolean> {
  const rawUrl = tab?.url?.trim();
  if (!rawUrl || !chrome.permissions?.request) return false;
  let parsed: URL;
  try {
    parsed = new URL(rawUrl);
  } catch {
    return false;
  }
  if (parsed.protocol !== "http:" && parsed.protocol !== "https:") return false;
  const origin = `${parsed.origin}/*`;
  if (chrome.permissions.contains && await chrome.permissions.contains({ origins: [origin] })) {
    return true;
  }
  return chrome.permissions.request({ origins: [origin] });
}

async function executePageCapture(tabId: number): Promise<CapturedPageDom | null> {
  try {
    const [result] = await chrome.scripting.executeScript({
      target: { tabId },
      func: collectPageDom,
    });
    const captured = result?.result;
    return captured && typeof captured === "object" ? captured : null;
  } catch {
    return null;
  }
}

function snapshotFromCaptured(
  fallback: BrowserPageSnapshot,
  captured: CapturedPageDom | null,
): BrowserPageSnapshot {
  if (!captured) return fallback;
  return {
    ...fallback,
    title: typeof captured.title === "string" && captured.title.trim() ? captured.title.trim() : fallback.title,
    url: typeof captured.url === "string" ? captured.url : fallback.url,
    selection: typeof captured.selection === "string" ? captured.selection.trim() : "",
    text: typeof captured.text === "string" ? boundPageText(captured.text) : "",
  };
}

export async function captureActivePage(
  options: CaptureActivePageOptions = {},
): Promise<BrowserPageSnapshot> {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  const fallback = {
    tabId: tab?.id,
    windowId: tab?.windowId,
    title: tab?.title?.trim() ?? "Current page",
    url: tab?.url ?? "",
    selection: "",
    text: "",
  };
  if (tab?.id === undefined) return fallback;

  const captured = await executePageCapture(tab.id);
  if (captured) return snapshotFromCaptured(fallback, captured);
  if (options.requestHostPermission && await requestCurrentOriginPermission(tab)) {
    return snapshotFromCaptured(fallback, await executePageCapture(tab.id));
  }
  return fallback;
}
