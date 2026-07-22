import { StreamLanguage } from "@codemirror/language";
import { tags as t } from "@lezer/highlight";

const KEYWORDS = new Set([
  "if",
  "then",
  "else",
  "elif",
  "fi",
  "for",
  "do",
  "done",
  "while",
  "case",
  "esac",
  "in",
  "function",
  "return",
  "export",
  "local",
  "readonly",
  "declare",
  "unset",
  "set",
  "exit",
  "break",
  "continue",
  "true",
  "false",
]);

/** Minimal bash/shell highlighter for read-only script snippets (no LSP). */
export const shellLanguage = StreamLanguage.define({
  name: "shell",
  token(stream) {
    if (stream.eatSpace()) return null;

    if (stream.match("#")) {
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

    if (stream.eat("'")) {
      while (!stream.eol() && stream.next() !== "'") {
        /* consume literal */
      }
      return "string";
    }

    if (stream.match(/\$\{/) || stream.match(/\$\(/)) {
      stream.skipTo("}") || stream.skipTo(")");
      stream.next();
      return "special";
    }

    if (stream.eat("$")) {
      stream.eatWhile(/[\w_]/);
      return "special";
    }

    if (stream.match(/[\w_]+/)) {
      const word = stream.current();
      if (KEYWORDS.has(word)) return "keyword";
      return "variableName";
    }

    stream.next();
    return "operator";
  },
  tokenTable: {
    lineComment: t.lineComment,
    string: t.string,
    keyword: t.keyword,
    operator: t.operator,
    variableName: t.variableName,
    special: t.special(t.variableName),
  },
});
