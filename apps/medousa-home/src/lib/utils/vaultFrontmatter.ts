/** M7c.1 — vault note kind from frontmatter + path inference. */

export type VaultNoteKind =
  | "daily"
  | "project"
  | "ledger"
  | "board"
  | "slides"
  | "resume"
  | "inbox"
  | "bug"
  | "note";

const KNOWN_KINDS = new Set<VaultNoteKind>([
  "daily",
  "project",
  "ledger",
  "board",
  "slides",
  "resume",
  "inbox",
  "bug",
  "note",
]);

export function normalizeKind(value: string | null | undefined): VaultNoteKind {
  const raw = (value ?? "").trim().toLowerCase();
  switch (raw) {
    case "daily":
    case "journal":
      return "daily";
    case "project":
    case "projects":
      return "project";
    case "ledger":
    case "finance":
      return "ledger";
    case "board":
    case "boards":
      return "board";
    case "slides":
    case "deck":
    case "presentation":
      return "slides";
    case "resume":
    case "cv":
    case "curriculum":
      return "resume";
    case "inbox":
    case "capture":
      return "inbox";
    case "bug":
    case "bugs":
      return "bug";
    case "note":
    case "notes":
      return "note";
    default:
      return raw && KNOWN_KINDS.has(raw as VaultNoteKind)
        ? (raw as VaultNoteKind)
        : "note";
  }
}

export function resolveKindFromPath(path: string): VaultNoteKind {
  if (path.startsWith("journal/")) return "daily";
  if (path.startsWith("projects/")) return "project";
  if (path.startsWith("finance/")) return "ledger";
  if (path.startsWith("boards/")) return "board";
  if (path.startsWith("slides/") || path.startsWith("decks/")) return "slides";
  if (path.startsWith("resumes/") || path.startsWith("cv/")) return "resume";
  if (path.startsWith("inbox/")) return "inbox";
  if (path.startsWith("bugs/")) return "bug";
  return "note";
}

export function resolveKind(
  path: string,
  kind: string | null | undefined,
): VaultNoteKind {
  if (kind?.trim()) return normalizeKind(kind);
  return resolveKindFromPath(path);
}

export function kindForSpace(spaceId: string): VaultNoteKind {
  switch (spaceId) {
    case "journal":
      return "daily";
    case "projects":
      return "project";
    case "finance":
      return "ledger";
    case "boards":
      return "board";
    case "slides":
    case "decks":
      return "slides";
    case "resumes":
    case "cv":
      return "resume";
    case "inbox":
      return "inbox";
    case "bugs":
      return "bug";
    default:
      return "note";
  }
}

export function kindLabel(kind: VaultNoteKind): string {
  switch (kind) {
    case "daily":
      return "Daily";
    case "project":
      return "Project";
    case "ledger":
      return "Ledger";
    case "board":
      return "Board";
    case "slides":
      return "Slides";
    case "resume":
      return "Resume";
    case "inbox":
      return "Inbox";
    case "bug":
      return "Bug";
    case "note":
      return "Note";
  }
}

export function kindBadgeClass(kind: VaultNoteKind): string {
  switch (kind) {
    case "ledger":
      return "variant-soft-surface border border-success-500/30 text-success-200";
    case "daily":
      return "variant-soft-surface border border-primary-500/25 text-primary-200";
    default:
      return "variant-soft-surface text-surface-300";
  }
}

/** Serialize YAML frontmatter without leading/trailing blank YAML lines. */
export function serializeFrontmatter(yaml: string, body: string): string {
  const yamlBody = yaml.replace(/^\n+/, "").replace(/\n+$/, "");
  const trimmed = body.replace(/^\n+/, "");
  if (!yamlBody) {
    return trimmed;
  }
  return `---\n${yamlBody}\n---\n\n${trimmed}`;
}

export function wrapWithFrontmatter(kind: VaultNoteKind, body: string): string {
  return serializeFrontmatter(`kind: ${kind}`, body);
}

/** Known kinds for Live properties kind picker (display order). */
export const VAULT_KIND_OPTIONS: VaultNoteKind[] = [
  "note",
  "daily",
  "project",
  "ledger",
  "board",
  "slides",
  "resume",
  "inbox",
  "bug",
];

function readFrontmatterField(
  frontmatter: string | null,
  key: string,
): string | null {
  if (!frontmatter?.trim()) return null;
  const prefix = `${key}:`;
  for (const raw of frontmatter.split("\n")) {
    const trimmed = raw.trim();
    if (!trimmed.startsWith(prefix)) continue;
    return trimmed.slice(prefix.length).trim().replace(/^['"]|['"]$/g, "");
  }
  return null;
}

function upsertFrontmatterField(
  frontmatter: string | null,
  key: string,
  value: string,
): string {
  const lines = (frontmatter ?? "").split("\n").filter((line, i, arr) => {
    if (line.trim()) return true;
    // keep interior blanks only
    return i > 0 && i < arr.length - 1;
  });
  const prefix = `${key}:`;
  let replaced = false;
  const next = lines.map((line) => {
    if (line.trimStart().startsWith(prefix)) {
      replaced = true;
      return `${key}: ${value}`;
    }
    return line;
  });
  if (!replaced) {
    if (key === "title") next.unshift(`${key}: ${value}`);
    else if (key === "kind") {
      const titleIdx = next.findIndex((l) => l.trimStart().startsWith("title:"));
      if (titleIdx >= 0) next.splice(titleIdx + 1, 0, `${key}: ${value}`);
      else next.unshift(`${key}: ${value}`);
    } else {
      next.push(`${key}: ${value}`);
    }
  }
  return next.join("\n").replace(/^\n+/, "").replace(/\n+$/, "");
}

export function parseFrontmatterTitle(frontmatter: string | null): string {
  return readFrontmatterField(frontmatter, "title") ?? "";
}

export function parseFrontmatterKindValue(
  frontmatter: string | null,
): string {
  return readFrontmatterField(frontmatter, "kind") ?? "";
}

export function parseFrontmatterAuthor(frontmatter: string | null): string {
  return readFrontmatterField(frontmatter, "author") ?? "";
}

export function parseFrontmatterDate(frontmatter: string | null): string {
  return (
    readFrontmatterField(frontmatter, "date") ??
    readFrontmatterField(frontmatter, "updated") ??
    ""
  );
}

export function setFrontmatterAuthorYaml(
  frontmatter: string | null,
  author: string,
): string {
  const trimmed = author.trim();
  if (!trimmed) {
    if (!frontmatter) return "";
    return frontmatter
      .split("\n")
      .filter((line) => !line.trimStart().startsWith("author:"))
      .join("\n")
      .replace(/^\n+/, "")
      .replace(/\n+$/, "");
  }
  return upsertFrontmatterField(frontmatter, "author", trimmed);
}

export function setFrontmatterDateYaml(
  frontmatter: string | null,
  date: string,
): string {
  const trimmed = date.trim();
  if (!trimmed) {
    if (!frontmatter) return "";
    return frontmatter
      .split("\n")
      .filter((line) => !line.trimStart().startsWith("date:"))
      .join("\n")
      .replace(/^\n+/, "")
      .replace(/\n+$/, "");
  }
  return upsertFrontmatterField(frontmatter, "date", trimmed);
}

/** Update `title:` in YAML (creates frontmatter when missing). */
export function setFrontmatterTitleYaml(
  frontmatter: string | null,
  title: string,
): string {
  const trimmed = title.trim();
  if (!trimmed) {
    if (!frontmatter) return "";
    return frontmatter
      .split("\n")
      .filter((line) => !line.trimStart().startsWith("title:"))
      .join("\n")
      .replace(/^\n+/, "")
      .replace(/\n+$/, "");
  }
  return upsertFrontmatterField(frontmatter, "title", trimmed);
}

/** Update `kind:` in YAML (creates frontmatter when missing). */
export function setFrontmatterKindYaml(
  frontmatter: string | null,
  kind: VaultNoteKind,
): string {
  return upsertFrontmatterField(frontmatter, "kind", kind);
}

/** Replace `tags:` with an inline list; preserves other YAML keys. */
export function setFrontmatterTagsYaml(
  frontmatter: string | null,
  tags: string[],
): string {
  const cleaned = [...new Set(tags.map((t) => t.trim()).filter(Boolean))];
  const tagsLine =
    cleaned.length === 0
      ? null
      : `tags: [${cleaned.map((t) => (t.includes(" ") || t.includes(":") ? `"${t}"` : t)).join(", ")}]`;

  const lines = (frontmatter ?? "").split("\n");
  const out: string[] = [];
  let inTagsBlock = false;
  let wroteTags = false;
  for (const raw of lines) {
    const trimmed = raw.trim();
    if (!inTagsBlock && trimmed.startsWith("tags:")) {
      const inline = trimmed.slice("tags:".length).trim();
      if (!inline || (inline.startsWith("[") && inline.endsWith("]"))) {
        if (tagsLine) {
          out.push(tagsLine);
          wroteTags = true;
        }
        continue;
      }
      if (inline) {
        if (tagsLine) {
          out.push(tagsLine);
          wroteTags = true;
        }
        continue;
      }
      inTagsBlock = true;
      if (tagsLine) {
        out.push(tagsLine);
        wroteTags = true;
      }
      continue;
    }
    if (inTagsBlock) {
      if (trimmed.startsWith("-")) continue;
      inTagsBlock = false;
    }
    if (trimmed || out.length > 0) out.push(raw);
  }
  if (!wroteTags && tagsLine) out.push(tagsLine);
  return out.join("\n").replace(/^\n+/, "").replace(/\n+$/, "");
}

export function stripFrontmatter(body: string): { content: string; frontmatter: string | null } {
  const trimmed = body.trimStart();
  if (!trimmed.startsWith("---")) {
    return { content: body, frontmatter: null };
  }
  const rest = trimmed.slice(3);
  const end = rest.indexOf("\n---");
  if (end === -1) {
    return { content: body, frontmatter: null };
  }
  // Drop the newline(s) that follow the opening `---` so rewrite does not grow blanks.
  const frontmatter = rest.slice(0, end).replace(/^\n+/, "").replace(/\n+$/, "");
  const content = rest.slice(end + 4).replace(/^\n+/, "");
  return { content, frontmatter };
}

export function setFrontmatterKind(body: string, kind: VaultNoteKind): string {
  const { content, frontmatter } = stripFrontmatter(body);
  if (frontmatter == null) {
    return wrapWithFrontmatter(kind, content);
  }
  const lines = frontmatter.split("\n");
  let replaced = false;
  const nextLines = lines.map((line) => {
    if (line.trimStart().startsWith("kind:")) {
      replaced = true;
      return `kind: ${kind}`;
    }
    return line;
  });
  if (!replaced) {
    nextLines.unshift(`kind: ${kind}`);
  }
  return serializeFrontmatter(nextLines.join("\n"), content);
}

export function insertTextAtSection(
  body: string,
  sectionHeading: string,
  text: string,
): string {
  const needle = sectionHeading.trim();
  const idx = body.indexOf(needle);
  if (idx === -1) {
    const trimmed = body.replace(/\s+$/, "");
    return `${trimmed}\n\n${needle}\n\n${text}\n`;
  }
  const afterHeading = idx + needle.length;
  const rest = body.slice(afterHeading);
  const nextSection = rest.search(/\n## /);
  if (nextSection === -1) {
    const trimmed = body.replace(/\s+$/, "");
    if (trimmed.includes(text.trim())) return body;
    return `${trimmed}\n\n${text}\n`;
  }
  const insertAt = afterHeading + nextSection;
  const before = body.slice(0, insertAt).replace(/\s+$/, "");
  const after = body.slice(insertAt).replace(/^\n+/, "");
  if (before.includes(text.trim())) return body;
  return `${before}\n\n${text}\n\n${after}`;
}

/** Parse `tags:` from YAML frontmatter (inline list or block list). */
export function parseFrontmatterTags(frontmatter: string | null): string[] {
  if (!frontmatter?.trim()) return [];
  const tags: string[] = [];
  let inTagsBlock = false;
  for (const rawLine of frontmatter.split("\n")) {
    const line = rawLine.trimEnd();
    const trimmed = line.trim();
    if (!inTagsBlock && trimmed.startsWith("tags:")) {
      const inline = trimmed.slice("tags:".length).trim();
      if (inline.startsWith("[") && inline.endsWith("]")) {
        for (const token of inline.slice(1, -1).split(",")) {
          const value = token.trim().replace(/^['"]|['"]$/g, "");
          if (value) tags.push(value);
        }
        continue;
      }
      if (inline) {
        tags.push(inline.replace(/^['"]|['"]$/g, ""));
        continue;
      }
      inTagsBlock = true;
      continue;
    }
    if (inTagsBlock) {
      if (!trimmed.startsWith("-")) break;
      const value = trimmed.slice(1).trim().replace(/^['"]|['"]$/g, "");
      if (value) tags.push(value);
    }
  }
  return tags;
}

export function readBodyTags(body: string): string[] {
  const { frontmatter } = stripFrontmatter(body);
  return parseFrontmatterTags(frontmatter);
}

/** System/workshop tags that should stay quiet in the UI. */
export function isWorkshopVaultTag(tag: string): boolean {
  const t = tag.trim().toLowerCase();
  return (
    t === "medousa" ||
    t === "vault" ||
    t === "session" ||
    t.startsWith("profile:") ||
    t.startsWith("chat:")
  );
}

/** Human tags first, workshop/system last; deduped. */
export function sortVaultTagsForDisplay(tags: string[]): string[] {
  const human: string[] = [];
  const workshop: string[] = [];
  for (const tag of tags) {
    if (!tag.trim()) continue;
    if (isWorkshopVaultTag(tag)) {
      if (!workshop.includes(tag)) workshop.push(tag);
    } else if (!human.includes(tag)) {
      human.push(tag);
    }
  }
  return [...human, ...workshop];
}

/** Resting Live chips: up to `limit` human tags; rest (incl. workshop) behind +N. */
export function restingVaultTagChips(
  tags: string[],
  limit = 2,
): { visible: string[]; hiddenCount: number } {
  const ordered = sortVaultTagsForDisplay(tags);
  const human = ordered.filter((t) => !isWorkshopVaultTag(t));
  const workshop = ordered.filter((t) => isWorkshopVaultTag(t));
  const visible = human.slice(0, limit);
  const hiddenCount = human.length - visible.length + workshop.length;
  return { visible, hiddenCount: Math.max(0, hiddenCount) };
}
