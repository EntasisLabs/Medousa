import { Marked, type Token } from "marked";

export interface StreamingMarkdownUpdate {
  reset: boolean;
  completed: string[];
  tail: string;
}

/**
 * Retains an append-only Markdown prefix and exposes only confirmed lexer
 * blocks. Two final non-space tokens stay mutable so an incomplete table,
 * fence, list, or HTML-ish block can be reclassified as more bytes arrive.
 */
export class StreamingMarkdownBlocks {
  private source = "";
  private committedLength = 0;

  update(source: string, terminal: boolean): StreamingMarkdownUpdate {
    let reset = false;
    if (!source.startsWith(this.source)) {
      this.source = "";
      this.committedLength = 0;
      reset = true;
    }
    this.source = source;

    const pending = source.slice(this.committedLength);
    if (!pending) return { reset, completed: [], tail: "" };
    if (terminal) {
      this.committedLength = source.length;
      return { reset, completed: [pending], tail: "" };
    }
    // Definitions can retroactively change earlier reference/footnote tokens.
    // Keep that document suffix together until terminal reconciliation.
    if (hasDeferredReferenceSyntax(pending)) {
      return { reset, completed: [], tail: pending };
    }

    const parser = new Marked();
    parser.use({ gfm: true, breaks: false });
    const tokens = parser.lexer(pending);
    const stableTokenCount = confirmedTokenCount(tokens);
    if (stableTokenCount === 0) {
      return { reset, completed: [], tail: pending };
    }

    const stableTokens = tokens.slice(0, stableTokenCount);
    const stableLength = stableTokens.reduce((length, token) => length + token.raw.length, 0);
    const stableSource = pending.slice(0, stableLength);
    const completed = tokenBlocks(stableTokens, stableSource);
    this.committedLength += stableLength;
    return {
      reset,
      completed,
      tail: source.slice(this.committedLength),
    };
  }
}

function hasDeferredReferenceSyntax(source: string): boolean {
  return (
    /\[\^[^\]]+\]/.test(source) ||
    /\[[^\]]+\]\[[^\]]*\]/.test(source) ||
    // A later `[label]: target` definition can turn a shortcut `[label]`
    // into a link. Do not freeze that earlier token while the document grows.
    /(?<!\[)!?\[[^\]\n]+\](?!\s*(?:\(|\[)|\])/.test(source) ||
    /^\s*\[[^\]]+\]:\s*\S/m.test(source)
  );
}

function confirmedTokenCount(tokens: Token[]): number {
  const contentIndexes: number[] = [];
  for (let index = tokens.length - 1; index >= 0; index -= 1) {
    if (tokens[index].type === "space") continue;
    contentIndexes.push(index);
    if (contentIndexes.length === 3) {
      // Exclude the two newest content tokens and the whitespace between them.
      return contentIndexes[1];
    }
  }
  return 0;
}

function tokenBlocks(tokens: Token[], source: string): string[] {
  const blocks: string[] = [];
  let offset = 0;
  let blockStart = 0;
  for (let index = 0; index < tokens.length; index += 1) {
    offset += tokens[index].raw.length;
    const next = tokens[index + 1];
    if (tokens[index].type !== "space" && next?.type !== "space") {
      blocks.push(source.slice(blockStart, offset));
      blockStart = offset;
    }
    if (tokens[index].type === "space" && next?.type !== "space") {
      blocks.push(source.slice(blockStart, offset));
      blockStart = offset;
    }
  }
  if (blockStart < source.length) blocks.push(source.slice(blockStart));
  return blocks.filter((block) => block.length > 0);
}
