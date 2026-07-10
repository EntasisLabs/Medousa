/**
 * Defense-in-depth for chat body markdown.
 *
 * Some providers / journal composers fold reasoning into the answer as
 * `> [!abstract] Reasoning` callouts. Chat already paints reasoning via the
 * thinking shell — strip those callouts from the prose body so they don't
 * double-render as a loud "REASONING" box.
 */

const REASONING_CALLOUT_START = /^>\s*\[!abstract\]\s*(.*)$/i;
const CALLOUT_CONT = /^>\s?(.*)$/;

export interface StripChatBodyResult {
  /** Body markdown with reasoning callouts removed. */
  markdown: string;
  /** Concatenated reasoning text recovered from stripped callouts (if any). */
  recoveredReasoning: string | null;
}

/**
 * Remove Obsidian-style `> [!abstract] …` callouts from assistant body markdown.
 * Optionally recover their body text so it can feed the thinking shell.
 */
export function stripChatBodyChrome(source: string): StripChatBodyResult {
  if (!source.includes("[!abstract]")) {
    return { markdown: source, recoveredReasoning: null };
  }

  const lines = source.replace(/\r\n/g, "\n").split("\n");
  const out: string[] = [];
  const recovered: string[] = [];
  let index = 0;

  while (index < lines.length) {
    const match = lines[index].match(REASONING_CALLOUT_START);
    if (!match) {
      out.push(lines[index]);
      index += 1;
      continue;
    }

    // Title line may carry trailing text after `[!abstract] Reasoning`
    const titleTail = match[1]?.trim() ?? "";
    // Drop the conventional "Reasoning" title; keep any other tail as content.
    if (titleTail && !/^reasoning$/i.test(titleTail)) {
      recovered.push(titleTail);
    }
    index += 1;

    while (index < lines.length) {
      const cont = lines[index].match(CALLOUT_CONT);
      if (!cont) break;
      const bodyLine = (cont[1] ?? "").trimEnd();
      if (bodyLine.trim()) recovered.push(bodyLine);
      index += 1;
    }

    // Avoid leaving a blank gap where the callout was (collapse one empty line).
    if (out.length > 0 && out[out.length - 1] !== "") {
      /* keep prior content */
    }
    while (index < lines.length && lines[index].trim() === "") {
      index += 1;
    }
  }

  const markdown = out.join("\n").replace(/^\n+/, "").replace(/\n{3,}/g, "\n\n").trimEnd();
  const recoveredReasoning =
    recovered.length > 0 ? recovered.join("\n").trim() || null : null;
  return { markdown, recoveredReasoning };
}
