/** Find-in-note state shared across editor and preview. */

import type { EditorView } from "@codemirror/view";
import {
  clearPreviewFindHighlights,
  clearTextareaFindBackdrop,
  findMatches,
  replaceAllFindMatches,
  replaceFindMatch,
  revealTextareaMatch,
  scrollPreviewToFindMatch,
  syncTextareaFindScroll,
  updateTextareaFindBackdrop,
  type FindMatch,
} from "$lib/utils/vaultFindInNote";
import { revealFindMatchInView } from "$lib/utils/vaultCodeMirror";
import type { EditResult } from "$lib/utils/vaultMarkdownEdit";

function matchesEqual(left: FindMatch[], right: FindMatch[]): boolean {
  if (left.length !== right.length) return false;
  return left.every(
    (match, index) =>
      match.start === right[index]?.start && match.end === right[index]?.end,
  );
}

export type VaultFindReplaceHandler = (result: EditResult) => void;
export type VaultFindHighlightHandler = (
  matches: FindMatch[],
  activeIndex: number,
) => void;

export class VaultFindStore {
  open = $state(false);
  query = $state("");
  replaceQuery = $state("");
  matchCase = $state(false);
  replaceMode = $state(false);
  matchIndex = $state(0);
  matches = $state<FindMatch[]>([]);
  /** Plain text used for match indexing (editor draft or rendered preview). */
  sourceText = $state("");
  /** Bumped when the active match should scroll into view. */
  revealEpoch = $state(0);

  replaceHandler: VaultFindReplaceHandler | null = null;
  highlightHandler: VaultFindHighlightHandler | null = null;

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

  openReplace(initialQuery = "") {
    this.replaceMode = true;
    this.openFind(initialQuery);
  }

  close() {
    this.open = false;
    this.replaceMode = false;
    clearPreviewFindHighlights();
    clearTextareaFindBackdrop(this.textareaEl, this.textareaBackdropEl);
    this.highlightHandler?.([], 0);
  }

  reset() {
    this.query = "";
    this.replaceQuery = "";
    this.matchCase = false;
    this.replaceMode = false;
    this.matches = [];
    this.matchIndex = 0;
    this.sourceText = "";
    this.open = false;
    clearPreviewFindHighlights();
    clearTextareaFindBackdrop(this.textareaEl, this.textareaBackdropEl);
    this.highlightHandler?.([], 0);
  }

  setQuery(query: string) {
    this.query = query;
    if (this.matchIndex !== 0) {
      this.matchIndex = 0;
    }
  }

  setReplaceQuery(query: string) {
    this.replaceQuery = query;
  }

  setMatchCase(next: boolean) {
    this.matchCase = next;
    if (this.matchIndex !== 0) {
      this.matchIndex = 0;
    }
    this.revealEpoch += 1;
  }

  toggleMatchCase() {
    this.setMatchCase(!this.matchCase);
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
    this.setMatches(
      findMatches(sourceText, this.query, { caseSensitive: this.matchCase }),
    );
  }

  registerReplaceHandler(handler: VaultFindReplaceHandler | null) {
    this.replaceHandler = handler;
  }

  registerHighlightHandler(handler: VaultFindHighlightHandler | null) {
    this.highlightHandler = handler;
  }

  replaceOne(): boolean {
    const match = this.currentMatch;
    if (!match || !this.replaceHandler || !this.query.trim()) return false;
    const result = replaceFindMatch(this.sourceText, match, this.replaceQuery);
    this.replaceHandler(result);
    return true;
  }

  replaceAll(): boolean {
    if (!this.replaceHandler || !this.query.trim()) return false;
    const result = replaceAllFindMatches(
      this.sourceText,
      this.query,
      this.replaceQuery,
      { caseSensitive: this.matchCase },
    );
    if (result.count === 0) return false;
    this.replaceHandler({
      content: result.content,
      selectionStart: result.selectionStart,
      selectionEnd: result.selectionEnd,
    });
    return true;
  }

  textareaEl: HTMLTextAreaElement | null = null;
  textareaBackdropEl: HTMLElement | null = null;
  previewEl: HTMLElement | null = null;
  codeMirrorView: EditorView | null = null;

  registerTextarea(el: HTMLTextAreaElement | null) {
    this.textareaEl = el;
  }

  registerTextareaBackdrop(el: HTMLElement | null) {
    this.textareaBackdropEl = el;
  }

  registerPreview(el: HTMLElement | null) {
    this.previewEl = el;
  }

  registerCodeMirror(view: EditorView | null) {
    this.codeMirrorView = view;
  }

  syncAndReveal(mode: "edit" | "preview") {
    if (!this.open) return;
    const next = findMatches(this.sourceText, this.query, {
      caseSensitive: this.matchCase,
    });
    this.setMatches(next);
    const index =
      next.length === 0
        ? 0
        : ((this.matchIndex % next.length) + next.length) % next.length;
    const match = next[index] ?? null;
    if (mode === "edit" && this.codeMirrorView) {
      this.highlightHandler?.(next, index);
      revealFindMatchInView(this.codeMirrorView, match);
      return;
    }
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
      scrollPreviewToFindMatch(this.previewEl, this.query, index, next, {
        caseSensitive: this.matchCase,
      });
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
