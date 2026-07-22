import { tags as t } from "@lezer/highlight";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";

/** CodeMirror highlight style mapped to shared `--syn-*` tokens. */
export const medousaSyntaxHighlightStyle = HighlightStyle.define([
  { tag: t.keyword, color: "rgb(var(--syn-keyword))" },
  {
    tag: [t.function(t.variableName), t.function(t.propertyName)],
    color: "rgb(var(--syn-function))",
  },
  { tag: [t.typeName, t.className, t.namespace], color: "rgb(var(--syn-type))" },
  { tag: t.string, color: "rgb(var(--syn-string))" },
  { tag: [t.number, t.bool, t.atom], color: "rgb(var(--syn-number))" },
  { tag: t.operator, color: "rgb(var(--syn-operator))" },
  { tag: t.punctuation, color: "rgb(var(--syn-punctuation))" },
  { tag: t.variableName, color: "rgb(var(--syn-fg))" },
  { tag: t.special(t.variableName), color: "rgb(var(--syn-attr))" },
  { tag: [t.propertyName, t.attributeName], color: "rgb(var(--syn-attr))" },
  { tag: [t.comment, t.lineComment, t.blockComment], color: "rgb(var(--syn-comment))", fontStyle: "italic" },
  { tag: t.meta, color: "rgb(var(--syn-meta))" },
  { tag: t.literal, color: "rgb(var(--syn-string))" },
]);

export const medousaSyntaxHighlighting = syntaxHighlighting(medousaSyntaxHighlightStyle);
