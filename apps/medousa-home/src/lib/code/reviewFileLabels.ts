export function basename(path: string): string {
  const parts = path.replaceAll("\\", "/").split("/");
  return parts[parts.length - 1] || path;
}

export function parentDir(path: string): string {
  const normalized = path.replaceAll("\\", "/");
  const idx = normalized.lastIndexOf("/");
  return idx > 0 ? normalized.slice(0, idx) : "";
}

export function shortKind(kind: string | null | undefined): string {
  const raw = (kind ?? "").toLowerCase();
  if (raw.includes("fn") || raw.includes("function") || raw.includes("method")) return "fn";
  if (raw.includes("struct") || raw.includes("class") || raw.includes("type")) return "type";
  if (raw.includes("trait") || raw.includes("interface")) return "trait";
  if (raw.includes("mod") || raw.includes("module")) return "mod";
  if (raw.includes("enum")) return "enum";
  if (raw.includes("const")) return "const";
  if (raw.length <= 6 && raw) return raw;
  return "sym";
}

export function symbolName(label: string): string {
  const trimmed = label.trim();
  if (!trimmed) return label;
  const parts = trimmed.split("::");
  return parts[parts.length - 1] || trimmed;
}
