/** BrowserHost search API for omnibox (desktop invoke; HTTP fallback in dev). */

import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "$lib/platform";
import { browserHostBaseUrl } from "$lib/browserBridge";

export interface BrowserSearchHit {
  title: string;
  url: string;
  snippet: string;
}

export interface BrowserSearchResponse {
  query: string;
  provider: string;
  results: BrowserSearchHit[];
  cached: boolean;
  challenge?: string | null;
}

export async function browserHostSearch(
  query: string,
  maxResults = 5,
): Promise<BrowserSearchResponse> {
  if (isTauri()) {
    try {
      return await invoke<BrowserSearchResponse>("browser_host_search", {
        query,
        maxResults,
      });
    } catch {
      // fall through to HTTP
    }
  }

  const base = await browserHostBaseUrl();
  if (base) {
    const response = await fetch(`${base.replace(/\/$/, "")}/v1/search`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ query, max_results: maxResults }),
    });
    if (response.ok) {
      return response.json() as Promise<BrowserSearchResponse>;
    }
  }

  throw new Error("BrowserHost search unavailable");
}
