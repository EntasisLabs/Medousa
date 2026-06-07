/** Lightweight markdown preview for M1 — not a full renderer. */
export function renderMarkdownPreview(source: string): string {
  const escaped = source
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");

  return escaped
    .split("\n")
    .map((line) => {
      if (/^#{1,6}\s/.test(line)) {
        const level = line.match(/^#+/)?.[0].length ?? 1;
        const text = line.replace(/^#{1,6}\s+/, "");
        return `<h${Math.min(level, 6)} class="font-semibold mt-4 mb-2">${inline(text)}</h${Math.min(level, 6)}>`;
      }
      if (/^[-*]\s/.test(line)) {
        return `<li class="ml-4 list-disc">${inline(line.slice(2))}</li>`;
      }
      if (line.trim() === "") {
        return "<br />";
      }
      return `<p class="mb-2 leading-relaxed">${inline(line)}</p>`;
    })
    .join("\n");
}

function inline(text: string): string {
  return text
    .replace(/\[\[([^\]]+)\]\]/g, '<span class="text-primary-400">[[$1]]</span>')
    .replace(/`([^`]+)`/g, '<code class="rounded bg-surface-800 px-1 text-sm">$1</code>')
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/\*([^*]+)\*/g, "<em>$1</em>");
}
