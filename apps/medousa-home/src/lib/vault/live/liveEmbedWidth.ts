/** Shared width presets for compare / slides / matrix embeds. */

export type LiveEmbedWidth = "narrow" | "medium" | "wide" | "full";

export const LIVE_EMBED_WIDTHS: LiveEmbedWidth[] = [
  "narrow",
  "medium",
  "wide",
  "full",
];

export function isLiveEmbedWidth(value: string): value is LiveEmbedWidth {
  return (LIVE_EMBED_WIDTHS as string[]).includes(value);
}

/** Parse `width:` from fence body KV lines. Invalid → null (default). */
export function parseEmbedWidth(body: string): LiveEmbedWidth | null {
  for (const line of body.replace(/\r\n/g, "\n").split("\n")) {
    const match = line.match(/^\s*width\s*:\s*(\S+)\s*$/i);
    if (!match) continue;
    const raw = match[1].toLowerCase();
    if (isLiveEmbedWidth(raw)) return raw;
    return null;
  }
  return null;
}

/**
 * Upsert or remove `width:` in fence body. Default `wide` / omitted when full
 * is not required — we omit only when width is null (caller decides).
 */
export function setEmbedWidthInBody(
  body: string,
  width: LiveEmbedWidth | null,
): string {
  const lines = body.replace(/\r\n/g, "\n").split("\n");
  const out: string[] = [];
  let seen = false;
  for (const line of lines) {
    if (/^\s*width\s*:/i.test(line)) {
      if (width && !seen) {
        out.push(`width: ${width}`);
        seen = true;
      }
      continue;
    }
    out.push(line);
  }
  if (width && !seen) {
    // Insert after leading blank / title-like keys when possible.
    let insertAt = 0;
    while (insertAt < out.length && out[insertAt].trim() === "") insertAt += 1;
    out.splice(insertAt, 0, `width: ${width}`);
  }
  return out.join("\n").replace(/\n{3,}/g, "\n\n");
}

export function embedWidthClass(width: LiveEmbedWidth | null | undefined): string {
  const resolved = width && isLiveEmbedWidth(width) ? width : "wide";
  return `vault-live-embed-width vault-live-embed-width--${resolved}`;
}
