import { browserHostSearch } from "$lib/utils/browserHostApi";
import {
  duckDuckGoSearchUrl,
  looksLikeBrowserUrl,
  normalizeBrowserUrlInput,
} from "$lib/utils/resolveBrowserInput";

/** Resolve omnibox text to a navigable URL (URL normalization or search). */
export async function resolveBrowserDestination(input: string): Promise<string> {
  const trimmed = input.trim();
  if (!trimmed) {
    throw new Error("Empty input");
  }

  if (looksLikeBrowserUrl(trimmed)) {
    return normalizeBrowserUrlInput(trimmed);
  }

  try {
    const response = await browserHostSearch(trimmed, 1);
    const first = response.results[0]?.url?.trim();
    if (first) return first;
  } catch {
    // BrowserHost unavailable (iOS / offline) — fall back to DDG results page
  }

  return duckDuckGoSearchUrl(trimmed);
}
