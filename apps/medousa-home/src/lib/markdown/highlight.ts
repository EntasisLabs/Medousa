type HljsCore = typeof import("highlight.js/lib/core").default;

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
};

let hljsInstance: HljsCore | null = null;

async function ensureHljs(): Promise<HljsCore> {
  if (hljsInstance) return hljsInstance;

  const hljs = (await import("highlight.js/lib/core")).default;
  const [
    bash,
    css,
    go,
    ini,
    javascript,
    json,
    markdown,
    python,
    rust,
    shell,
    sql,
    typescript,
    xml,
    yaml,
  ] = await Promise.all([
    import("highlight.js/lib/languages/bash"),
    import("highlight.js/lib/languages/css"),
    import("highlight.js/lib/languages/go"),
    import("highlight.js/lib/languages/ini"),
    import("highlight.js/lib/languages/javascript"),
    import("highlight.js/lib/languages/json"),
    import("highlight.js/lib/languages/markdown"),
    import("highlight.js/lib/languages/python"),
    import("highlight.js/lib/languages/rust"),
    import("highlight.js/lib/languages/shell"),
    import("highlight.js/lib/languages/sql"),
    import("highlight.js/lib/languages/typescript"),
    import("highlight.js/lib/languages/xml"),
    import("highlight.js/lib/languages/yaml"),
  ]);

  hljs.registerLanguage("bash", bash.default);
  hljs.registerLanguage("css", css.default);
  hljs.registerLanguage("go", go.default);
  hljs.registerLanguage("ini", ini.default);
  hljs.registerLanguage("javascript", javascript.default);
  hljs.registerLanguage("json", json.default);
  hljs.registerLanguage("markdown", markdown.default);
  hljs.registerLanguage("python", python.default);
  hljs.registerLanguage("rust", rust.default);
  hljs.registerLanguage("shell", shell.default);
  hljs.registerLanguage("sql", sql.default);
  hljs.registerLanguage("typescript", typescript.default);
  hljs.registerLanguage("html", xml.default);
  hljs.registerLanguage("xml", xml.default);
  hljs.registerLanguage("yaml", yaml.default);

  hljsInstance = hljs;
  return hljsInstance;
}

function resolveLanguage(className: string): string | null {
  const match = className.match(/language-([\w-]+)/i);
  if (!match) return null;
  const raw = match[1].toLowerCase();
  const canonical = LANG_ALIASES[raw] ?? raw;
  return canonical;
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
