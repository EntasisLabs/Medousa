import type { Extension } from "@codemirror/state";
import { Prec } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { tags as t } from "@lezer/highlight";

export type CodeSyntaxThemeId =
  | "dark-plus"
  | "one-dark"
  | "monokai"
  | "github-dark"
  | "github-light"
  | "dracula";

export const CODE_SYNTAX_THEME_IDS: readonly CodeSyntaxThemeId[] = [
  "dark-plus",
  "one-dark",
  "monokai",
  "github-dark",
  "github-light",
  "dracula",
] as const;

export const DEFAULT_CODE_SYNTAX_THEME: CodeSyntaxThemeId = "dark-plus";

type TokenPalette = {
  fg: string;
  keyword: string;
  function: string;
  type: string;
  string: string;
  number: string;
  operator: string;
  punctuation: string;
  attr: string;
  comment: string;
  meta: string;
};

type CanvasPalette = {
  background: string;
  foreground: string;
  caret: string;
  selection: string;
  selectionMatch: string;
  activeLine: string;
  gutterBackground: string;
  gutterForeground: string;
  gutterActiveForeground: string;
  gutterBorder: string;
};

export type CodeSyntaxThemeDefinition = {
  id: CodeSyntaxThemeId;
  label: string;
  tagline: string;
  dark: boolean;
  tokens: TokenPalette;
  canvas: CanvasPalette;
  highlightStyle: HighlightStyle;
  highlighting: Extension;
  editorTheme: Extension;
  extensions: Extension[];
};

function defineHighlightStyle(tokens: TokenPalette): HighlightStyle {
  return HighlightStyle.define([
    { tag: t.keyword, color: tokens.keyword },
    {
      tag: [t.function(t.variableName), t.function(t.propertyName)],
      color: tokens.function,
    },
    { tag: [t.typeName, t.className, t.namespace], color: tokens.type },
    { tag: t.string, color: tokens.string },
    { tag: [t.number, t.bool, t.atom], color: tokens.number },
    { tag: t.operator, color: tokens.operator },
    { tag: t.punctuation, color: tokens.punctuation },
    { tag: t.variableName, color: tokens.fg },
    { tag: t.special(t.variableName), color: tokens.attr },
    { tag: [t.propertyName, t.attributeName], color: tokens.attr },
    {
      tag: [t.comment, t.lineComment, t.blockComment],
      color: tokens.comment,
      fontStyle: "italic",
    },
    { tag: t.meta, color: tokens.meta },
    { tag: t.literal, color: tokens.string },
  ]);
}

function defineEditorTheme(canvas: CanvasPalette, dark: boolean): Extension {
  return [
    EditorView.darkTheme.of(dark),
    Prec.high(
      EditorView.theme(
        {
          "&": {
            color: canvas.foreground,
            backgroundColor: canvas.background,
          },
          ".cm-scroller": {
            backgroundColor: canvas.background,
          },
          ".cm-content": {
            caretColor: canvas.caret,
            color: canvas.foreground,
            backgroundColor: canvas.background,
          },
          ".cm-cursor, .cm-dropCursor": {
            borderLeftColor: canvas.caret,
          },
          "&.cm-focused .cm-cursor": {
            borderLeftColor: canvas.caret,
          },
          ".cm-selectionBackground, &.cm-focused .cm-selectionBackground": {
            backgroundColor: `${canvas.selection} !important`,
          },
          ".cm-selectionMatch": {
            backgroundColor: canvas.selectionMatch,
          },
          ".cm-activeLine": {
            backgroundColor: canvas.activeLine,
          },
          ".cm-gutters": {
            backgroundColor: canvas.background,
            color: canvas.gutterForeground,
            border: "none",
          },
          ".cm-activeLineGutter": {
            backgroundColor: canvas.activeLine,
            color: canvas.gutterActiveForeground,
          },
          ".cm-foldGutter span": {
            color: canvas.gutterForeground,
            opacity: "0.85",
          },
          /* Base theme uses #eee — reads as a white “minimize” chip on dark packs. */
          ".cm-foldPlaceholder": {
            backgroundColor: canvas.activeLine,
            border: `1px solid ${canvas.gutterBorder}`,
            color: canvas.gutterForeground,
            borderRadius: "0.2em",
            margin: "0 1px",
            padding: "0 0.35em",
            cursor: "pointer",
          },
        },
        { dark },
      ),
    ),
  ];
}

function buildTheme(
  id: CodeSyntaxThemeId,
  label: string,
  tagline: string,
  dark: boolean,
  tokens: TokenPalette,
  canvas: CanvasPalette,
): CodeSyntaxThemeDefinition {
  const highlightStyle = defineHighlightStyle(tokens);
  const highlighting = syntaxHighlighting(highlightStyle);
  const editorTheme = defineEditorTheme(canvas, dark);
  return {
    id,
    label,
    tagline,
    dark,
    tokens,
    canvas,
    highlightStyle,
    highlighting,
    editorTheme,
    extensions: [editorTheme, highlighting],
  };
}

const THEMES: Record<CodeSyntaxThemeId, CodeSyntaxThemeDefinition> = {
  "dark-plus": buildTheme(
    "dark-plus",
    "Dark+",
    "VS Code classic — cool keywords on charcoal",
    true,
    {
      fg: "#d4d4d4",
      keyword: "#569cd6",
      function: "#dcdcaa",
      type: "#4ec9b0",
      string: "#ce9178",
      number: "#b5cea8",
      operator: "#d4d4d4",
      punctuation: "#d4d4d4",
      attr: "#9cdcfe",
      comment: "#6a9955",
      meta: "#c586c0",
    },
    {
      background: "#1e1e1e",
      foreground: "#d4d4d4",
      caret: "#aeafad",
      selection: "#264f78",
      selectionMatch: "#3a3d41",
      activeLine: "#2a2a2a",
      gutterBackground: "#1e1e1e",
      gutterForeground: "#858585",
      gutterActiveForeground: "#c6c6c6",
      gutterBorder: "#252526",
    },
  ),
  "one-dark": buildTheme(
    "one-dark",
    "One Dark",
    "Atom heritage — purple keywords, soft contrast",
    true,
    {
      fg: "#abb2bf",
      keyword: "#c678dd",
      function: "#61afef",
      type: "#e5c07b",
      string: "#98c379",
      number: "#d19a66",
      operator: "#56b6c2",
      punctuation: "#abb2bf",
      attr: "#e06c75",
      comment: "#5c6370",
      meta: "#56b6c2",
    },
    {
      background: "#282c34",
      foreground: "#abb2bf",
      caret: "#528bff",
      selection: "#3e4451",
      selectionMatch: "#3e4451",
      activeLine: "#2c313c",
      gutterBackground: "#282c34",
      gutterForeground: "#4b5263",
      gutterActiveForeground: "#abb2bf",
      gutterBorder: "#181a1f",
    },
  ),
  monokai: buildTheme(
    "monokai",
    "Monokai",
    "Sublime heat — pink keywords, neon types",
    true,
    {
      fg: "#f8f8f2",
      keyword: "#f92672",
      function: "#a6e22e",
      type: "#66d9ef",
      string: "#e6db74",
      number: "#ae81ff",
      operator: "#f92672",
      punctuation: "#f8f8f2",
      attr: "#a6e22e",
      comment: "#75715e",
      meta: "#66d9ef",
    },
    {
      background: "#272822",
      foreground: "#f8f8f2",
      caret: "#f8f8f0",
      selection: "#49483e",
      selectionMatch: "#3e3d32",
      activeLine: "#3e3d32",
      gutterBackground: "#272822",
      gutterForeground: "#90908a",
      gutterActiveForeground: "#c2c2bf",
      gutterBorder: "#3e3d32",
    },
  ),
  "github-dark": buildTheme(
    "github-dark",
    "GitHub Dark",
    "PR night mode — coral keywords on ink",
    true,
    {
      fg: "#e6edf3",
      keyword: "#ff7b72",
      function: "#d2a8ff",
      type: "#ffa657",
      string: "#a5d6ff",
      number: "#79c0ff",
      operator: "#ff7b72",
      punctuation: "#e6edf3",
      attr: "#79c0ff",
      comment: "#8b949e",
      meta: "#ffa657",
    },
    {
      background: "#0d1117",
      foreground: "#e6edf3",
      caret: "#e6edf3",
      selection: "#1f6feb55",
      selectionMatch: "#1f6feb33",
      activeLine: "#161b22",
      gutterBackground: "#0d1117",
      gutterForeground: "#6e7681",
      gutterActiveForeground: "#e6edf3",
      gutterBorder: "#21262d",
    },
  ),
  "github-light": buildTheme(
    "github-light",
    "GitHub Light",
    "Daylight review — ink on paper",
    false,
    {
      fg: "#1f2328",
      keyword: "#cf222e",
      function: "#6639ba",
      type: "#953800",
      string: "#0a3069",
      number: "#0550ae",
      operator: "#1f2328",
      punctuation: "#1f2328",
      attr: "#0550ae",
      comment: "#59636e",
      meta: "#1a7f37",
    },
    {
      background: "#ffffff",
      foreground: "#1f2328",
      caret: "#1f2328",
      selection: "#0969da40",
      selectionMatch: "#0969da28",
      activeLine: "#eaeef2",
      gutterBackground: "#ffffff",
      gutterForeground: "#656d76",
      gutterActiveForeground: "#1f2328",
      gutterBorder: "#d0d7de",
    },
  ),
  dracula: buildTheme(
    "dracula",
    "Dracula",
    "Gothic neon — pink keywords, mint functions",
    true,
    {
      fg: "#f8f8f2",
      keyword: "#ff79c6",
      function: "#50fa7b",
      type: "#8be9fd",
      string: "#f1fa8c",
      number: "#bd93f9",
      operator: "#ff79c6",
      punctuation: "#f8f8f2",
      attr: "#50fa7b",
      comment: "#6272a4",
      meta: "#8be9fd",
    },
    {
      background: "#282a36",
      foreground: "#f8f8f2",
      caret: "#f8f8f0",
      selection: "#44475a",
      selectionMatch: "#44475a",
      activeLine: "#44475a55",
      gutterBackground: "#282a36",
      gutterForeground: "#6272a4",
      gutterActiveForeground: "#f8f8f2",
      gutterBorder: "#191a21",
    },
  ),
};

export function resolveCodeSyntaxTheme(
  raw: string | null | undefined,
): CodeSyntaxThemeId {
  const key = (raw ?? "").trim().toLowerCase();
  return (CODE_SYNTAX_THEME_IDS as readonly string[]).includes(key)
    ? (key as CodeSyntaxThemeId)
    : DEFAULT_CODE_SYNTAX_THEME;
}

export function getCodeSyntaxTheme(
  id: CodeSyntaxThemeId | string | null | undefined,
): CodeSyntaxThemeDefinition {
  return THEMES[resolveCodeSyntaxTheme(id)];
}

export function buildCodeSyntaxThemeExtensions(
  id: CodeSyntaxThemeId | string | null | undefined,
): Extension[] {
  return getCodeSyntaxTheme(id).extensions;
}

export function listCodeSyntaxThemes(): Array<{
  id: CodeSyntaxThemeId;
  label: string;
  tagline: string;
  dark: boolean;
}> {
  return CODE_SYNTAX_THEME_IDS.map((id) => {
    const theme = THEMES[id];
    return {
      id: theme.id,
      label: theme.label,
      tagline: theme.tagline,
      dark: theme.dark,
    };
  });
}

export function cycleCodeSyntaxTheme(
  current: CodeSyntaxThemeId | string | null | undefined,
): CodeSyntaxThemeId {
  const resolved = resolveCodeSyntaxTheme(current);
  const index = CODE_SYNTAX_THEME_IDS.indexOf(resolved);
  return CODE_SYNTAX_THEME_IDS[(index + 1) % CODE_SYNTAX_THEME_IDS.length]!;
}

/** Tiny colored spans for settings / PowerShell-style theme previews. */
export type CodeSyntaxPreviewSpan = {
  text: string;
  color: string;
};

export function codeSyntaxPreviewLines(
  id: CodeSyntaxThemeId | string | null | undefined,
): CodeSyntaxPreviewSpan[][] {
  const { tokens } = getCodeSyntaxTheme(id);
  return [
    [
      { text: "fn ", color: tokens.keyword },
      { text: "main", color: tokens.function },
      { text: "() {", color: tokens.punctuation },
    ],
    [
      { text: "  let ", color: tokens.keyword },
      { text: "x", color: tokens.fg },
      { text: " = ", color: tokens.operator },
      { text: "42", color: tokens.number },
      { text: ";", color: tokens.punctuation },
    ],
    [
      { text: "  // ready", color: tokens.comment },
    ],
    [
      { text: "  print", color: tokens.function },
      { text: "(", color: tokens.punctuation },
      { text: '"ok"', color: tokens.string },
      { text: ")", color: tokens.punctuation },
    ],
    [
      { text: "}", color: tokens.punctuation },
    ],
  ];
}
