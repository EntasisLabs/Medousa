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
};

const LANGUAGE_LOADERS: Record<string, () => Promise<LanguageModule>> = {
  bash: () => import("highlight.js/lib/languages/bash"),
  css: () => import("highlight.js/lib/languages/css"),
  diff: () => import("highlight.js/lib/languages/diff"),
  dockerfile: () => import("highlight.js/lib/languages/dockerfile"),
  go: () => import("highlight.js/lib/languages/go"),
  graphql: () => import("highlight.js/lib/languages/graphql"),
  http: () => import("highlight.js/lib/languages/http"),
  ini: () => import("highlight.js/lib/languages/ini"),
  javascript: () => import("highlight.js/lib/languages/javascript"),
  json: () => import("highlight.js/lib/languages/json"),
  kotlin: () => import("highlight.js/lib/languages/kotlin"),
  latex: () => import("highlight.js/lib/languages/latex"),
  makefile: () => import("highlight.js/lib/languages/makefile"),
  markdown: () => import("highlight.js/lib/languages/markdown"),
  nginx: () => import("highlight.js/lib/languages/nginx"),
  plaintext: () => import("highlight.js/lib/languages/plaintext"),
  powershell: () => import("highlight.js/lib/languages/powershell"),
  properties: () => import("highlight.js/lib/languages/properties"),
  protobuf: () => import("highlight.js/lib/languages/protobuf"),
  python: () => import("highlight.js/lib/languages/python"),
  ruby: () => import("highlight.js/lib/languages/ruby"),
  rust: () => import("highlight.js/lib/languages/rust"),
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

function resolveLanguage(className: string): string | null {
  const match = className.match(/language-([\w-]+)/i);
  if (!match) return null;
  const raw = match[1].toLowerCase();
  return LANG_ALIASES[raw] ?? raw;
}

function applyLanguageClass(code: HTMLElement, hljs: HljsCore): boolean {
  const lang = resolveLanguage(code.className);
  if (!lang || !hljs.getLanguage(lang)) return false;

  code.classList.remove(...[...code.classList].filter((name) => name.startsWith("language-")));
  code.classList.add(`language-${lang}`);
  return true;
}

/** Apply calm syntax highlighting to fenced code blocks (client only). */
export async function highlightCodeBlocks(root: HTMLElement): Promise<void> {
  if (typeof window === "undefined") return;

  const blocks = root.querySelectorAll<HTMLElement>("pre.markdown-pre > code.markdown-code");
  if (blocks.length === 0) return;

  const pending = [...blocks].filter((node) => node.dataset.hljs !== "1");
  if (pending.length === 0) return;

  const hljs = await ensureHljs();

  for (const code of pending) {
    if (!applyLanguageClass(code, hljs)) {
      code.dataset.hljs = "1";
      continue;
    }
    try {
      hljs.highlightElement(code);
      code.dataset.hljs = "1";
    } catch {
      code.dataset.hljs = "1";
    }
  }
}

/** Languages registered for fenced blocks (for docs / debugging). */
export const MARKDOWN_HIGHLIGHT_LANGUAGES = [
  ...Object.keys(LANGUAGE_LOADERS),
  "html",
] as const;
