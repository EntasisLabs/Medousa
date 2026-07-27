import {
  blockIdFromFragment,
  isBlockIdFragment,
} from "$lib/markdown/blockAnchors";

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

/** Nearest ancestor that actually scrolls (overflow auto/scroll with overflow). */
export function nearestScrollContainer(from: HTMLElement): HTMLElement {
  let node: HTMLElement | null = from;
  while (node) {
    const style = window.getComputedStyle(node);
    const overflowY = style.overflowY;
    if (
      (overflowY === "auto" || overflowY === "scroll" || overflowY === "overlay") &&
      node.scrollHeight > node.clientHeight + 1
    ) {
      return node;
    }
    node = node.parentElement;
  }
  return from;
}

/** Scroll a heading into view without dragging ancestor chrome (status bars, rails). */
export function scrollElementWithinContainer(
  container: HTMLElement,
  target: HTMLElement,
): void {
  const scroller = nearestScrollContainer(container);
  const scrollerRect = scroller.getBoundingClientRect();
  const targetRect = target.getBoundingClientRect();
  const nextTop =
    scroller.scrollTop + (targetRect.top - scrollerRect.top) - 12;
  scroller.scrollTo({
    top: Math.max(0, nextTop),
    behavior: "smooth",
  });
  target.classList.add("markdown-heading-flash");
  window.setTimeout(() => target.classList.remove("markdown-heading-flash"), 1200);
}

export function scrollToHeadingInContainer(
  container: HTMLElement,
  rawHeading: string,
): boolean {
  if (isBlockIdFragment(rawHeading)) {
    const blockId = blockIdFromFragment(rawHeading);
    if (blockId) {
      const byData = container.querySelector<HTMLElement>(
        `[data-block-id="${cssEscapeAttr(blockId)}"]`,
      );
      if (byData) {
        scrollElementWithinContainer(container, byData);
        return true;
      }
      const byId = container.querySelector<HTMLElement>(
        `#${cssEscape(`^${blockId}`)}`,
      );
      if (byId) {
        scrollElementWithinContainer(container, byId);
        return true;
      }
    }
    return false;
  }

  for (const slug of headingSlugCandidates(rawHeading)) {
    const byId = container.querySelector<HTMLElement>(`#${cssEscape(slug)}`);
    if (byId) {
      scrollElementWithinContainer(container, byId);
      return true;
    }
    const byData = container.querySelector<HTMLElement>(
      `[data-heading-slug="${cssEscapeAttr(slug)}"]`,
    );
    if (byData) {
      scrollElementWithinContainer(container, byData);
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
      scrollElementWithinContainer(container, heading);
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

const OUTLINE_HEADING_SELECTOR =
  "h1.markdown-heading[id], h2.markdown-heading[id], h3.markdown-heading[id]";

/** Heading id currently “in view” near the top of a scroll container. */
export function activeMarkdownHeadingId(
  scrollRoot: HTMLElement,
  contentRoot: HTMLElement = scrollRoot,
): string | null {
  const headings = [
    ...contentRoot.querySelectorAll<HTMLElement>(OUTLINE_HEADING_SELECTOR),
  ];
  if (headings.length === 0) return null;
  const rootTop = scrollRoot.getBoundingClientRect().top;
  const threshold = rootTop + 56;
  let active: string | null = headings[0]?.id ?? null;
  for (const heading of headings) {
    if (heading.getBoundingClientRect().top <= threshold) {
      active = heading.id || null;
    } else {
      break;
    }
  }
  return active;
}

/**
 * Keep an outline `activeId` in sync while `scrollRoot` scrolls.
 * Scroll-driven by default; debounced MutationObserver only rebinds targets
 * when heading nodes appear/disappear (preview hydrate / Live edits).
 */
export function observeActiveMarkdownHeading(
  scrollRoot: HTMLElement,
  onActive: (id: string | null) => void,
  contentRoot: HTMLElement = scrollRoot,
): () => void {
  let last: string | null | undefined;
  let observedSignature = "";
  let moTimer: ReturnType<typeof setTimeout> | null = null;

  const publish = () => {
    const next = activeMarkdownHeadingId(scrollRoot, contentRoot);
    if (next !== last) {
      last = next;
      onActive(next);
    }
  };

  const io = new IntersectionObserver(publish, {
    root: scrollRoot,
    rootMargin: "-10% 0px -55% 0px",
    threshold: [0, 0.25, 0.6, 1],
  });

  const headingSignature = () => {
    const headings = contentRoot.querySelectorAll(OUTLINE_HEADING_SELECTOR);
    let sig = `${headings.length}`;
    for (const heading of headings) {
      sig += `|${(heading as HTMLElement).id}`;
    }
    return sig;
  };

  const watchHeadings = () => {
    const nextSig = headingSignature();
    if (nextSig === observedSignature) {
      publish();
      return;
    }
    observedSignature = nextSig;
    io.disconnect();
    for (const heading of contentRoot.querySelectorAll(OUTLINE_HEADING_SELECTOR)) {
      io.observe(heading);
    }
    publish();
  };

  watchHeadings();
  scrollRoot.addEventListener("scroll", publish, { passive: true });
  const mo = new MutationObserver(() => {
    if (moTimer != null) clearTimeout(moTimer);
    moTimer = setTimeout(() => {
      moTimer = null;
      watchHeadings();
    }, 120);
  });
  mo.observe(contentRoot, { childList: true, subtree: true });

  return () => {
    scrollRoot.removeEventListener("scroll", publish);
    io.disconnect();
    mo.disconnect();
    if (moTimer != null) clearTimeout(moTimer);
  };
}
