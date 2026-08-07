/**
 * File-icon resolution for the Code tree, using Material Icon Theme
 * associations (same family Cursor ships with) + vendored SVGs under
 * `/file-icons/*.svg`.
 */
import theme from "./materialIconTheme.json";

export type CodeFileIconRef = {
  /** Material icon id → `/file-icons/{id}.svg` */
  id: string;
  label: string;
};

const DEFAULT_ID = theme.file ?? "file";

/** When an extension isn’t listed, map through language ids (VS Code order). */
const EXTENSION_LANGUAGE_IDS: Record<string, string> = {
  rs: "rust",
  ts: "typescript",
  mts: "typescript",
  cts: "typescript",
  tsx: "typescriptreact",
  js: "javascript",
  mjs: "javascript",
  cjs: "javascript",
  jsx: "javascriptreact",
  py: "python",
  pyi: "python",
  go: "go",
  json: "json",
  jsonc: "jsonc",
  md: "markdown",
  mdx: "markdown",
  yml: "yaml",
  yaml: "yaml",
  toml: "toml",
  html: "html",
  htm: "html",
  css: "css",
  scss: "scss",
  less: "less",
  svg: "svg",
  xml: "xml",
  sh: "shellscript",
  bash: "shellscript",
  zsh: "shellscript",
  fish: "shellscript",
  ps1: "powershell",
  c: "c",
  h: "c",
  cpp: "cpp",
  cc: "cpp",
  cxx: "cpp",
  hpp: "cpp",
  cs: "csharp",
  java: "java",
  kt: "kotlin",
  kts: "kotlin",
  rb: "ruby",
  php: "php",
  swift: "swift",
  lua: "lua",
  sql: "sql",
  graphql: "graphql",
  gql: "graphql",
  vue: "vue",
  svelte: "svelte",
  dockerfile: "dockerfile",
};

function basename(path: string): string {
  const normalized = path.replace(/\\/g, "/");
  const slash = normalized.lastIndexOf("/");
  return slash >= 0 ? normalized.slice(slash + 1) : normalized;
}

/** Longest-first extension keys: `foo.d.ts` → `d.ts`, `ts`. */
function extensionCandidates(fileName: string): string[] {
  const lower = fileName.toLowerCase();
  if (!lower.includes(".")) return [];
  if (lower.startsWith(".") && lower.lastIndexOf(".") === 0) return [];
  const parts = lower.split(".");
  const out: string[] = [];
  for (let index = 1; index < parts.length; index += 1) {
    out.push(parts.slice(index).join("."));
  }
  return out;
}

function iconExists(id: string): boolean {
  // Associations may name clones that we didn’t vendor; fall back instead.
  // Runtime check would need a manifest — treat known default as always ok;
  // img onerror in the component covers the rest.
  return id.length > 0;
}

function resolveIconId(path: string): string {
  const name = basename(path);
  if (!name) return DEFAULT_ID;
  const lower = name.toLowerCase();

  const byName = (theme.fileNames as Record<string, string>)[lower];
  if (byName && iconExists(byName)) return byName;

  for (const ext of extensionCandidates(lower)) {
    const byExt = (theme.fileExtensions as Record<string, string>)[ext];
    if (byExt && iconExists(byExt)) return byExt;
  }

  for (const ext of extensionCandidates(lower)) {
    const languageId = EXTENSION_LANGUAGE_IDS[ext];
    if (!languageId) continue;
    const byLang = (theme.languageIds as Record<string, string>)[languageId];
    if (byLang && iconExists(byLang)) return byLang;
  }

  return DEFAULT_ID;
}

export function codeFileIconForPath(path: string): CodeFileIconRef {
  const id = resolveIconId(path);
  return { id, label: id };
}

export function codeFileIconSrc(id: string): string {
  return `/file-icons/${encodeURIComponent(id)}.svg`;
}
