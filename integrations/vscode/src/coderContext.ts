import * as path from "node:path";
import type {
  CodeIntentContext,
  ForgeUndertaking,
  MedousaContext,
} from "@medousa/client";

export function buildCodeIntentContext(
  context: MedousaContext,
  undertaking: ForgeUndertaking,
  openFiles: string[] = [],
): CodeIntentContext {
  const worktree = undertaking.environment?.worktree;
  return {
    work_id: undertaking.id,
    project_title: undertaking.title,
    outcome: undertaking.brief,
    active_path: worktree && context.file
      ? relativePathWithin(worktree, context.file)
      : undefined,
    cursor_line: context.cursor ? context.cursor.line + 1 : undefined,
    selection_start_line: context.selection?.start
      ? context.selection.start.line + 1
      : undefined,
    selection_end_line: context.selection?.end
      ? context.selection.end.line + 1
      : undefined,
    selected_text: context.selection?.text,
    open_files: worktree
      ? openFiles
          .map((file) => relativePathWithin(worktree, file))
          .filter((file): file is string => Boolean(file))
          .slice(0, 24)
      : [],
    diagnostics: (context.diagnostics ?? []).slice(0, 24).map((diagnostic) =>
      [diagnostic.severity, diagnostic.source, diagnostic.message]
        .filter(Boolean)
        .join(" · "),
    ),
  };
}

export function relativePathWithin(root: string, candidate: string): string | undefined {
  const relative = path.relative(path.resolve(root), path.resolve(candidate));
  if (!relative || relative === ".") return undefined;
  if (relative === ".." || relative.startsWith(`..${path.sep}`) || path.isAbsolute(relative)) {
    return undefined;
  }
  return relative.split(path.sep).join("/");
}
