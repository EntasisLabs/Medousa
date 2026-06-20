export function buildModuleOpCall(op: string): string {
  const normalized = op.trim();
  if (!normalized) return "";

  const lower = normalized.toLowerCase();
  if (lower.includes("duckduckgo") || lower.endsWith(".search")) {
    return `${normalized}(query: "your query here")`;
  }
  if (lower.startsWith("core.echo")) {
    return `${normalized}(message: "hello")`;
  }
  if (lower.startsWith("http.get")) {
    return `${normalized}(url: "https://example.com")`;
  }
  if (lower.startsWith("http.")) {
    return `${normalized}(url: "https://example.com")`;
  }
  if (lower.includes("to_md") || lower.includes("html.")) {
    return `${normalized}(html: "<p>Hello</p>")`;
  }
  return `${normalized}()`;
}

export function pickExampleForOp(examples: string[], op: string): string | null {
  const needle = op.trim().toLowerCase();
  if (!needle) return null;

  for (const example of examples) {
    const trimmed = example.trim();
    if (!trimmed || trimmed.includes("/") && !trimmed.includes("(")) {
      continue;
    }
    if (trimmed.toLowerCase().includes(needle)) {
      return trimmed;
    }
  }
  return null;
}

export function wrapGlyphBody(body: string): string {
  const lines = body
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
  const inner = lines.length > 0 ? lines.map((line) => `  ${line}`).join("\n") : "  core.echo(message: \"hello\")";
  return `glyph Main {\n${inner}\n}`;
}

export function prepareModuleInsert(currentBody: string, op: string, examples: string[] = []): string {
  const example = pickExampleForOp(examples, op);
  const call = example ?? buildModuleOpCall(op);

  if (!currentBody.trim()) {
    return wrapGlyphBody(call);
  }

  const indent = currentBody.includes("\n") ? "  " : "";
  const prefix = currentBody.endsWith("\n") ? "" : "\n";
  return `${prefix}${indent}${call}`;
}
