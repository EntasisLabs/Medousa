import { StateEffect, StateField } from "@codemirror/state";
import { undo } from "@codemirror/commands";
import { EditorView, type ViewUpdate } from "@codemirror/view";
import {
  closeSearchPanel, findNext, findPrevious, openSearchPanel,
  replaceAll, replaceNext, search, SearchQuery, searchPanelOpen, setSearchQuery,
} from "@codemirror/search";

type SelectionScope = { from: number; to: number } | null;
const setSelectionScope = StateEffect.define<SelectionScope>();
const selectionScope = StateField.define<SelectionScope>({
  create: () => null,
  update(range, transaction) {
    if (range && transaction.docChanged) range = {
      from: transaction.changes.mapPos(range.from, -1),
      to: transaction.changes.mapPos(range.to, 1),
    };
    for (const effect of transaction.effects) if (effect.is(setSelectionScope)) range = effect.value;
    return range;
  },
});

export const codeFindExtension = [
  selectionScope,
  search({
    // Search state/highlights stay native. The Svelte host owns the visible bar.
    createPanel: () => {
      const dom = document.createElement("div");
      dom.className = "code-find-engine-panel";
      dom.hidden = true;
      return { dom };
    },
    scrollToMatch: (range) => EditorView.scrollIntoView(range, { y: "center", yMargin: 96 }),
  }),
  EditorView.theme({ ".cm-panels:has(.code-find-engine-panel)": { display: "none" } }),
];

/** File-local preferences survive tab switches and editor preference remounts. */
export class CodeFindState {
  open = $state(false);
  query = $state("");
  replacement = $state("");
  matchCase = $state(false);
  wholeWord = $state(false);
  regexp = $state(false);
  replaceMode = $state(false);
  selection = $state<{ from: number; to: number } | null>(null);
}

export class CodeFindStateCache {
  private scope = "";
  private files = new Map<string, CodeFindState>();

  forFile(scope: string, tabId: string | null, openTabIds: string[]) {
    if (scope !== this.scope) { this.files.clear(); this.scope = scope; }
    for (const id of this.files.keys()) if (!openTabIds.includes(id)) this.files.delete(id);
    if (!tabId) return undefined;
    let state = this.files.get(tabId);
    if (!state) { state = new CodeFindState(); this.files.set(tabId, state); }
    return state;
  }
}

const COUNT_LIMIT = 10_000;

/** The floating chrome delegates matching, captures, edits, and history to CodeMirror. */
export class CodeFindController {
  private view: EditorView | undefined;
  private matches: Array<{ from: number; to: number }> = [];
  private selectionCandidate: SelectionScope = null;
  private bar: HTMLElement | undefined;
  matchCount = $state(0);
  matchIndex = $state(0);
  countLimited = $state(false);
  canSelect = $state(false);
  readOnly = $state(false);
  error = $state("");
  feedback = $state("");
  replacementUndoAvailable = $state(false);
  focusEpoch = $state(0);
  floatBelow = $state(false);

  constructor(private getState: () => CodeFindState) {}
  get state() { return this.getState(); }
  get status() {
    if (this.error) return "Invalid expression";
    if (!this.state.query) return "";
    if (!this.matchCount) return "No results";
    if (this.countLimited) return `${COUNT_LIMIT.toLocaleString()}+ matches`;
    return this.matchIndex ? `${this.matchIndex} of ${this.matchCount}` : `${this.matchCount} matches`;
  }

  bind(view: EditorView) {
    this.view = view;
    // A persisted selection cannot outlive a replacement of the entire buffer.
    const range = this.state.selection;
    if (range && range.to > view.state.doc.length) this.state.selection = null;
    if (this.state.open) { openSearchPanel(view); this.commit(); }
    this.refresh(true);
  }

  destroy() { this.view = undefined; }

  attachBar(bar: HTMLElement) {
    this.bar = bar;
    const observer = new ResizeObserver(() => this.avoidActiveMatch());
    observer.observe(bar);
    this.avoidActiveMatch();
    return { destroy: () => { observer.disconnect(); this.bar = undefined; } };
  }

  private avoidActiveMatch() {
    const view = this.view;
    const bar = this.bar;
    if (!view || !bar) return;
    view.requestMeasure({
      key: this,
      read: () => {
        if (!this.matchIndex || !this.state.open) return false;
        const parent = bar.offsetParent;
        const from = view.coordsAtPos(view.state.selection.main.from);
        const to = view.coordsAtPos(view.state.selection.main.to);
        if (!parent || !from || !to) return false;
        const container = parent.getBoundingClientRect();
        const size = bar.getBoundingClientRect();
        const right = container.right - 12;
        const left = right - size.width;
        const top = container.top + 10;
        return from.top < top + size.height && to.bottom > top &&
          Math.min(from.left, to.left) < right && Math.max(from.right, to.right) > left;
      },
      write: (overlaps) => { if (this.view === view && this.bar === bar) this.floatBelow = overlaps; },
    });
  }

  show(replace = false) {
    const view = this.view;
    if (!view) return;
    const selected = view.state.selection.main;
    if (!this.state.open && !selected.empty) this.selectionCandidate = { from: selected.from, to: selected.to };
    if (!this.state.open && !selected.empty && selected.to - selected.from <= 1_000) {
      const text = view.state.sliceDoc(selected.from, selected.to);
      this.state.query = this.state.regexp ? text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&") : text;
    }
    this.state.open = true;
    if (replace && !view.state.readOnly) this.state.replaceMode = true;
    openSearchPanel(view);
    this.commit();
    this.focusEpoch++;
  }

  close() {
    this.state.open = false;
    this.feedback = "";
    if (this.view) { closeSearchPanel(this.view); this.view.focus(); }
  }

  private query() {
    return new SearchQuery({
      search: this.state.query, replace: this.state.replacement, literal: true,
      caseSensitive: this.state.matchCase, wholeWord: this.state.wholeWord,
      regexp: this.state.regexp,
      test: this.state.selection
        ? (_match, state, from, to) => {
            const range = state.field(selectionScope);
            return !!range && from >= range.from && to <= range.to;
          }
        : undefined,
    });
  }

  private commit() {
    if (!this.view) return;
    this.feedback = "";
    const query = this.query();
    this.error = this.state.regexp && this.state.query && !query.valid
      ? "Invalid regular expression. Check the pattern." : "";
    this.view.dispatch({ effects: [setSelectionScope.of(this.state.selection), setSearchQuery.of(query)] });
    this.refresh(true);
  }

  setQuery(value: string) {
    this.state.query = value;
    this.commit();
    if (this.view && this.matchCount && !this.matchIndex) {
      // Extending a query should keep its current hit, rather than skip it.
      this.view.dispatch({ selection: { anchor: this.view.state.selection.main.from }, userEvent: "select.search" });
      this.next();
    }
  }

  setReplacement(value: string) { this.state.replacement = value; this.commit(); }

  toggle(option: "matchCase" | "wholeWord" | "regexp") {
    this.state[option] = !this.state[option];
    this.commit();
    if (this.matchCount && !this.matchIndex) this.next();
  }

  toggleSelection() {
    if (!this.view) return;
    if (this.state.selection) this.state.selection = null;
    else {
      const { from, to } = this.selectionCandidate ?? this.view.state.selection.main;
      if (from === to) return;
      this.state.selection = { from, to };
    }
    this.commit();
    if (this.matchCount && !this.matchIndex) this.next();
  }

  next() { if (this.view && this.matchCount) findNext(this.view); }
  previous() { if (this.view && this.matchCount) findPrevious(this.view); }

  replace(all = false) {
    const view = this.view;
    if (!view || view.state.readOnly || !this.matchCount || this.error) return;
    // Replace the current hit on the first click, even if the cursor moved away.
    const selected = view.state.selection.main;
    const current = this.query().getCursor(view.state, selected.from, selected.to).next();
    const selectedMatch = !current.done && current.value.from === selected.from && current.value.to === selected.to;
    if (!all && !this.matchIndex && !selectedMatch) findNext(view);
    const before = view.state.doc;
    const count = this.matchCount;
    const limited = this.countLimited;
    (all ? replaceAll : replaceNext)(view);
    if (view.state.doc !== before) {
      this.feedback = all
        ? limited ? "Replaced all matches" : `Replaced ${count} ${count === 1 ? "match" : "matches"}`
        : "Replaced 1 match";
      this.replacementUndoAvailable = true;
    }
  }

  undoReplacement() {
    if (!this.view || this.view.state.readOnly || !this.replacementUndoAvailable) return;
    if (undo(this.view)) this.feedback = "Replacement undone";
  }

  update(update: ViewUpdate) {
    if (update.docChanged && this.selectionCandidate) this.selectionCandidate = {
      from: update.changes.mapPos(this.selectionCandidate.from, -1),
      to: update.changes.mapPos(this.selectionCandidate.to, 1),
    };
    if (update.selectionSet && update.transactions.some((tr) => tr.selection && !tr.isUserEvent("select.search") && !tr.docChanged)) {
      const selected = update.state.selection.main;
      this.selectionCandidate = selected.empty ? null : { from: selected.from, to: selected.to };
    }
    if (update.docChanged && this.state.selection) {
      this.state.selection = update.state.field(selectionScope);
    }
    if (this.view && !searchPanelOpen(update.state) && this.state.open) this.state.open = false;
    if (update.docChanged) { this.feedback = ""; this.replacementUndoAvailable = false; }
    if (update.docChanged || update.selectionSet || update.transactions.some((tr) => tr.effects.length)) {
      this.refresh(update.docChanged);
    } else if (update.viewportChanged || update.geometryChanged) this.avoidActiveMatch();
  }

  private refresh(rescan: boolean) {
    const view = this.view;
    if (!view) return;
    this.readOnly = view.state.readOnly;
    this.canSelect = !!this.selectionCandidate || !view.state.selection.main.empty;
    if (!this.state.open) return;
    if (rescan) {
      this.matches = [];
      this.countLimited = false;
      const query = this.query();
      if (query.valid) {
        const cursor = query.getCursor(view.state);
        for (let match = cursor.next(); !match.done; match = cursor.next()) {
          if (this.matches.length === COUNT_LIMIT) { this.countLimited = true; break; }
          this.matches.push({ from: match.value.from, to: match.value.to });
        }
      }
      this.matchCount = this.matches.length;
    }
    const selection = view.state.selection.main;
    this.matchIndex = this.matches.findIndex(({ from, to }) => from === selection.from && to === selection.to) + 1;
    this.avoidActiveMatch();
  }
}
