import { StreamLanguage } from "@codemirror/language";
import { tags as t } from "@lezer/highlight";

const KEYWORDS = new Set([
  "query",
  "glyph",
  "import",
  "from",
  "on",
  "set",
  "let",
  "type",
  "true",
  "false",
  "null",
  "if",
  "else",
  "match",
  "return",
  "and",
  "or",
  "not",
  "in",
]);

export const graphemeLanguage = StreamLanguage.define({
  name: "grapheme",
  token(stream) {
    if (stream.eatSpace()) return null;

    if (stream.match("//")) {
      stream.skipToEnd();
      return "lineComment";
    }

    if (stream.eat('"')) {
      let escaped = false;
      while (!stream.eol()) {
        const ch = stream.next();
        if (escaped) {
          escaped = false;
          continue;
        }
        if (ch === "\\") {
          escaped = true;
          continue;
        }
        if (ch === '"') break;
      }
      return "string";
    }

    if (stream.match("|>")) return "operator";

    if (stream.eat("$")) {
      stream.eatWhile(/[\w.]/);
      return "special";
    }

    if (stream.match(/\d+(\.\d+)?/)) return "number";

    if (stream.match(/[\w.]+/)) {
      const word = stream.current();
      if (KEYWORDS.has(word)) return "keyword";
      if (word.includes(".")) return "namespace";
      if (/^[A-Z]/.test(word)) return "typeName";
      /* Call sites: name( → function tag so --syn-function applies. */
      if (stream.match(/^\s*\(/, false)) return "function";
      return "variableName";
    }

    stream.next();
    return "operator";
  },
  tokenTable: {
    lineComment: t.lineComment,
    string: t.string,
    keyword: t.keyword,
    number: t.number,
    operator: t.operator,
    namespace: t.namespace,
    typeName: t.typeName,
    variableName: t.variableName,
    function: t.function(t.variableName),
    special: t.special(t.variableName),
  },
});
