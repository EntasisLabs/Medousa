/** M7c.1 — vault note kind from frontmatter + path inference. */

export type VaultNoteKind =
  | "daily"
  | "project"
  | "ledger"
  | "board"
  | "inbox"
  | "bug"
  | "note";

const KNOWN_KINDS = new Set<VaultNoteKind>([
  "daily",
  "project",
  "ledger",
  "board",
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

/** Workshop/system tags first, then user tags; deduped. */
export function sortVaultTagsForDisplay(tags: string[]): string[] {
  const workshop = new Set(["medousa", "vault", "session"]);
  const ordered: string[] = [];
  for (const tag of tags) {
    if (workshop.has(tag) || tag.startsWith("profile:") || tag.startsWith("chat:")) {
      if (!ordered.includes(tag)) ordered.push(tag);
    }
  }
  for (const tag of tags) {
    if (!ordered.includes(tag)) ordered.push(tag);
  }
  return ordered;
}
