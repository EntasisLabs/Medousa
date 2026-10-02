/** Preserve LSP provenance while CodeMirror maps positions into the current buffer. */
import { LSPPlugin, type LSPClient } from "@codemirror/lsp-client";
import { setDiagnostics, type Diagnostic } from "@codemirror/lint";
import type { Text } from "@codemirror/state";
import type { CodeWorkspaceDiagnostic } from "./codingEngineClient";
type RawDiagnostic = NonNullable<CodeWorkspaceDiagnostic["diagnostics"]>[number];
const metadata = new WeakMap<Diagnostic, { raw: RawDiagnostic; version?: number; document: Text }>();
export function diagnosticMetadata(diagnostic: Diagnostic) { return metadata.get(diagnostic); }
export function presentCodeDiagnostics(client: LSPClient, value: unknown): boolean {
  const params = value as CodeWorkspaceDiagnostic | null;
  if (!params || typeof params.uri !== "string" || !Array.isArray(params.diagnostics)) return true;
  const file = client.workspace.getFile(params.uri);
  if (!file || (params.version != null && params.version !== file.version)) return true;
  const view = file.getView();
  const plugin = view && LSPPlugin.get(view);
  if (!view || !plugin || !plugin.unsyncedChanges.empty) return true;
  const markers = params.diagnostics.flatMap((raw): Diagnostic[] => {
    if (!raw.range?.start || !raw.range.end || typeof raw.message !== "string") return [];
    if ([raw.range.start.line, raw.range.start.character, raw.range.end.line, raw.range.end.character].some((position) => !Number.isInteger(position) || (position ?? -1) < 0)) return [];
    const marker: Diagnostic = {
      from: plugin.unsyncedChanges.mapPos(plugin.fromPosition(raw.range.start as { line: number; character: number }, plugin.syncedDoc)),
      to: plugin.unsyncedChanges.mapPos(plugin.fromPosition(raw.range.end as { line: number; character: number }, plugin.syncedDoc)),
      severity: raw.severity === 2 ? "warning" : raw.severity === 3 ? "info" : raw.severity === 4 ? "hint" : "error",
      message: raw.message,
      source: raw.source,
    };
    marker.to = Math.max(marker.from, marker.to);
    metadata.set(marker, { raw, version: params.version, document: view.state.doc });
    return [marker];
  });
  view.dispatch(setDiagnostics(view.state, markers));
  return true;
}
