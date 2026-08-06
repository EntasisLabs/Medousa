import type { EditorView } from "@codemirror/view";
import { canonicalCodeDocumentUri } from "$lib/code/codeDocumentUri";

/** Live CodeMirror views, independent of which pooled language client owns them. */
class CodeEditorViewRegistry {
  private views = new Map<string, EditorView[]>();
  private waiters = new Map<string, Set<(view: EditorView | null) => void>>();

  register(uri: string, view: EditorView): () => void {
    const key = canonicalCodeDocumentUri(uri);
    const current = this.views.get(key) ?? [];
    if (!current.includes(view)) this.views.set(key, [...current, view]);
    this.resolveWaiters(key, view);
    let registered = true;
    return () => {
      if (!registered) return;
      registered = false;
      const remaining = (this.views.get(key) ?? []).filter(
        (entry) => entry !== view,
      );
      if (remaining.length > 0) this.views.set(key, remaining);
      else this.views.delete(key);
    };
  }

  get(uri: string): EditorView | null {
    const views = this.views.get(canonicalCodeDocumentUri(uri)) ?? [];
    return (
      views.find((view) => view.hasFocus) ??
      views[views.length - 1] ??
      null
    );
  }

  waitFor(uri: string, timeoutMs = 5_000): Promise<EditorView | null> {
    const key = canonicalCodeDocumentUri(uri);
    const mounted = this.get(key);
    if (mounted) return Promise.resolve(mounted);
    return new Promise((resolve) => {
      const waiters = this.waiters.get(key) ?? new Set();
      let settled = false;
      const finish = (view: EditorView | null) => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        waiters.delete(finish);
        if (waiters.size === 0) this.waiters.delete(key);
        resolve(view);
      };
      const timer = setTimeout(() => finish(null), timeoutMs);
      waiters.add(finish);
      this.waiters.set(key, waiters);
    });
  }

  private resolveWaiters(uri: string, view: EditorView) {
    const waiters = this.waiters.get(uri);
    if (!waiters) return;
    this.waiters.delete(uri);
    for (const resolve of waiters) resolve(view);
  }
}

export const codeEditorViewRegistry = new CodeEditorViewRegistry();
