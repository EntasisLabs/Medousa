/** Wave 2/3 — parse markdown kanban card text into liquid-friendly presentation. */

export interface KanbanCardWikilink {
  /** Path / target passed to openWikilink (left of `|`). */
  target: string;
  /** Human label for chips / titles (right of `|`, else basename). */
  label: string;
}

export interface KanbanCardPresentation {
  /** Leading emoji if present. */
  emoji: string | null;
  /** First line / primary label (wikilink target preferred when card is link-primary). */
  title: string;
  /** Remaining lines after the title (may be empty). */
  body: string;
  wikilinks: KanbanCardWikilink[];
  tags: string[];
  /** Local or remote image href when referenced in the card. */
  imageHref: string | null;
  /** True when the card is essentially one wikilink (± tags/emoji). */
  wikilinkPrimary: boolean;
  /** Whether there is expandable depth beyond the title line. */
  hasDepth: boolean;
}

const WIKILINK_RE = /\[\[([^\]]+)\]\]/g;
const TAG_RE = /(^|[\s([{])#([a-zA-Z][\w/-]*)/g;
const MD_IMAGE_RE = /!\[[^\]]*]\(([^)\s]+)(?:\s+"[^"]*")?\)/;
const BARE_IMAGE_RE =
  /(?:^|\s)((?:\/|[A-Za-z]:\\|\.\/|\.\.\/)?[^\s]+\.(?:png|jpe?g|gif|webp|avif|svg))(?:\s|$)/i;
const EMOJI_RE =
  /^(\p{Extended_Pictographic}(?:\uFE0F|\u200D\p{Extended_Pictographic})*)\s+/u;

export function parseWikilinkInner(inner: string): KanbanCardWikilink {
  const trimmed = inner.trim();
  const pipe = trimmed.indexOf("|");
  if (pipe === -1) {
    const target = trimmed;
    const base = target.split(/[/\\]/).pop() ?? target;
    return { target, label: base || target || "Note" };
  }
  const target = trimmed.slice(0, pipe).trim();
  const label = trimmed.slice(pipe + 1).trim() || target;
  return { target, label: label || "Note" };
}

export function extractWikilinks(text: string): KanbanCardWikilink[] {
  const out: KanbanCardWikilink[] = [];
  const seen = new Set<string>();
  const re = new RegExp(WIKILINK_RE.source, "g");
  let match: RegExpExecArray | null;
  while ((match = re.exec(text)) !== null) {
    const link = parseWikilinkInner(match[1]);
    if (!link.target || seen.has(link.target)) continue;
    seen.add(link.target);
    out.push(link);
  }
  return out;
}

export function extractTags(text: string): string[] {
  const out: string[] = [];
  const re = new RegExp(TAG_RE.source, "g");
  let match: RegExpExecArray | null;
  while ((match = re.exec(text)) !== null) {
    const tag = match[2];
    if (tag && !out.includes(tag)) out.push(tag);
  }
  return out;
}

export function extractImageHref(text: string): string | null {
  const md = text.match(MD_IMAGE_RE);
  if (md?.[1]) return md[1].trim();
  const bare = text.match(BARE_IMAGE_RE);
  if (bare?.[1]) return bare[1].trim();
  return null;
}

function stripDecorators(line: string): string {
  return line
    .replace(MD_IMAGE_RE, " ")
    .replace(BARE_IMAGE_RE, " ")
    .replace(TAG_RE, "$1")
    .replace(/\s+/g, " ")
    .trim();
}

function displayTitleFromLine(line: string, wikilinks: KanbanCardWikilink[]): string {
  const stripped = stripDecorators(line);
  if (!stripped && wikilinks.length === 1) return wikilinks[0].label;
  // Prefer human label from `[[path|Alias]]`.
  const withLabels = stripped.replace(/\[\[([^\]]+)\]\]/g, (_, inner: string) =>
    parseWikilinkInner(inner).label,
  );
  return withLabels.trim() || "Untitled card";
}

/**
 * Split card markdown into title / body.
 * Prefers first line as title; also accepts `Title — detail` / `Title - detail` on one line.
 */
export function parseKanbanCardText(raw: string): KanbanCardPresentation {
  const text = raw.replace(/\r\n/g, "\n").trim();
  if (!text) {
    return {
      emoji: null,
      title: "Untitled card",
      body: "",
      wikilinks: [],
      tags: [],
      imageHref: null,
      wikilinkPrimary: false,
      hasDepth: false,
    };
  }

  let working = text;
  let emoji: string | null = null;
  const emojiMatch = working.match(EMOJI_RE);
  if (emojiMatch) {
    emoji = emojiMatch[1];
    working = working.slice(emojiMatch[0].length);
  }

  const wikilinks = extractWikilinks(text);
  const tags = extractTags(text);
  const imageHref = extractImageHref(text);

  const lines = working.split("\n").map((line) => line.trimEnd());
  const nonEmpty = lines.filter((line) => line.trim().length > 0);
  let titleLine = nonEmpty[0] ?? "";
  let bodyLines = nonEmpty.slice(1);

  const dashSplit = titleLine.match(/^(.+?)\s+[—–-]\s+(.+)$/);
  if (dashSplit && bodyLines.length === 0) {
    titleLine = dashSplit[1].trim();
    bodyLines = [dashSplit[2].trim()];
  }

  const title = displayTitleFromLine(titleLine, wikilinks);
  const body = bodyLines
    .join("\n")
    .replace(/\[\[([^\]]+)\]\]/g, (_, inner: string) => parseWikilinkInner(inner).label)
    .trim();

  const withoutEmoji = text.replace(EMOJI_RE, "").trim();
  const onlyLink =
    wikilinks.length === 1 &&
    withoutEmoji
      .replace(WIKILINK_RE, "")
      .replace(TAG_RE, "$1")
      .replace(MD_IMAGE_RE, "")
      .replace(BARE_IMAGE_RE, "")
      .trim() === "";

  return {
    emoji,
    title,
    body,
    wikilinks,
    tags,
    imageHref,
    wikilinkPrimary: onlyLink,
    hasDepth: Boolean(body) || wikilinks.length > 0 || tags.length > 0 || Boolean(imageHref),
  };
}
