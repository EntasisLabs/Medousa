import type { JSONContent } from "@tiptap/core";
import { marked } from "marked";
import { markdownToLiveDoc } from "$lib/vault/live/markdownToLiveDoc";
import { liveDocToMarkdown } from "$lib/vault/live/liveDocToMarkdown";
import { splitMarkdownSegments } from "$lib/vault/live/fenceCard";

type SourceBlock = { raw: string; leading: string; fingerprint: string };
const fingerprint = (node: JSONContent): string => {
  const copy = structuredClone(node);
  if (copy.attrs) delete copy.attrs.sourceBlockId;
  return JSON.stringify(copy);
};

/** Source remains authoritative. Only edited rich blocks are serialized. */
export class MarkdownSourceDocument {
  readonly doc: JSONContent;
  private blocks = new Map<string, SourceBlock>();
  private prefix: string;
  private trailing = "";
  private newline: string;
  private baseline = "";
  private implicitTail: string | null = null;
  constructor(private original: string) {
    this.prefix = /^(?:\uFEFF)?---\r?\n[\s\S]*?\r?\n---(?:\r?\n|$)/.exec(original)?.[0] ?? "";
    const body = original.slice(this.prefix.length);
    this.newline = body.includes("\r\n") ? "\r\n" : "\n";
    // Marked normalizes CRLF; retain an offset map to the original bytes.
    let normalized = "";
    const offsets = [0];
    for (let i = 0; i < body.length; i++) {
      if (body[i] === "\r" && body[i + 1] === "\n") i++;
      normalized += body[i];
      offsets.push(i + 1);
    }
    const children: JSONContent[] = [];
    let cursor = 0;
    let leading = "";
    const append = (node: JSONContent, raw: string) => {
      const id = String(children.length);
      node.attrs = { ...node.attrs, sourceBlockId: id };
      this.blocks.set(id, { raw, leading, fingerprint: "" });
      children.push(node);
      leading = "";
    };
    const gap = (raw: string) => {
      if (raw.trim()) append({ type: "markdownSourceBlock", attrs: { raw } }, raw);
      else leading += raw;
    };
    // Keep Medousa's nested organism fences whole, just as the Notes editor does.
    const tokens = splitMarkdownSegments(normalized).flatMap((segment) => segment.kind === "fence" ? [{ type: "code", raw: segment.raw }] : marked.lexer(segment.text));
    for (const token of tokens) {
      const start = normalized.indexOf(token.raw, cursor);
      if (start < 0) continue;
      gap(body.slice(offsets[cursor], offsets[start]));
      const end = start + token.raw.length;
      const raw = body.slice(offsets[start], offsets[end]);
      cursor = end;
      if (token.type === "space") { leading += raw; continue; }
      // Raw HTML, definitions, and unsupported composites never pass through a lossy codec.
      const parsed = markdownToLiveDoc(raw).content ?? [];
      const node: JSONContent = token.type === "html" || token.type === "def" || (token.type !== "code" && /<\/?[a-z][^>]*>|\]\[[^\]]*\]/i.test(raw)) || parsed.length !== 1
        ? { type: "markdownSourceBlock", attrs: { raw } }
        : parsed[0];
      // A codec must account for all visible text before we allow rich editing.
      if (token.type === "code" && node.type !== "fenceBlock") {
        node.type = "markdownSourceBlock";
        node.attrs = { raw };
        delete node.content;
      }
      append(node, raw);
    }
    gap(body.slice(offsets[cursor]));
    this.trailing = leading;
    if (["fenceBlock", "embedBlock", "markdownSourceBlock", "image"].includes(children.at(-1)?.type ?? "")) children.push({ type: "paragraph" });
    this.doc = { type: "doc", content: children.length ? children : [{ type: "paragraph" }] };
  }

  /** TipTap adds schema defaults; compare against the actual mounted document. */
  establishBaseline(doc: JSONContent) {
    this.baseline = JSON.stringify(doc);
    const last = doc.content?.at(-1);
    this.implicitTail = last && last.attrs?.sourceBlockId == null && last.type === "paragraph" && !last.content?.length ? fingerprint(last) : null;
    for (const node of doc.content ?? []) {
      const block = this.blocks.get(String(node.attrs?.sourceBlockId));
      if (block) block.fingerprint = fingerprint(node);
    }
  }

  serialize(doc: JSONContent): string {
    if (JSON.stringify(doc) === this.baseline) return this.original;
    const used = new Set<string>();
    let result = this.prefix;
    let previousId: string | null = null;
    const nodes = doc.content ?? [];
    for (const [index, node] of nodes.entries()) {
      if (index === nodes.length - 1 && node.attrs?.sourceBlockId == null && fingerprint(node) === this.implicitTail) continue;
      const id = String(node.attrs?.sourceBlockId);
      const block = used.has(id) ? undefined : this.blocks.get(id);
      used.add(id);
      if (block && fingerprint(node) === block.fingerprint) {
        let leading = block.leading;
        if (result.length > this.prefix.length && (previousId == null || Number(id) !== Number(previousId) + 1)) {
          const breaks = ((result + leading).match(/(?:\r?\n)+$/)?.[0].match(/\n/g) ?? []).length;
          if (breaks < 2) leading += this.newline.repeat(2 - breaks);
        }
        result += leading + block.raw;
        previousId = id;
        continue;
      }
      const markdown = node.type === "markdownSourceBlock" ? String(node.attrs?.raw ?? "") : liveDocToMarkdown({ type: "doc", content: [node] });
      const body = markdown.replace(/\r?\n/g, this.newline).replace(/(?:\r?\n)+$/, "");
      const suffix = block ? block.raw.match(/(?:\r?\n)+$/)?.[0] ?? "" : this.newline + this.newline;
      const leading = block?.leading ?? (result && !result.endsWith(this.newline + this.newline) ? (result.endsWith(this.newline) ? this.newline : this.newline + this.newline) : "");
      result += leading + body + suffix;
      previousId = block ? id : null;
    }
    return result + this.trailing;
  }
}
