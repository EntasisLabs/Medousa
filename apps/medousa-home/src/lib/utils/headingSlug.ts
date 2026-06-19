/** Obsidian-style heading slug for in-doc anchors and `[[note#Heading]]`. */
export function slugifyHeading(text: string): string {
  return text
    .trim()
    .toLowerCase()
    .replace(/[^\p{L}\p{N}\s-]/gu, "")
    .replace(/\s+/g, "-")
    .replace(/-+/g, "-")
    .replace(/^-|-$/g, "");
}

export function uniqueHeadingSlug(
  text: string,
  counts: Map<string, number>,
): string {
  const base = slugifyHeading(text) || "section";
  const seen = counts.get(base) ?? 0;
  counts.set(base, seen + 1);
  return seen === 0 ? base : `${base}-${seen}`;
}

export function headingSlugCandidates(rawHeading: string): string[] {
  const trimmed = rawHeading.trim();
  if (!trimmed) return [];
  const primary = slugifyHeading(trimmed);
  const candidates = [primary, slugifyHeading(decodeURIComponent(trimmed))].filter(
    Boolean,
  );
  return [...new Set(candidates)];
}

export function scrollToHeadingInContainer(
  container: HTMLElement,
  rawHeading: string,
): boolean {
  for (const slug of headingSlugCandidates(rawHeading)) {
    const byId = container.querySelector<HTMLElement>(`#${cssEscape(slug)}`);
    if (byId) {
      byId.scrollIntoView({ behavior: "smooth", block: "start" });
      byId.classList.add("markdown-heading-flash");
      window.setTimeout(() => byId.classList.remove("markdown-heading-flash"), 1200);
      return true;
    }
    const byData = container.querySelector<HTMLElement>(
      `[data-heading-slug="${cssEscapeAttr(slug)}"]`,
    );
    if (byData) {
      byData.scrollIntoView({ behavior: "smooth", block: "start" });
      byData.classList.add("markdown-heading-flash");
      window.setTimeout(() => byData.classList.remove("markdown-heading-flash"), 1200);
      return true;
    }
  }

  const targetSlug = slugifyHeading(rawHeading);
  const headings = container.querySelectorAll<HTMLElement>(".markdown-heading");
  for (const heading of headings) {
    const slug =
      heading.dataset.headingSlug ??
      heading.id ??
      slugifyHeading(heading.textContent ?? "");
    if (slug === targetSlug) {
      heading.scrollIntoView({ behavior: "smooth", block: "start" });
      heading.classList.add("markdown-heading-flash");
      window.setTimeout(() => heading.classList.remove("markdown-heading-flash"), 1200);
      return true;
    }
  }

  return false;
}

function cssEscape(value: string): string {
  if (typeof CSS !== "undefined" && "escape" in CSS) {
    return CSS.escape(value);
  }
  return value.replace(/[^a-zA-Z0-9_-]/g, "\\$&");
}

function cssEscapeAttr(value: string): string {
  return value.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
}

export interface MarkdownHeadingEntry {
  depth: number;
  text: string;
  slug: string;
}

/** Scan ATX headings for TOC generation (skips fenced code blocks). */
export function extractMarkdownHeadings(source: string): MarkdownHeadingEntry[] {
  const lines = source.replace(/\r\n/g, "\n").split("\n");
  const counts = new Map<string, number>();
  const headings: MarkdownHeadingEntry[] = [];
  let inFence = false;

  for (const line of lines) {
    const trimmedStart = line.trimStart();
    if (trimmedStart.startsWith("```")) {
      inFence = !inFence;
      continue;
    }
    if (inFence) continue;

    const match = /^(#{1,6})\s+(.+?)\s*$/.exec(line);
    if (!match) continue;

    const text = match[2].replace(/\s+#+\s*$/, "").trim();
    if (!text) continue;

    headings.push({
      depth: match[1].length,
      text,
      slug: uniqueHeadingSlug(text, counts),
    });
  }

  return headings;
}
