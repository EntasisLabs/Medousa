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
  if (lower === "shell.run" || (lower.startsWith("shell.") && lower.endsWith(".run"))) {
    return `${normalized}(command: "echo hello", network: false, timeout_ms: 5000)`;
  }
  if (lower === "shell.status" || (lower.startsWith("shell.") && lower.endsWith(".status"))) {
    return `${normalized}()`;
  }
  if (lower.startsWith("medousa.digest")) {
    return `${normalized}(text: "summarize this turn")`;
  }
  if (lower.startsWith("medousa.")) {
    return `${normalized}(payload: {})`;
  }
  return `${normalized}()`;
}

/** Qualify a short op name with its module id when missing. */
export function qualifyModuleOp(moduleId: string | null | undefined, op: string): string {
  const trimmed = op.trim();
  if (!trimmed) return "";
  if (trimmed.includes(".")) return trimmed;
  const module = moduleId?.trim();
  if (!module) return trimmed;
  return `${module}.${trimmed}`;
}

export function pickExampleForOp(examples: string[], op: string): string | null {
  const needle = op.trim().toLowerCase();
  if (!needle) return null;

  for (const example of examples) {
    const trimmed = example.trim();
    if (!trimmed || (trimmed.includes("/") && !trimmed.includes("("))) {
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
  const inner =
    lines.length > 0 ? lines.map((line) => `  ${line}`).join("\n") : '  core.echo(message: "hello")';
  return `glyph Main {\n${inner}\n}`;
}

export function prepareModuleInsert(
  currentBody: string,
  op: string,
  examples: string[] = [],
): string {
  const example = pickExampleForOp(examples, op);
  const call = example ?? buildModuleOpCall(op);

  if (!currentBody.trim()) {
    // Full example queries already wrap themselves; stub calls need a glyph shell.
    if (example && /\b(query|mutation|glyph)\b/i.test(example)) {
      return example.trim() + "\n";
    }
    return wrapGlyphBody(call);
  }

  // Prefer inserting just the call line when the editor already has a body.
  const callOnly = example
    ? (extractCallFromExample(example, op) ?? buildModuleOpCall(op))
    : call;
  const indent = currentBody.includes("\n") ? "  " : "";
  const prefix = currentBody.endsWith("\n") ? "" : "\n";
  return `${prefix}${indent}${callOnly}`;
}

function extractCallFromExample(example: string, op: string): string | null {
  const needle = op.trim().toLowerCase();
  const short = needle.includes(".") ? needle.split(".").pop()! : needle;
  for (const line of example.split("\n")) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith("//")) continue;
    const lower = trimmed.toLowerCase();
    if (lower.includes(`${needle}(`) || lower.includes(`${short}(`)) {
      return trimmed.replace(/\s*\{\s*$/, "").trim();
    }
  }
  return null;
}
