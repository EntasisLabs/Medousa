/** Shared presentation for fenced Markdown and Liquid code snippets. */
export const CODE_PREVIEW_LINES = 12;

const LANGUAGE_LABELS: Record<string, string> = {
  sh: "Shell", shell: "Shell", bash: "Bash", zsh: "Zsh",
  ts: "TypeScript", tsx: "TypeScript", typescript: "TypeScript",
  js: "JavaScript", jsx: "JavaScript", javascript: "JavaScript",
  py: "Python", python: "Python", rs: "Rust", rust: "Rust",
  json: "JSON", yaml: "YAML", yml: "YAML", html: "HTML", xml: "XML",
  css: "CSS", sql: "SQL", md: "Markdown", markdown: "Markdown",
  c: "C", cpp: "C++", "c++": "C++", cs: "C#", csharp: "C#",
  go: "Go", swift: "Swift", diff: "Diff", toml: "TOML",
  text: "Code", txt: "Code", plaintext: "Code",
};

export function codeLanguageLabel(language: string): string {
  const raw = language.trim();
  return LANGUAGE_LABELS[raw.toLowerCase()] ?? (raw || "Code");
}

export function codeLineCount(source: string): number {
  return source ? source.replace(/\r?\n$/, "").split("\n").length : 0;
}

export function isLongCode(source: string): boolean {
  return codeLineCount(source) > CODE_PREVIEW_LINES;
}

export function canWrapCode(source: string): boolean {
  return isLongCode(source) || /[^\n]{81}/.test(source);
}

const icon = (paths: string) => `<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${paths}</svg>`;
const COPY_ICON = icon('<rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/>');
const CHECK_ICON = icon('<path d="m9 12 2 2 4-4"/>');

export type CodeCopyState = "idle" | "copied" | "failed";

export function codeCopyContent(state: CodeCopyState = "idle"): string {
  const label = state === "copied" ? "Copied" : state === "failed" ? "Failed" : "Copy";
  return `${state === "copied" ? CHECK_ICON : COPY_ICON}<span aria-live="polite">${label}</span>`;
}

export async function copyCodeText(text: string): Promise<boolean> {
  if (typeof navigator === "undefined" || !navigator.clipboard?.writeText) return false;
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}
