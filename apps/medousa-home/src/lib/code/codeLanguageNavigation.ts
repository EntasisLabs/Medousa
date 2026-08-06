import { LSPPlugin } from "@codemirror/lsp-client";
import type { EditorView } from "@codemirror/view";
import { canonicalCodeDocumentUri } from "$lib/code/codeDocumentUri";

export type CodeLanguageNavigationKind =
  | "definition"
  | "declaration"
  | "typeDefinition"
  | "implementation";

export type CodeLanguageNavigationResult = {
  kind: CodeLanguageNavigationKind;
  uri: string;
  line: number;
  character: number;
  alternatives: number;
};

type Position = { line: number; character: number };
type Range = { start: Position; end: Position };
type Location = { uri: string; range: Range };
type LocationLink = {
  targetUri: string;
  targetRange: Range;
  targetSelectionRange?: Range;
};

const NAVIGATION_REQUESTS: Record<
  CodeLanguageNavigationKind,
  { method: string; capability: string }
> = {
  definition: {
    method: "textDocument/definition",
    capability: "definitionProvider",
  },
  declaration: {
    method: "textDocument/declaration",
    capability: "declarationProvider",
  },
  typeDefinition: {
    method: "textDocument/typeDefinition",
    capability: "typeDefinitionProvider",
  },
  implementation: {
    method: "textDocument/implementation",
    capability: "implementationProvider",
  },
};

function isPosition(value: unknown): value is Position {
  if (!value || typeof value !== "object") return false;
  const position = value as Partial<Position>;
  return (
    Number.isInteger(position.line) &&
    Number.isInteger(position.character) &&
    (position.line ?? -1) >= 0 &&
    (position.character ?? -1) >= 0
  );
}

function normalizeLocation(value: unknown): Location | null {
  if (!value || typeof value !== "object") return null;
  const candidate = value as Partial<Location & LocationLink>;
  if (candidate.uri && isPosition(candidate.range?.start)) {
    return {
      uri: canonicalCodeDocumentUri(candidate.uri),
      range: candidate.range!,
    };
  }
  if (candidate.targetUri && isPosition(candidate.targetRange?.start)) {
    return {
      uri: canonicalCodeDocumentUri(candidate.targetUri),
      range:
        candidate.targetSelectionRange &&
        isPosition(candidate.targetSelectionRange.start)
          ? candidate.targetSelectionRange
          : candidate.targetRange!,
    };
  }
  return null;
}

/** Request and reveal an LSP navigation target, including LocationLink replies. */
export async function navigateToCodeLanguageLocation(
  view: EditorView,
  kind: CodeLanguageNavigationKind,
): Promise<CodeLanguageNavigationResult | null> {
  const plugin = LSPPlugin.get(view);
  if (!plugin) return null;
  const request = NAVIGATION_REQUESTS[kind];
  const capabilities = plugin.client.serverCapabilities as Record<string, unknown> | null;
  if (capabilities && !capabilities[request.capability]) {
    return null;
  }

  plugin.client.sync();
  const params = {
    textDocument: { uri: plugin.uri },
    position: plugin.toPosition(view.state.selection.main.head),
  };
  return plugin.client.withMapping(async (mapping) => {
    const response = await plugin.client.request<
      typeof params,
      Location | LocationLink | Array<Location | LocationLink> | null
    >(request.method, params);
    const rawLocations = Array.isArray(response)
      ? response
      : response
        ? [response]
        : [];
    const locations = rawLocations
      .map(normalizeLocation)
      .filter((location): location is Location => Boolean(location));

    for (const location of locations) {
      const currentUri = canonicalCodeDocumentUri(plugin.uri);
      const target =
        location.uri === currentUri
          ? view
          : await plugin.client.workspace.displayFile(location.uri);
      if (!target) continue;
      const position = mapping.getMapping(location.uri)
        ? mapping.mapPosition(location.uri, location.range.start)
        : plugin.fromPosition(location.range.start, target.state.doc);
      target.dispatch({
        selection: { anchor: position },
        scrollIntoView: true,
        userEvent: "select.definition",
      });
      target.focus();
      const currentLine = target.state.doc.lineAt(position);
      return {
        kind,
        uri: location.uri,
        line: currentLine.number,
        character: position - currentLine.from + 1,
        alternatives: locations.length,
      };
    }
    return null;
  });
}
