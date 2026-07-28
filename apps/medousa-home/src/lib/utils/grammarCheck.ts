/**
 * LanguageTool-compatible grammar check for vault notes.
 *
 * Talks to a LanguageTool v2 HTTP endpoint (`/v2/check`). Default points at a
 * local server (`http://localhost:8081`) — the public hosted API requires an
 * API key, so Medousa ships local-first and the endpoint is configurable.
 * Privacy: only note text is sent, never vault paths.
 */

const SETTINGS_KEY = "medousa-vault-grammar";
const DEFAULT_ENDPOINT = "http://localhost:8081";
const REQUEST_TIMEOUT_MS = 12_000;
const MAX_TEXT_LENGTH = 20_000;

export interface GrammarSettings {
  enabled: boolean;
  endpoint: string;
  language: string;
}

export const DEFAULT_GRAMMAR_SETTINGS: GrammarSettings = {
  enabled: false,
  endpoint: DEFAULT_ENDPOINT,
  language: "auto",
};

export interface GrammarMatch {
  /** Character offset into the checked text. */
  offset: number;
  length: number;
  message: string;
  replacements: string[];
  ruleId: string;
}

interface LanguageToolMatch {
  offset?: number;
  length?: number;
  message?: string;
  replacements?: { value?: string }[];
  rule?: { id?: string };
}

export function readGrammarSettings(): GrammarSettings {
  if (typeof localStorage === "undefined") return { ...DEFAULT_GRAMMAR_SETTINGS };
  try {
    const raw = localStorage.getItem(SETTINGS_KEY);
    if (!raw) return { ...DEFAULT_GRAMMAR_SETTINGS };
    const parsed = JSON.parse(raw) as Partial<GrammarSettings>;
    return {
      enabled: Boolean(parsed.enabled),
      endpoint:
        typeof parsed.endpoint === "string" && parsed.endpoint.trim()
          ? parsed.endpoint.trim().replace(/\/+$/, "")
          : DEFAULT_ENDPOINT,
      language:
        typeof parsed.language === "string" && parsed.language.trim()
          ? parsed.language.trim()
          : "auto",
    };
  } catch {
    return { ...DEFAULT_GRAMMAR_SETTINGS };
  }
}

export function writeGrammarSettings(settings: GrammarSettings): void {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
  window.dispatchEvent(new CustomEvent("medousa-grammar-settings"));
}

/**
 * Strip fenced code blocks and frontmatter so grammar only checks prose.
 * Returns the cleaned text plus a map back to source offsets.
 */
export function extractProseForGrammar(markdown: string): {
  text: string;
  toSource: (offset: number) => number;
} {
  const lines = markdown.split("\n");
  const kept: { line: string; sourceStart: number }[] = [];
  let inFence = false;
  let sourceOffset = 0;
  let frontmatterDone = true;

  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i];
    const lineLen = line.length + 1;

    if (i === 0 && line.trim() === "---") {
      frontmatterDone = false;
      sourceOffset += lineLen;
      continue;
    }
    if (!frontmatterDone) {
      if (line.trim() === "---") frontmatterDone = true;
      sourceOffset += lineLen;
      continue;
    }

    if (/^```/.test(line.trim())) {
      inFence = !inFence;
      sourceOffset += lineLen;
      continue;
    }
    if (inFence) {
      sourceOffset += lineLen;
      continue;
    }

    kept.push({ line, sourceStart: sourceOffset });
    sourceOffset += lineLen;
  }

  const segments = kept.filter((entry) => entry.line.trim().length > 0);
  const text = segments.map((entry) => entry.line).join("\n");
  const offsets = segments.map((entry) => entry.sourceStart);
  const textLineStarts: number[] = [];
  let cursor = 0;
  for (const entry of segments) {
    textLineStarts.push(cursor);
    cursor += entry.line.length + 1;
  }

  function toSource(offset: number): number {
    let line = 0;
    for (let i = 0; i < textLineStarts.length; i += 1) {
      if (textLineStarts[i] <= offset) line = i;
      else break;
    }
    const lineStart = textLineStarts[line] ?? 0;
    return (offsets[line] ?? 0) + (offset - lineStart);
  }

  return { text, toSource };
}

export async function checkGrammar(
  markdown: string,
  settings: GrammarSettings,
  signal?: AbortSignal,
): Promise<GrammarMatch[]> {
  const { text, toSource } = extractProseForGrammar(markdown);
  const clipped = text.slice(0, MAX_TEXT_LENGTH);
  if (!clipped.trim()) return [];

  const params = new URLSearchParams({
    text: clipped,
    language: settings.language === "auto" ? "auto" : settings.language,
    enabledOnly: "false",
  });

  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), REQUEST_TIMEOUT_MS);
  const onAbort = () => controller.abort();
  signal?.addEventListener("abort", onAbort);

  try {
    const response = await fetch(`${settings.endpoint}/v2/check`, {
      method: "POST",
      headers: { "Content-Type": "application/x-www-form-urlencoded" },
      body: params.toString(),
      signal: controller.signal,
    });
    if (!response.ok) {
      throw new Error(`Grammar check failed (HTTP ${response.status})`);
    }
    const data = (await response.json()) as { matches?: LanguageToolMatch[] };
    const matches = data.matches ?? [];
    return matches
      .filter((m) => typeof m.offset === "number" && typeof m.length === "number")
      .map((m) => ({
        offset: toSource(m.offset ?? 0),
        length: m.length ?? 0,
        message: m.message ?? "Grammar suggestion",
        replacements: (m.replacements ?? [])
          .map((r) => r.value ?? "")
          .filter((v) => v.length > 0)
          .slice(0, 5),
        ruleId: m.rule?.id ?? "",
      }))
      .filter((m) => m.length > 0);
  } finally {
    clearTimeout(timeout);
    signal?.removeEventListener("abort", onAbort);
  }
}
