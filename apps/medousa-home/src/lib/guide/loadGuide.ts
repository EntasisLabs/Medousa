import { GUIDE_CHAPTERS, getGuideChapter } from "./catalog";

const pageModules = import.meta.glob("./pages/*.md", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

function pageKey(file: string): string {
  return `./pages/${file}`;
}

/** Raw markdown for a chapter id, or null if missing. */
export function loadGuideMarkdown(chapterId: string): string | null {
  const chapter = getGuideChapter(chapterId);
  if (!chapter) return null;
  const raw = pageModules[pageKey(chapter.file)];
  return typeof raw === "string" ? raw : null;
}

/** Every catalog chapter has a bundled page (dev/test guard). */
export function missingGuidePages(): string[] {
  return GUIDE_CHAPTERS.filter((chapter) => !pageModules[pageKey(chapter.file)]).map(
    (chapter) => chapter.file,
  );
}
