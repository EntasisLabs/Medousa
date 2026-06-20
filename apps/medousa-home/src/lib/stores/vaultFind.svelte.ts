/** Find-in-note state shared across editor and preview. */

import {
  clearPreviewFindHighlights,
  clearTextareaFindBackdrop,
  findMatches,
  revealTextareaMatch,
  scrollPreviewToFindMatch,
  syncTextareaFindScroll,
  updateTextareaFindBackdrop,
  type FindMatch,
} from "$lib/utils/vaultFindInNote";

function matchesEqual(left: FindMatch[], right: FindMatch[]): boolean {
  if (left.length !== right.length) return false;
  return left.every(
    (match, index) =>
      match.start === right[index]?.start && match.end === right[index]?.end,
  );
}

export class VaultFindStore {
  open = $state(false);
  query = $state("");
  matchIndex = $state(0);
  matches = $state<FindMatch[]>([]);
  /** Plain text used for match indexing (editor draft or rendered preview). */
  sourceText = $state("");
  /** Bumped when the active match should scroll into view. */
  revealEpoch = $state(0);

  matchCount = $derived(this.matches.length);
  currentMatch = $derived(this.matches[this.matchIndex] ?? null);
  statusLabel = $derived.by(() => {
    if (!this.query.trim()) return "";
    if (this.matchCount === 0) return "No results";
    return `${this.matchIndex + 1} of ${this.matchCount}`;
  });

  openFind(initialQuery = "") {
    this.open = true;
    if (initialQuery && !this.query) {
      this.query = initialQuery;
    }
    this.revealEpoch += 1;
  }

  close() {
    this.open = false;
    clearPreviewFindHighlights();
    clearTextareaFindBackdrop(this.textareaEl, this.textareaBackdropEl);
  }

  reset() {
    this.query = "";
    this.matches = [];
    this.matchIndex = 0;
    this.sourceText = "";
    this.open = false;
    clearPreviewFindHighlights();
    clearTextareaFindBackdrop(this.textareaEl, this.textareaBackdropEl);
  }

  setQuery(query: string) {
    this.query = query;
    if (this.matchIndex !== 0) {
      this.matchIndex = 0;
    }
  }

  /** Replace matches only when the result set actually changed. */
  setSourceText(text: string) {
    if (this.sourceText === text) return;
    this.sourceText = text;
  }

  setMatches(next: FindMatch[]) {
    if (matchesEqual(this.matches, next)) return;
    this.matches = next;
    if (next.length === 0) {
      if (this.matchIndex !== 0) this.matchIndex = 0;
      return;
    }
    if (this.matchIndex >= next.length) {
      this.matchIndex = 0;
    }
  }

  refreshMatches(sourceText: string) {
    this.setMatches(findMatches(sourceText, this.query));
  }

  textareaEl: HTMLTextAreaElement | null = null;
  textareaBackdropEl: HTMLElement | null = null;
  previewEl: HTMLElement | null = null;

  registerTextarea(el: HTMLTextAreaElement | null) {
    this.textareaEl = el;
  }

  registerTextareaBackdrop(el: HTMLElement | null) {
    this.textareaBackdropEl = el;
  }

  registerPreview(el: HTMLElement | null) {
    this.previewEl = el;
  }

  syncAndReveal(mode: "edit" | "preview") {
    if (!this.open) return;
    const next = findMatches(this.sourceText, this.query);
    this.setMatches(next);
    const index =
      next.length === 0
        ? 0
        : ((this.matchIndex % next.length) + next.length) % next.length;
    const match = next[index] ?? null;
    if (mode === "edit" && this.textareaEl) {
      updateTextareaFindBackdrop(
        this.textareaEl,
        this.textareaBackdropEl,
        this.sourceText,
        next,
        index,
        this.query,
      );
      revealTextareaMatch(this.textareaEl, match);
      if (this.textareaBackdropEl) {
        syncTextareaFindScroll(this.textareaEl, this.textareaBackdropEl);
      }
      return;
    }
    if (mode === "preview" && this.previewEl) {
      scrollPreviewToFindMatch(this.previewEl, this.query, index, next);
    }
  }

  requestReveal() {
    this.revealEpoch += 1;
  }

  next() {
    if (this.matchCount === 0) return;
    this.matchIndex = (this.matchIndex + 1) % this.matchCount;
    this.revealEpoch += 1;
  }

  prev() {
    if (this.matchCount === 0) return;
    this.matchIndex = (this.matchIndex - 1 + this.matchCount) % this.matchCount;
    this.revealEpoch += 1;
  }
}

export const vaultFind = new VaultFindStore();
