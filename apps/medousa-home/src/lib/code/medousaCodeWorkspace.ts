import {
  ChangeSet,
  EditorState,
  Text,
  type TransactionSpec,
} from "@codemirror/state";
import {
  LSPPlugin,
  Workspace,
  type LSPClient,
  type WorkspaceFile,
} from "@codemirror/lsp-client";
import type { EditorView } from "@codemirror/view";
import { canonicalCodeDocumentUri } from "$lib/code/codeDocumentUri";

export { canonicalCodeDocumentUri } from "$lib/code/codeDocumentUri";

export type MedousaWorkspaceDocument = {
  languageId: string;
  text: string | Text;
};

export type MedousaCodeWorkspaceHandler = {
  handlesUri?: (uri: string) => boolean;
  requestFile?: (
    uri: string,
  ) => Promise<MedousaWorkspaceDocument | null> | MedousaWorkspaceDocument | null;
  displayFile?: (
    uri: string,
  ) =>
    | Promise<EditorView | "handled" | null>
    | EditorView
    | "handled"
    | null;
};

/**
 * Connects a pooled language client to whichever Home code surface can load
 * and reveal documents for that project. Newer registrations get first refusal
 * so the most recently mounted editor group wins without owning the LSP client.
 */
export class MedousaCodeWorkspaceBridge {
  private handlers: MedousaCodeWorkspaceHandler[] = [];

  register(handler: MedousaCodeWorkspaceHandler): () => void {
    this.handlers = [...this.handlers, handler];
    let registered = true;
    return () => {
      if (!registered) return;
      registered = false;
      this.handlers = this.handlers.filter((entry) => entry !== handler);
    };
  }

  async requestFile(uri: string): Promise<MedousaWorkspaceDocument | null> {
    for (let index = this.handlers.length - 1; index >= 0; index -= 1) {
      const handler = this.handlers[index];
      if (handler.handlesUri && !handler.handlesUri(uri)) continue;
      const document = await handler.requestFile?.(uri);
      if (document) return document;
    }
    return null;
  }

  async displayFile(uri: string): Promise<EditorView | "handled" | null> {
    for (let index = this.handlers.length - 1; index >= 0; index -= 1) {
      const handler = this.handlers[index];
      if (handler.handlesUri && !handler.handlesUri(uri)) continue;
      const view = await handler.displayFile?.(uri);
      if (view) return view;
    }
    return null;
  }
}

export class MedousaCodeWorkspaceConflictError extends Error {
  constructor(readonly uri: string) {
    super(`Editor views for ${uri} contain divergent unsynchronized drafts`);
    this.name = "MedousaCodeWorkspaceConflictError";
  }
}

class MedousaWorkspaceFile implements WorkspaceFile {
  readonly views: EditorView[] = [];
  pendingDoc: Text | null = null;

  constructor(
    readonly uri: string,
    public languageId: string,
    public version: number,
    public doc: Text,
    readonly retainWhenHeadless: boolean,
  ) {}

  getView(main?: EditorView): EditorView | null {
    if (main && this.views.includes(main)) return main;
    return (
      this.views.find((view) => view.hasFocus) ??
      this.views[this.views.length - 1] ??
      null
    );
  }
}

function asText(value: string | Text): Text {
  return typeof value === "string" ? Text.of(value.split("\n")) : value;
}

function replaceDocument(doc: Text, replacement: Text): ChangeSet {
  return ChangeSet.of(
    { from: 0, to: doc.length, insert: replacement },
    doc.length,
  );
}

/**
 * CodeMirror workspace implementation for Home's pooled project-language
 * clients. It supports multiple views of one URI and headless documents used
 * by references while preserving the daemon/workshop as filesystem authority.
 */
export class MedousaCodeWorkspace extends Workspace {
  files: MedousaWorkspaceFile[] = [];
  private fileVersions = new Map<string, number>();
  private requestingFiles = new Map<
    string,
    Promise<MedousaWorkspaceFile | null>
  >();
  private viewWaiters = new Map<
    string,
    Set<(view: EditorView | null) => void>
  >();

  constructor(
    client: LSPClient,
    readonly bridge: MedousaCodeWorkspaceBridge,
  ) {
    super(client);
  }

  override getFile(uri: string): MedousaWorkspaceFile | null {
    const canonicalUri = canonicalCodeDocumentUri(uri);
    return this.files.find((file) => file.uri === canonicalUri) ?? null;
  }

  private nextFileVersion(uri: string): number {
    const version = (this.fileVersions.get(uri) ?? -1) + 1;
    this.fileVersions.set(uri, version);
    return version;
  }

  override openFile(uri: string, languageId: string, view: EditorView): void {
    const canonicalUri = canonicalCodeDocumentUri(uri);
    let file = this.getFile(canonicalUri);
    if (!file) {
      file = new MedousaWorkspaceFile(
        canonicalUri,
        languageId,
        this.nextFileVersion(canonicalUri),
        view.state.doc,
        false,
      );
      file.views.push(view);
      this.files = [...this.files, file];
      this.client.didOpen(file);
      this.resolveViewWaiters(canonicalUri, view);
      return;
    }

    if (file.views.includes(view)) return;
    if (!file.languageId && languageId) file.languageId = languageId;
    if (!file.doc.eq(view.state.doc)) {
      if (file.views.length > 0) {
        throw new MedousaCodeWorkspaceConflictError(canonicalUri);
      }
      file.pendingDoc = view.state.doc;
    }
    file.views.push(view);
    this.resolveViewWaiters(canonicalUri, view);
  }

  override closeFile(uri: string, view: EditorView): void {
    const file = this.getFile(uri);
    if (!file) return;
    const index = file.views.indexOf(view);
    if (index >= 0) file.views.splice(index, 1);
    if (file.views.length > 0 || file.retainWhenHeadless) return;
    this.files = this.files.filter((entry) => entry !== file);
    this.client.didClose(file.uri);
  }

  override async requestFile(uri: string): Promise<WorkspaceFile | null> {
    const canonicalUri = canonicalCodeDocumentUri(uri);
    const existing = this.getFile(canonicalUri);
    if (existing) return existing;
    const pending = this.requestingFiles.get(canonicalUri);
    if (pending) return pending;
    const request = (async () => {
      const document = await this.bridge.requestFile(canonicalUri);
      if (!document) return null;
      const raced = this.getFile(canonicalUri);
      if (raced) return raced;
      const file = new MedousaWorkspaceFile(
        canonicalUri,
        document.languageId,
        this.nextFileVersion(canonicalUri),
        asText(document.text),
        true,
      );
      this.files = [...this.files, file];
      this.client.didOpen(file);
      return file;
    })().finally(() => {
      if (this.requestingFiles.get(canonicalUri) === request) {
        this.requestingFiles.delete(canonicalUri);
      }
    });
    this.requestingFiles.set(canonicalUri, request);
    return request;
  }

  override async displayFile(uri: string): Promise<EditorView | null> {
    const canonicalUri = canonicalCodeDocumentUri(uri);
    const displayed = await this.bridge.displayFile(canonicalUri);
    if (displayed !== "handled") {
      return displayed ?? this.getFile(canonicalUri)?.getView() ?? null;
    }
    const mounted = this.getFile(canonicalUri)?.getView();
    return mounted ?? this.waitForView(canonicalUri);
  }

  private waitForView(uri: string, timeoutMs = 5_000): Promise<EditorView | null> {
    const mounted = this.getFile(uri)?.getView();
    if (mounted) return Promise.resolve(mounted);
    return new Promise((resolve) => {
      const waiters = this.viewWaiters.get(uri) ?? new Set();
      let settled = false;
      const finish = (view: EditorView | null) => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        waiters.delete(finish);
        if (waiters.size === 0) this.viewWaiters.delete(uri);
        resolve(view);
      };
      const timer = setTimeout(() => finish(null), timeoutMs);
      waiters.add(finish);
      this.viewWaiters.set(uri, waiters);
    });
  }

  private resolveViewWaiters(uri: string, view: EditorView): void {
    const waiters = this.viewWaiters.get(uri);
    if (!waiters) return;
    this.viewWaiters.delete(uri);
    for (const resolve of waiters) resolve(view);
  }

  override updateFile(uri: string, update: TransactionSpec): void {
    const file = this.getFile(uri);
    if (!file) return;
    if (file.views.length > 0) {
      for (const view of file.views) view.dispatch(update);
      return;
    }
    const state = EditorState.create({ doc: file.pendingDoc ?? file.doc });
    file.pendingDoc = state.update(update).state.doc;
  }

  override syncFiles() {
    const updates: Array<{
      file: WorkspaceFile;
      prevDoc: Text;
      changes: ChangeSet;
    }> = [];

    for (const file of this.files) {
      const dirtyViews = file.views
        .map((view) => ({ view, plugin: LSPPlugin.get(view) }))
        .filter(({ plugin }) => plugin && !plugin.unsyncedChanges.empty) as Array<{
        view: EditorView;
        plugin: LSPPlugin;
      }>;

      let nextDoc = file.pendingDoc;
      for (const { view } of dirtyViews) {
        if (nextDoc && !nextDoc.eq(view.state.doc)) {
          throw new MedousaCodeWorkspaceConflictError(file.uri);
        }
        nextDoc = view.state.doc;
      }
      if (!nextDoc) continue;

      for (const view of file.views) {
        if (nextDoc.eq(view.state.doc)) continue;
        const plugin = LSPPlugin.get(view);
        if (plugin && !plugin.unsyncedChanges.empty) {
          throw new MedousaCodeWorkspaceConflictError(file.uri);
        }
        if (!file.doc.eq(view.state.doc)) {
          throw new MedousaCodeWorkspaceConflictError(file.uri);
        }
      }

      const prevDoc = file.doc;
      const primaryPlugin = dirtyViews[0]?.plugin;
      const changes =
        primaryPlugin?.syncedDoc.eq(prevDoc) && nextDoc.eq(dirtyViews[0].view.state.doc)
          ? primaryPlugin.unsyncedChanges
          : replaceDocument(prevDoc, nextDoc);

      for (const view of file.views) {
        if (!nextDoc.eq(view.state.doc)) view.dispatch({ changes });
      }
      file.doc = nextDoc;
      file.pendingDoc = null;
      file.version = this.nextFileVersion(file.uri);
      for (const view of file.views) LSPPlugin.get(view)?.clear();
      updates.push({ file, prevDoc, changes });
    }

    return updates;
  }
}
