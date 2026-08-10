import { highlightTree } from "@lezer/highlight";
import type { LanguageSupport } from "@codemirror/language";
import { javascript } from "@codemirror/lang-javascript";
import { json } from "@codemirror/lang-json";
import { html } from "@codemirror/lang-html";
import { css } from "@codemirror/lang-css";
import { xml } from "@codemirror/lang-xml";
import { python } from "@codemirror/lang-python";
import { rust } from "@codemirror/lang-rust";
import { yaml } from "@codemirror/lang-yaml";
import { go } from "@codemirror/lang-go";
import { cpp } from "@codemirror/lang-cpp";
import { java } from "@codemirror/lang-java";
import { php } from "@codemirror/lang-php";
import { svelte } from "codemirror-lang-svelte";
import {
  resolveCodeEditorLanguage,
  type CodeEditorLanguageId,
} from "$lib/code/codeEditorLanguageRegistry";
import { activeCodeSyntaxHighlightStyle } from "$lib/syntax/codemirrorSyntaxTheme";
import { readCodeEditorSyntaxTheme } from "$lib/config/codeEditorPreferences";

export type DiffHighlightSpan = {
  text: string;
  style: string | null;
};

const languageCache = new Map<CodeEditorLanguageId, LanguageSupport | null>();
const lineCache = new Map<string, DiffHighlightSpan[]>();
const LINE_CACHE_LIMIT = 2_000;

function languageSupportFor(id: CodeEditorLanguageId): LanguageSupport | null {
  if (languageCache.has(id)) return languageCache.get(id) ?? null;
  let support: LanguageSupport | null = null;
  switch (id) {
    case "javascript":
      support = javascript();
      break;
    case "jsx":
      support = javascript({ jsx: true });
      break;
    case "typescript":
      support = javascript({ typescript: true });
      break;
    case "tsx":
      support = javascript({ typescript: true, jsx: true });
      break;
    case "svelte":
      support = svelte();
      break;
    case "python":
      support = python();
      break;
    case "rust":
      support = rust();
      break;
    case "go":
      support = go();
      break;
    case "c":
    case "cpp":
      support = cpp();
      break;
    case "java":
      support = java();
      break;
    case "php":
      support = php();
      break;
    case "yaml":
      support = yaml();
      break;
    case "json":
      support = json();
      break;
    case "html":
      support = html();
      break;
    case "css":
      support = css();
      break;
    case "xml":
      support = xml();
      break;
    default:
      support = null;
  }
  languageCache.set(id, support);
  return support;
}

function escapePlain(text: string): DiffHighlightSpan[] {
  return text ? [{ text, style: null }] : [];
}

/**
 * Highlight a single source line with the same Lezer style map as the editor.
 * Returns style+text spans suitable for rendering without an EditorView.
 * Memoized per (language, line).
 */
export function highlightDiffLine(
  line: string,
  languageHint: string | null | undefined,
): DiffHighlightSpan[] {
  if (!line) return [];
  const languageId = resolveCodeEditorLanguage(languageHint);
  const themeId = readCodeEditorSyntaxTheme();
  const cacheKey = `${themeId}\0${languageId}\0${line}`;
  const cached = lineCache.get(cacheKey);
  if (cached) return cached;

  const support = languageSupportFor(languageId);
  if (!support) {
    const plain = escapePlain(line);
    remember(cacheKey, plain);
    return plain;
  }

  try {
    const tree = support.language.parser.parse(line);
    const spans: DiffHighlightSpan[] = [];
    let last = 0;
    const highlightStyle = activeCodeSyntaxHighlightStyle();
    highlightTree(tree, highlightStyle, (from, to, style) => {
      if (from > last) {
        spans.push({ text: line.slice(last, from), style: null });
      }
      spans.push({ text: line.slice(from, to), style: style || null });
      last = to;
    });
    if (last < line.length) {
      spans.push({ text: line.slice(last), style: null });
    }
    const result = spans.length ? spans : escapePlain(line);
    remember(cacheKey, result);
    return result;
  } catch {
    const plain = escapePlain(line);
    remember(cacheKey, plain);
    return plain;
  }
}

function remember(key: string, value: DiffHighlightSpan[]) {
  if (lineCache.size >= LINE_CACHE_LIMIT) {
    const first = lineCache.keys().next().value;
    if (first != null) lineCache.delete(first);
  }
  lineCache.set(key, value);
}

/** Resolve a language id from a file path for diff highlighting. */
export function languageHintForPath(path: string | null | undefined): string {
  if (!path) return "plaintext";
  const base = path.split("/").pop() ?? path;
  const ext = base.includes(".") ? base.split(".").pop()?.toLowerCase() ?? "" : "";
  return resolveCodeEditorLanguage(ext);
}
