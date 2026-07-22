type HljsCore = typeof import("highlight.js/lib/core").default;
type LanguageModule = { default: Parameters<HljsCore["registerLanguage"]>[1] };

/** Fence tag aliases → highlight.js language id. */
const LANG_ALIASES: Record<string, string> = {
  rs: "rust",
  ts: "typescript",
  tsx: "typescript",
  js: "javascript",
  jsx: "javascript",
  py: "python",
  yml: "yaml",
  sh: "bash",
  zsh: "bash",
  shell: "bash",
  md: "markdown",
  toml: "ini",
  ini: "ini",
  docker: "dockerfile",
  proto: "protobuf",
  pb: "protobuf",
  gql: "graphql",
  ps1: "powershell",
  ps: "powershell",
  make: "makefile",
  patch: "diff",
  udiff: "diff",
  svelte: "xml",
  vue: "xml",
  svg: "xml",
  htm: "html",
  text: "plaintext",
  txt: "plaintext",
  env: "properties",
  dotenv: "properties",
  cs: "csharp",
  "c#": "csharp",
  csharp: "csharp",
  "c++": "cpp",
  cxx: "cpp",
  cc: "cpp",
  hpp: "cpp",
  h: "cpp",
  cpp: "cpp",
  c: "c",
  java: "java",
  php: "php",
  r: "r",
  scala: "scala",
};

const LANGUAGE_LOADERS: Record<string, () => Promise<LanguageModule>> = {
  bash: () => import("highlight.js/lib/languages/bash"),
  c: () => import("highlight.js/lib/languages/c"),
  cpp: () => import("highlight.js/lib/languages/cpp"),
  csharp: () => import("highlight.js/lib/languages/csharp"),
  css: () => import("highlight.js/lib/languages/css"),
  diff: () => import("highlight.js/lib/languages/diff"),
  dockerfile: () => import("highlight.js/lib/languages/dockerfile"),
  go: () => import("highlight.js/lib/languages/go"),
  graphql: () => import("highlight.js/lib/languages/graphql"),
  http: () => import("highlight.js/lib/languages/http"),
  ini: () => import("highlight.js/lib/languages/ini"),
  java: () => import("highlight.js/lib/languages/java"),
  javascript: () => import("highlight.js/lib/languages/javascript"),
  json: () => import("highlight.js/lib/languages/json"),
  kotlin: () => import("highlight.js/lib/languages/kotlin"),
  latex: () => import("highlight.js/lib/languages/latex"),
  makefile: () => import("highlight.js/lib/languages/makefile"),
  markdown: () => import("highlight.js/lib/languages/markdown"),
  nginx: () => import("highlight.js/lib/languages/nginx"),
  php: () => import("highlight.js/lib/languages/php"),
  plaintext: () => import("highlight.js/lib/languages/plaintext"),
  powershell: () => import("highlight.js/lib/languages/powershell"),
  properties: () => import("highlight.js/lib/languages/properties"),
  protobuf: () => import("highlight.js/lib/languages/protobuf"),
  python: () => import("highlight.js/lib/languages/python"),
  r: () => import("highlight.js/lib/languages/r"),
  ruby: () => import("highlight.js/lib/languages/ruby"),
  rust: () => import("highlight.js/lib/languages/rust"),
  scala: () => import("highlight.js/lib/languages/scala"),
  scss: () => import("highlight.js/lib/languages/scss"),
  shell: () => import("highlight.js/lib/languages/shell"),
  sql: () => import("highlight.js/lib/languages/sql"),
  swift: () => import("highlight.js/lib/languages/swift"),
  typescript: () => import("highlight.js/lib/languages/typescript"),
  wasm: () => import("highlight.js/lib/languages/wasm"),
  xml: () => import("highlight.js/lib/languages/xml"),
  yaml: () => import("highlight.js/lib/languages/yaml"),
};

let hljsInstance: HljsCore | null = null;

async function ensureHljs(): Promise<HljsCore> {
  if (hljsInstance) return hljsInstance;

  const hljs = (await import("highlight.js/lib/core")).default;
  const entries = Object.entries(LANGUAGE_LOADERS);
  const modules = await Promise.all(entries.map(([, loader]) => loader()));

  entries.forEach(([name], index) => {
    hljs.registerLanguage(name, modules[index].default);
  });

  hljs.registerLanguage("html", modules[entries.findIndex(([name]) => name === "xml")].default);

  hljsInstance = hljs;
  return hljsInstance;
}

/** Resolve a fence/language alias to a registered hljs id, or null if unknown. */
export function resolveHighlightLang(alias: string | null | undefined): string | null {
  const raw = (alias ?? "").trim().toLowerCase();
  if (!raw) return null;
  const resolved = LANG_ALIASES[raw] ?? raw;
  return resolved || null;
}

function langFromClassName(className: string): string | null {
  const match = className.match(/language-([\w-]+)/i);
  if (!match) return null;
  return resolveHighlightLang(match[1]);
}

function applyLanguageClass(code: HTMLElement, hljs: HljsCore, langHint?: string | null): boolean {
  const lang = resolveHighlightLang(langHint) ?? langFromClassName(code.className);
  if (!lang || !hljs.getLanguage(lang)) return false;

  code.classList.remove(...[...code.classList].filter((name) => name.startsWith("language-")));
  code.classList.add(`language-${lang}`);
  if (!code.classList.contains("syn-code") && !code.classList.contains("markdown-code")) {
    code.classList.add("syn-code");
  }
  return true;
}

/** Highlight a single `<code>` element in place (client only). */
export async function highlightElement(
  codeEl: HTMLElement,
  langHint?: string | null,
): Promise<void> {
  if (typeof window === "undefined") return;
  if (codeEl.dataset.hljs === "1") return;

  const hljs = await ensureHljs();
  if (!applyLanguageClass(codeEl, hljs, langHint)) {
    codeEl.dataset.hljs = "1";
    return;
  }
  try {
    hljs.highlightElement(codeEl);
  } catch {
    /* leave plain text */
  }
  codeEl.dataset.hljs = "1";
}

/**
 * Highlight source to HTML (escaped + spans). Safe for `{@html}` when `lang` is known.
 * Returns escaped plain text when language is unknown.
 */
export async function highlightToHtml(source: string, lang: string): Promise<string> {
  const resolved = resolveHighlightLang(lang) ?? "plaintext";
  const hljs = await ensureHljs();
  if (!hljs.getLanguage(resolved)) {
    return escapeHtml(source);
  }
  try {
    return hljs.highlight(source, { language: resolved, ignoreIllegals: true }).value;
  } catch {
    return escapeHtml(source);
  }
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

/** Apply syntax highlighting to Preview markdown fenced blocks (client only). */
export async function highlightCodeBlocks(root: HTMLElement): Promise<void> {
  if (typeof window === "undefined") return;

  const blocks = root.querySelectorAll<HTMLElement>(
    "pre.markdown-pre > code.markdown-code, pre > code.syn-code",
  );
  if (blocks.length === 0) return;

  const pending = [...blocks].filter((node) => node.dataset.hljs !== "1");
  if (pending.length === 0) return;

  await Promise.all(pending.map((code) => highlightElement(code)));
}

/** Languages registered for fenced blocks (for docs / debugging). */
export const MARKDOWN_HIGHLIGHT_LANGUAGES = [
  ...Object.keys(LANGUAGE_LOADERS),
  "html",
] as const;
