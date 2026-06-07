import { wikilinkLabel } from "$lib/utils/formatVault";

/** Lightweight markdown preview — human wikilinks when title map provided. */
export function renderMarkdownPreview(
  source: string,
  titleByPath?: Map<string, string>,
): string {
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
        return `<h${Math.min(level, 6)} class="font-semibold mt-4 mb-2">${inline(text, titleByPath)}</h${Math.min(level, 6)}>`;
      }
      if (/^[-*]\s/.test(line)) {
        return `<li class="ml-4 list-disc">${inline(line.slice(2), titleByPath)}</li>`;
      }
      if (line.trim() === "") {
        return "<br />";
      }
      return `<p class="mb-2 leading-relaxed">${inline(line, titleByPath)}</p>`;
    })
    .join("\n");
}

function inline(text: string, titleByPath?: Map<string, string>): string {
  return text
    .replace(/\[\[([^\]]+)\]\]/g, (_match, target: string) => {
      const label = wikilinkLabel(target, titleByPath);
      return `<span class="text-primary-400">${label}</span>`;
    })
    .replace(/`([^`]+)`/g, '<code class="rounded bg-surface-800 px-1 text-sm">$1</code>')
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/\*([^*]+)\*/g, "<em>$1</em>");
}
