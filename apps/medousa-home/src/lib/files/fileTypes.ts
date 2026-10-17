export type FileViewKind = "markdown" | "text" | "image" | "pdf" | "spreadsheet" | "unsupported";

const languages: Record<string, string> = {
  md: "markdown", markdown: "markdown", mdx: "markdown", txt: "plaintext", log: "plaintext",
  rs: "rust", js: "javascript", mjs: "javascript", cjs: "javascript", jsx: "jsx", ts: "typescript", tsx: "tsx",
  json: "json", jsonc: "json", yaml: "yaml", yml: "yaml", toml: "plaintext", ini: "plaintext", conf: "plaintext", sql: "plaintext", env: "plaintext", properties: "plaintext", diff: "plaintext", patch: "plaintext",
  html: "html", htm: "html", css: "css", xml: "xml", svelte: "svelte", py: "python", go: "go",
  c: "c", h: "c", cc: "cpp", cpp: "cpp", hpp: "cpp", cs: "csharp", java: "java", kt: "kotlin",
  rb: "ruby", php: "php", swift: "swift", lua: "lua", sh: "shell", bash: "shell", zsh: "shell", grapheme: "grapheme",
};
const imageMime: Record<string, string> = {
  png: "image/png", jpg: "image/jpeg", jpeg: "image/jpeg", gif: "image/gif", webp: "image/webp",
  avif: "image/avif", bmp: "image/bmp", svg: "image/svg+xml", ico: "image/x-icon",
  heic: "image/heic", heif: "image/heif", tif: "image/tiff", tiff: "image/tiff",
};
export function fileExtension(path: string): string { return path.split(/[/\\]/).pop()?.split(".").pop()?.toLowerCase() ?? ""; }
export function fileLanguage(path: string): string {
  const name = path.split(/[/\\]/).pop()?.toLowerCase() ?? "";
  if (["dockerfile", "makefile", "license", "readme", ".gitignore", ".env", ".editorconfig"].includes(name)) return "plaintext";
  return languages[fileExtension(path)] ?? "plaintext";
}
export function fileViewKind(path: string): FileViewKind {
  const ext = fileExtension(path);
  if (imageMime[ext]) return "image";
  if (ext === "pdf") return "pdf";
  if (["csv", "tsv", "xlsx", "xls", "xlsm"].includes(ext)) return "spreadsheet";
  if (languages[ext] === "markdown") return "markdown";
  const name = path.split(/[/\\]/).pop() ?? "";
  if (languages[ext] || !name.includes(".") || [".gitignore", ".env", ".editorconfig"].includes(name)) return "text";
  return "unsupported";
}
export function filePreviewMime(path: string): string { return imageMime[fileExtension(path)] ?? (fileExtension(path) === "pdf" ? "application/pdf" : "application/octet-stream"); }
