import { DEFAULT_GUIDE_CHAPTER_ID, getGuideChapter } from "./catalog";
import type { GuideHandoff } from "./types";
import { hideGuide, isTauri, showGuide } from "$lib/window";

export const GUIDE_CHAPTER_KEY = "medousa.guide.chapter";
export const GUIDE_ANCHOR_KEY = "medousa.guide.anchor";
export const GUIDE_HANDOFF_EVENT = "medousa-guide-handoff";

export function writeGuideHandoff(chapterId?: string | null, anchor?: string | null): void {
  if (typeof localStorage === "undefined") return;
  const chapter = getGuideChapter(chapterId) ?? getGuideChapter(DEFAULT_GUIDE_CHAPTER_ID);
  if (!chapter) return;
  try {
    localStorage.setItem(GUIDE_CHAPTER_KEY, chapter.id);
    if (anchor?.trim()) {
      localStorage.setItem(GUIDE_ANCHOR_KEY, anchor.trim().replace(/^#/, ""));
    } else {
      localStorage.removeItem(GUIDE_ANCHOR_KEY);
    }
  } catch {
    /* ignore quota / private mode */
  }

  if (typeof window !== "undefined") {
    window.dispatchEvent(
      new CustomEvent(GUIDE_HANDOFF_EVENT, {
        detail: { chapterId: chapter.id, anchor: anchor?.trim() || null } satisfies GuideHandoff,
      }),
    );
  }
}

export function readGuideHandoff(): GuideHandoff {
  if (typeof localStorage === "undefined") {
    return { chapterId: DEFAULT_GUIDE_CHAPTER_ID, anchor: null };
  }
  try {
    const chapterId =
      getGuideChapter(localStorage.getItem(GUIDE_CHAPTER_KEY))?.id ?? DEFAULT_GUIDE_CHAPTER_ID;
    const anchor = localStorage.getItem(GUIDE_ANCHOR_KEY)?.trim() || null;
    return { chapterId, anchor };
  } catch {
    return { chapterId: DEFAULT_GUIDE_CHAPTER_ID, anchor: null };
  }
}

/** Persist chapter (and optional anchor), then show the Operator's Guide window. */
export async function openGuide(
  chapterId?: string | null,
  anchor?: string | null,
): Promise<void> {
  writeGuideHandoff(chapterId, anchor);
  if (!isTauri()) {
    // Dev / browser: navigate to the guide route in-place.
    if (typeof window !== "undefined") {
      window.location.assign("/popout/guide");
    }
    return;
  }
  await showGuide();
}

export async function closeGuide(): Promise<void> {
  if (isTauri()) {
    await hideGuide();
    return;
  }
  if (typeof window !== "undefined" && window.history.length > 1) {
    window.history.back();
  }
}

/** Parse `guide:chapter` or `guide:chapter#anchor` hrefs. */
export function parseGuideHref(href: string): GuideHandoff | null {
  const raw = href.trim();
  if (!raw.startsWith("guide:")) return null;
  const rest = raw.slice("guide:".length);
  if (!rest) return null;
  const hash = rest.indexOf("#");
  const chapterId = (hash >= 0 ? rest.slice(0, hash) : rest).trim();
  const anchor = hash >= 0 ? rest.slice(hash + 1).trim() : null;
  if (!getGuideChapter(chapterId)) return null;
  return { chapterId, anchor: anchor || null };
}
