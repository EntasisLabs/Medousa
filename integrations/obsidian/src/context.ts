import { App, MarkdownView, TFile } from "obsidian";
import { boundContext, type MedousaContext } from "@medousa/client";

const MAX_NOTE_CHARS = 18_000;
const MAX_LINKS = 24;

export interface ObsidianContextSnapshot {
  context: MedousaContext;
  label: string;
  file: TFile | null;
}

export async function captureObsidianContext(
  app: App,
  preferredView?: MarkdownView | null,
): Promise<ObsidianContextSnapshot> {
  const view = preferredView ?? app.workspace.getActiveViewOfType(MarkdownView);
  const file = view?.file ?? app.workspace.getActiveFile();
  const editor = view?.editor;
  const selectedText = editor?.getSelection().trim() ?? "";
  const from = selectedText ? editor?.getCursor("from") : undefined;
  const to = selectedText ? editor?.getCursor("to") : undefined;
  const content = file ? await app.vault.cachedRead(file) : "";
  const metadata = file ? app.metadataCache.getFileCache(file) : null;
  const links = Array.from(
    new Set((metadata?.links ?? []).map((link) => link.link).filter(Boolean)),
  ).slice(0, MAX_LINKS);

  const context = boundContext({
    surface: "obsidian",
    workspace: app.vault.getName(),
    file: file?.path,
    notePath: file?.path,
    language: file ? "markdown" : undefined,
    documentExcerpt: file ? boundedNote(content) : undefined,
    relatedResources: links,
    selection: selectedText
      ? {
          text: selectedText,
          start: from ? { line: from.line, character: from.ch } : undefined,
          end: to ? { line: to.line, character: to.ch } : undefined,
        }
      : undefined,
  });

  return {
    context,
    label: file ? file.path : "vault workspace",
    file,
  };
}

function boundedNote(content: string): string {
  if (content.length <= MAX_NOTE_CHARS) return content;
  const head = Math.floor(MAX_NOTE_CHARS * 0.72);
  const tail = MAX_NOTE_CHARS - head;
  return `${content.slice(0, head)}\n\n[… note content bounded by Medousa …]\n\n${content.slice(-tail)}`;
}
