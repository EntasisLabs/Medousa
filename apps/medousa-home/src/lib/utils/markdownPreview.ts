import { wikilinkLabel } from "$lib/utils/formatVault";

type MarkdownBlock =
  | { kind: "heading"; level: number; text: string }
  | { kind: "list"; items: string[] }
  | { kind: "code"; language: string; content: string }
  | { kind: "paragraph"; text: string }
  | { kind: "blank" };

/** Lightweight markdown preview — wikilinks, inline code, fenced code blocks. */
export function renderMarkdownPreview(
  source: string,
  titleByPath?: Map<string, string>,
): string {
  return parseBlocks(source)
    .map((block) => renderBlock(block, titleByPath))
    .join("\n");
}

function parseBlocks(source: string): MarkdownBlock[] {
  const lines = source.replace(/\r\n/g, "\n").split("\n");
  const blocks: MarkdownBlock[] = [];
  let index = 0;

  while (index < lines.length) {
    const line = lines[index];
    const trimmed = line.trim();

    const fenceOpen = trimmed.match(/^```([^\s`]*)$/);
    if (fenceOpen) {
      const language = fenceOpen[1] ?? "";
      index += 1;
      const codeLines: string[] = [];
      while (index < lines.length && !lines[index].trim().startsWith("```")) {
        codeLines.push(lines[index]);
        index += 1;
      }
      if (index < lines.length) {
        index += 1;
      }
      blocks.push({ kind: "code", language, content: codeLines.join("\n") });
      continue;
    }

    if (trimmed === "") {
      blocks.push({ kind: "blank" });
      index += 1;
      continue;
    }

    if (/^#{1,6}\s/.test(line)) {
      const level = line.match(/^#+/)?.[0].length ?? 1;
      const text = line.replace(/^#{1,6}\s+/, "");
      blocks.push({ kind: "heading", level, text });
      index += 1;
      continue;
    }

    if (/^[-*]\s/.test(line)) {
      const items: string[] = [];
      while (index < lines.length && /^[-*]\s/.test(lines[index])) {
        items.push(lines[index].replace(/^[-*]\s+/, ""));
        index += 1;
      }
      blocks.push({ kind: "list", items });
      continue;
    }

    const paragraphLines: string[] = [line];
    index += 1;
    while (
      index < lines.length &&
      lines[index].trim() !== "" &&
      !/^#{1,6}\s/.test(lines[index]) &&
      !/^[-*]\s/.test(lines[index]) &&
      !lines[index].trim().startsWith("```")
    ) {
      paragraphLines.push(lines[index]);
      index += 1;
    }
    blocks.push({ kind: "paragraph", text: paragraphLines.join("\n") });
  }

  return blocks;
}

function renderBlock(
  block: MarkdownBlock,
  titleByPath?: Map<string, string>,
): string {
  switch (block.kind) {
    case "blank":
      return "<br />";
    case "heading": {
      const level = Math.min(block.level, 6);
      return `<h${level} class="font-semibold mt-4 mb-2 text-surface-50">${inline(
        block.text,
        titleByPath,
      )}</h${level}>`;
    }
    case "list":
      return `<ul class="my-2 list-disc space-y-1 pl-5">${block.items
        .map(
          (item) =>
            `<li class="leading-relaxed">${inline(item, titleByPath)}</li>`,
        )
        .join("")}</ul>`;
    case "code": {
      const language = block.language.trim();
      const label = language
        ? `<span class="markdown-code-lang">${escapeHtml(language)}</span>`
        : "";
      return `<div class="markdown-code-block">${label}<pre class="markdown-pre"><code class="markdown-code">${escapeHtml(block.content)}</code></pre></div>`;
    }
    case "paragraph":
      return `<p class="mb-2 leading-relaxed">${inline(block.text, titleByPath)}</p>`;
  }
}

function inline(text: string, titleByPath?: Map<string, string>): string {
  const escaped = escapeHtml(text);
  return escaped
    .replace(/\[\[([^\]]+)\]\]/g, (_match, target: string) => {
      const label = wikilinkLabel(target, titleByPath);
      return `<span class="text-primary-400">${escapeHtml(label)}</span>`;
    })
    .replace(
      /`([^`]+)`/g,
      '<code class="markdown-inline-code">$1</code>',
    )
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/\*([^*]+)\*/g, "<em>$1</em>")
    .replace(/\n/g, "<br />");
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}
