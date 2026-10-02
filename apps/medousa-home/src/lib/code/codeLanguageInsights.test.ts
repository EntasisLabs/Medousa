import { expect, it, vi } from "vitest";
import type { LSPClient } from "@codemirror/lsp-client";
import { CodeLanguageInsights } from "./codeLanguageInsights.svelte";

it("ignores old document symbols after changing files on the same client", async () => {
  let uri = "file:///repo/old.ts";
  let resolve!: (value: unknown) => void;
  const client = {
    sync: vi.fn(), request: vi.fn(() => new Promise((done) => { resolve = done; })),
  } as unknown as LSPClient;
  const onError = vi.fn();
  const insights = new CodeLanguageInsights({
    getScopeKey: () => "workshop/project", getDocumentUri: () => uri, getWorkId: () => "work",
    getLanguage: () => "typescript", getClient: () => client, onError,
  });
  const pending = insights.refreshSymbols();
  uri = "file:///repo/new.ts";
  insights.reset();
  resolve([{ name: "OldDocumentSymbol", kind: 12 }]);
  await pending;
  expect(insights.symbols).toEqual([]);
  expect(insights.symbolsLoading).toBe(false);
  expect(onError).not.toHaveBeenCalled();
});
