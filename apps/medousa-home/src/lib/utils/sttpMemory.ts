/** Display-only decoding of the STTP already delivered by the memory API. */
export interface MemoryField { name: string; value: unknown; confidence?: number }
export interface MemoryRelation { relation: string; target: string; confidence?: number }
export interface MemoryDocument {
  fields: MemoryField[];
  relations: MemoryRelation[];
  compressionDepth: number | null;
  parentNodeId: string | null;
}

export function decodeMemory(raw: string): MemoryDocument | null {
  if (!raw || raw.length > 512 * 1024) return null;
  try {
    const fields = readLayer(raw, "◈⟨");
    if (!fields) return null;
    const provenance = readLayer(raw, "⊕⟨") ?? [];
    const value = (name: string) => provenance.find((field) => field.name === name)?.value;
    const links = value("semantic_links");
    const relations: MemoryRelation[] = Array.isArray(links) ? links.flatMap((link: unknown) => {
      if (!link || typeof link !== "object") return [];
      const entry = link as Record<string, unknown>;
      if (typeof entry.rel !== "string" || typeof entry.target !== "string") return [];
      return [{ relation: entry.rel, target: entry.target, ...(typeof entry.confidence === "number" ? { confidence: entry.confidence } : {}) }];
    }) : [];
    const depth = value("compression_depth"), parent = value("parent_node");
    return { fields, relations, compressionDepth: typeof depth === "number" ? depth : null, parentNodeId: typeof parent === "string" ? parent : null };
  } catch { return null; }
}

function readLayer(raw: string, marker: string): MemoryField[] | null {
  let quoted = false, escaped = false;
  for (let i = 0; i < raw.length; i++) {
    const char = raw[i];
    if (quoted) {
      if (escaped) escaped = false;
      else if (char === "\\") escaped = true;
      else if (char === '"') quoted = false;
    } else if (char === '"') quoted = true;
    else if (raw.startsWith(marker, i)) {
      const start = raw.indexOf("{", i + marker.length);
      const end = raw.indexOf("⟩", i + marker.length);
      if (start < 0 || (end >= 0 && start > end)) return null;
      const reader = new ContentReader(raw, start);
      const fields = reader.object(0);
      reader.whitespace();
      if (!raw.startsWith("⟩", reader.position)) return null;
      return fields;
    }
  }
  return null;
}

/** Confidence-qualified keys need a structural reader, not regex rewrites of JSON. */
class ContentReader {
  remaining = 4096;
  constructor(readonly text: string, public position: number) {}
  whitespace() { while (/\s/.test(this.text[this.position] ?? "")) this.position++; }
  take(char: string) { this.whitespace(); if (this.text[this.position] !== char) return false; this.position++; return true; }
  string(): string {
    this.whitespace(); const start = this.position;
    if (this.text[this.position++] !== '"') throw new Error("Expected quoted string");
    while (this.position < this.text.length) {
      const char = this.text[this.position++];
      if (char === "\\") this.position++;
      else if (char === '"') return JSON.parse(this.text.slice(start, this.position)) as string;
    }
    throw new Error("Incomplete string");
  }
  object(depth: number): MemoryField[] {
    if (depth > 16 || !this.take("{")) throw new Error("Expected object");
    const fields: MemoryField[] = [];
    if (this.take("}")) return fields;
    while (this.position < this.text.length) {
      this.whitespace(); let key: string;
      if (this.text[this.position] === '"') key = this.string();
      else {
        const start = this.position;
        while (this.position < this.text.length && !':{},"'.includes(this.text[this.position])) this.position++;
        key = this.text.slice(start, this.position).trim();
      }
      if (!this.take(":")) throw new Error("Expected field separator");
      const signature = key.match(/^(.*?)\(\s*([.\d]+)\s*\)$/);
      const name = signature ? signature[1].trim() : key;
      const confidence = signature ? Number(signature[2]) : undefined;
      if (!name || (key.endsWith(")") && !signature) || (confidence !== undefined && (!Number.isFinite(confidence) || confidence < 0 || confidence > 1)) || fields.some((field) => field.name === name)) throw new Error("Invalid field");
      fields.push({ name, value: this.value(depth + 1), ...(confidence === undefined ? {} : { confidence }) });
      if (this.take("}")) return fields;
      if (!this.take(",")) throw new Error("Expected field separator");
      if (this.take("}")) return fields;
    }
    throw new Error("Incomplete object");
  }
  value(depth: number): unknown {
    if (depth > 16 || this.remaining-- <= 0) throw new Error("Content too deep");
    this.whitespace(); const char = this.text[this.position];
    if (char === '"') return this.string();
    if (char === "{") return Object.fromEntries(this.object(depth).map((field) => [field.name, field.value]));
    if (char === "[") {
      this.position++; const list: unknown[] = [];
      if (this.take("]")) return list;
      while (this.position < this.text.length) {
        list.push(this.value(depth + 1));
        if (this.take("]")) return list;
        if (!this.take(",")) throw new Error("Expected list separator");
        if (this.take("]")) return list;
      }
      throw new Error("Incomplete list");
    }
    const start = this.position;
    while (this.position < this.text.length && !",}]".includes(this.text[this.position])) this.position++;
    const atom = this.text.slice(start, this.position).trim();
    if (!atom || /[\[\{"\n\r]/.test(atom)) throw new Error("Unsupported value");
    try { return JSON.parse(atom) as unknown; } catch { return atom; }
  }
}
