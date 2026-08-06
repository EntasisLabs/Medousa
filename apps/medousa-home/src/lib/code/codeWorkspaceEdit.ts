import { workspaceRelativePathFromUri } from "./codeDocumentUri";
import type {
  ForgeSourceFile,
  ForgeSourceWorkspaceOperation,
  ForgeSourceWorkspacePrecondition,
} from "$lib/forge";

type LspPosition = { line?: unknown; character?: unknown };
type LspRange = { start?: LspPosition; end?: LspPosition };
type LspTextEdit = {
  range?: LspRange;
  newText?: unknown;
  annotationId?: unknown;
};

type VirtualFile = {
  content: string;
  /** Initial project path for this file identity; null means newly created. */
  lineage: string | null;
};

export type CodeWorkspaceEditPreviewFile = {
  id: string;
  path: string;
  oldPath?: string;
  status: "modified" | "created" | "deleted" | "renamed";
  before: string;
  after: string;
};

export type CodeWorkspaceEditPlan = {
  preconditions: ForgeSourceWorkspacePrecondition[];
  operations: ForgeSourceWorkspaceOperation[];
  files: CodeWorkspaceEditPreviewFile[];
  annotationLabels: string[];
};

export class CodeWorkspaceEditError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "CodeWorkspaceEditError";
  }
}

export type BuildCodeWorkspaceEditOptions = {
  workspaceRoot: string;
  loadSource: (path: string) => Promise<ForgeSourceFile | null>;
};

function objectValue(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function booleanOption(value: unknown, key: string): boolean {
  return objectValue(value)?.[key] === true;
}

function requireUriPath(
  value: unknown,
  workspaceRoot: string,
  label: string,
): string {
  if (typeof value !== "string" || !value) {
    throw new CodeWorkspaceEditError(`${label} is missing a file URI.`);
  }
  const path = workspaceRelativePathFromUri(value, workspaceRoot);
  if (!path) {
    throw new CodeWorkspaceEditError(
      `${label} points outside this project and cannot be applied: ${value}`,
    );
  }
  return path;
}

function integerPosition(value: unknown, label: string): number {
  if (!Number.isInteger(value) || (value as number) < 0) {
    throw new CodeWorkspaceEditError(`${label} must be a non-negative integer.`);
  }
  return value as number;
}

function lineStarts(content: string): number[] {
  const starts = [0];
  for (let index = 0; index < content.length; index += 1) {
    if (content.charCodeAt(index) === 10) starts.push(index + 1);
  }
  return starts;
}

/** LSP positions are UTF-16 code-unit offsets, which match JavaScript indices. */
function textOffset(
  content: string,
  starts: number[],
  rawPosition: unknown,
  label: string,
): number {
  const position = objectValue(rawPosition);
  if (!position) throw new CodeWorkspaceEditError(`${label} is missing.`);
  const line = integerPosition(position.line, `${label}.line`);
  const character = integerPosition(position.character, `${label}.character`);
  if (line >= starts.length) {
    throw new CodeWorkspaceEditError(
      `${label}.line ${line} is outside the ${starts.length}-line document.`,
    );
  }
  const start = starts[line];
  const rawEnd = line + 1 < starts.length ? starts[line + 1] - 1 : content.length;
  const logicalEnd = rawEnd > start && content.charCodeAt(rawEnd - 1) === 13
    ? rawEnd - 1
    : rawEnd;
  if (start + character > logicalEnd) {
    throw new CodeWorkspaceEditError(
      `${label}.character ${character} is outside line ${line}.`,
    );
  }
  return start + character;
}

export function applyCodeTextEdits(
  content: string,
  rawEdits: unknown,
  label = "Text edit",
): string {
  if (!Array.isArray(rawEdits)) {
    throw new CodeWorkspaceEditError(`${label} must contain an edits array.`);
  }
  const starts = lineStarts(content);
  const edits = rawEdits.map((rawEdit, index) => {
    const edit = objectValue(rawEdit) as LspTextEdit | null;
    const range = objectValue(edit?.range) as LspRange | null;
    if (!edit || !range || typeof edit.newText !== "string") {
      throw new CodeWorkspaceEditError(
        `${label} edit ${index + 1} is missing a range or replacement text.`,
      );
    }
    const from = textOffset(
      content,
      starts,
      range.start,
      `${label} edit ${index + 1} start`,
    );
    const to = textOffset(
      content,
      starts,
      range.end,
      `${label} edit ${index + 1} end`,
    );
    if (to < from) {
      throw new CodeWorkspaceEditError(`${label} edit ${index + 1} has a reversed range.`);
    }
    return { from, to, text: edit.newText, index };
  });

  const ascending = [...edits].sort(
    (left, right) => left.from - right.from || left.to - right.to || left.index - right.index,
  );
  for (let index = 1; index < ascending.length; index += 1) {
    const previous = ascending[index - 1];
    const current = ascending[index];
    const sameInsertion =
      previous.from === previous.to &&
      current.from === current.to &&
      previous.from === current.from;
    if (
      !sameInsertion &&
      (current.from < previous.to || current.from === previous.from)
    ) {
      throw new CodeWorkspaceEditError(`${label} contains overlapping edits.`);
    }
  }

  let next = content;
  for (const edit of edits.sort(
    (left, right) => right.from - left.from || right.index - left.index,
  )) {
    next = `${next.slice(0, edit.from)}${edit.text}${next.slice(edit.to)}`;
  }
  return next;
}

function referencedAnnotationIds(edit: unknown): string[] {
  const ids: string[] = [];
  const visit = (value: unknown) => {
    if (Array.isArray(value)) {
      for (const entry of value) visit(entry);
      return;
    }
    const object = objectValue(value);
    if (!object) return;
    if (typeof object.annotationId === "string") ids.push(object.annotationId);
    for (const [key, child] of Object.entries(object)) {
      if (key !== "changeAnnotations") visit(child);
    }
  };
  visit(edit);
  return [...new Set(ids)];
}

function annotationLabels(edit: Record<string, unknown>): string[] {
  const annotations = objectValue(edit.changeAnnotations);
  return referencedAnnotationIds(edit).map((id) => {
    const annotation = objectValue(annotations?.[id]);
    const label = annotation?.label;
    const description = annotation?.description;
    if (typeof label === "string" && label.trim()) return label.trim();
    if (typeof description === "string" && description.trim()) return description.trim();
    return id;
  });
}

function previewFiles(
  initial: Map<string, ForgeSourceFile | null>,
  state: Map<string, VirtualFile>,
): CodeWorkspaceEditPreviewFile[] {
  const files: CodeWorkspaceEditPreviewFile[] = [];
  const finalByLineage = new Map<string, { path: string; file: VirtualFile }>();
  for (const [path, file] of state) {
    if (file.lineage != null) finalByLineage.set(file.lineage, { path, file });
  }

  for (const [initialPath, source] of initial) {
    if (!source) continue;
    const final = finalByLineage.get(initialPath);
    if (!final) {
      files.push({
        id: `deleted:${initialPath}`,
        path: initialPath,
        status: "deleted",
        before: source.content,
        after: "",
      });
    } else if (final.path !== initialPath) {
      files.push({
        id: `renamed:${initialPath}:${final.path}`,
        path: final.path,
        oldPath: initialPath,
        status: "renamed",
        before: source.content,
        after: final.file.content,
      });
    } else if (final.file.content !== source.content) {
      files.push({
        id: `modified:${initialPath}`,
        path: initialPath,
        status: "modified",
        before: source.content,
        after: final.file.content,
      });
    }
  }

  for (const [path, file] of state) {
    if (file.lineage == null) {
      files.push({
        id: `created:${path}`,
        path,
        status: "created",
        before: "",
        after: file.content,
      });
    }
  }
  return files;
}

/**
 * Normalize a complete LSP WorkspaceEdit into a daemon transaction and a
 * human-reviewable before/after plan. Resource operations stay ordered.
 */
export async function buildCodeWorkspaceEditPlan(
  rawResult: unknown,
  options: BuildCodeWorkspaceEditOptions,
): Promise<CodeWorkspaceEditPlan> {
  const wrapper = objectValue(rawResult);
  const edit = objectValue(wrapper?.edit) ?? wrapper;
  if (!edit) throw new CodeWorkspaceEditError("The language server returned no workspace edit.");
  if (!options.workspaceRoot.trim()) {
    throw new CodeWorkspaceEditError("The project workspace root is unavailable.");
  }

  const initial = new Map<string, ForgeSourceFile | null>();
  const state = new Map<string, VirtualFile>();
  const operations: ForgeSourceWorkspaceOperation[] = [];

  const loadInitial = async (path: string): Promise<ForgeSourceFile | null> => {
    if (initial.has(path)) return initial.get(path) ?? null;
    let source: ForgeSourceFile | null;
    try {
      source = await options.loadSource(path);
    } catch (cause) {
      const detail = cause instanceof Error ? cause.message : String(cause);
      throw new CodeWorkspaceEditError(`Could not read ${path} before refactoring: ${detail}`);
    }
    if (
      source &&
      (source.path !== path || typeof source.content !== "string" || !source.digest)
    ) {
      throw new CodeWorkspaceEditError(`The workshop returned an invalid source snapshot for ${path}.`);
    }
    initial.set(path, source);
    if (source) state.set(path, { content: source.content, lineage: path });
    return source;
  };

  const requireFile = async (path: string, label: string): Promise<VirtualFile> => {
    await loadInitial(path);
    const file = state.get(path);
    if (!file) throw new CodeWorkspaceEditError(`${label} requires ${path}, but it does not exist.`);
    return file;
  };

  const applyTextDocumentEdit = async (
    uri: unknown,
    edits: unknown,
    label: string,
  ) => {
    const path = requireUriPath(uri, options.workspaceRoot, label);
    const file = await requireFile(path, label);
    const content = applyCodeTextEdits(file.content, edits, `${label} for ${path}`);
    if (content === file.content) return;
    state.set(path, { ...file, content });
    operations.push({ kind: "write", path, content });
  };

  const changes = objectValue(edit.changes);
  if (changes) {
    for (const uri of Object.keys(changes).sort()) {
      await applyTextDocumentEdit(uri, changes[uri], "Workspace edit");
    }
  } else if (edit.changes != null) {
    throw new CodeWorkspaceEditError("Workspace edit changes must be a URI-to-edits map.");
  }

  if (edit.documentChanges != null && !Array.isArray(edit.documentChanges)) {
    throw new CodeWorkspaceEditError("Workspace edit documentChanges must be an array.");
  }
  for (const [index, rawChange] of (edit.documentChanges as unknown[] | undefined ?? []).entries()) {
    const change = objectValue(rawChange);
    if (!change) {
      throw new CodeWorkspaceEditError(`Workspace operation ${index + 1} is invalid.`);
    }
    const label = `Workspace operation ${index + 1}`;
    const textDocument = objectValue(change.textDocument);
    if (textDocument) {
      await applyTextDocumentEdit(textDocument.uri, change.edits, label);
      continue;
    }

    if (change.kind === "create") {
      const path = requireUriPath(change.uri, options.workspaceRoot, `${label} create`);
      await loadInitial(path);
      const existing = state.get(path);
      if (existing) {
        if (booleanOption(change.options, "overwrite")) {
          if (existing.content !== "") {
            state.set(path, { ...existing, content: "" });
            operations.push({ kind: "write", path, content: "" });
          }
        } else if (!booleanOption(change.options, "ignoreIfExists")) {
          throw new CodeWorkspaceEditError(`Cannot create ${path} because it already exists.`);
        }
      } else {
        state.set(path, { content: "", lineage: null });
        operations.push({ kind: "create", path, content: "" });
      }
      continue;
    }

    if (change.kind === "rename") {
      const path = requireUriPath(change.oldUri, options.workspaceRoot, `${label} rename source`);
      const destination = requireUriPath(
        change.newUri,
        options.workspaceRoot,
        `${label} rename destination`,
      );
      if (path === destination) continue;
      const source = await requireFile(path, `${label} rename`);
      await loadInitial(destination);
      if (state.has(destination)) {
        if (booleanOption(change.options, "overwrite")) {
          state.delete(destination);
          operations.push({ kind: "delete", path: destination });
        } else if (booleanOption(change.options, "ignoreIfExists")) {
          continue;
        } else {
          throw new CodeWorkspaceEditError(
            `Cannot rename ${path} to ${destination} because the destination exists.`,
          );
        }
      }
      state.delete(path);
      state.set(destination, source);
      operations.push({ kind: "rename", path, destination });
      continue;
    }

    if (change.kind === "delete") {
      const path = requireUriPath(change.uri, options.workspaceRoot, `${label} delete`);
      if (booleanOption(change.options, "recursive")) {
        throw new CodeWorkspaceEditError(
          `Recursive directory deletion is not supported by the source workbench (${path}).`,
        );
      }
      await loadInitial(path);
      if (!state.has(path)) {
        if (!booleanOption(change.options, "ignoreIfNotExists")) {
          throw new CodeWorkspaceEditError(`Cannot delete ${path} because it does not exist.`);
        }
      } else {
        state.delete(path);
        operations.push({ kind: "delete", path });
      }
      continue;
    }

    throw new CodeWorkspaceEditError(
      `${label} uses an unsupported resource operation${typeof change.kind === "string" ? ` (${change.kind})` : ""}.`,
    );
  }

  const touched = new Set<string>();
  for (const operation of operations) {
    touched.add(operation.path);
    if (operation.kind === "rename") touched.add(operation.destination);
  }
  const preconditions: ForgeSourceWorkspacePrecondition[] = [];
  for (const [path, source] of initial) {
    if (!touched.has(path)) continue;
    preconditions.push(
      source
        ? { kind: "existing", path, expected_digest: source.digest }
        : { kind: "missing", path },
    );
  }

  return {
    preconditions,
    operations,
    files: previewFiles(initial, state),
    annotationLabels: annotationLabels(edit),
  };
}
