function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function plainTextFromHover(el: HTMLElement): string {
  return (el.innerText || el.textContent || "").replace(/\u00a0/g, " ").trim();
}

const ROW_RE = /^(args|returns|return|effect|throws|see)\s*:\s*(.*)$/i;

/** Restructure flat LSP hover dumps into title / blurb / labeled rows. */
export function enhanceGraphemeHoverElement(el: HTMLElement) {
  if (el.dataset.graphemeHover === "1") return;
  el.dataset.graphemeHover = "1";

  const text = plainTextFromHover(el);
  if (!text) return;

  const lines = text
    .split(/\n+/)
    .map((line) => line.trim())
    .filter(Boolean);
  if (lines.length < 2) return;

  const title = lines[0]!;
  let desc = "";
  const rows: Array<{ label: string; value: string }> = [];

  for (let i = 1; i < lines.length; i += 1) {
    const line = lines[i]!;
    const match = ROW_RE.exec(line);
    if (match) {
      rows.push({
        label: match[1]!.toLowerCase() === "return" ? "returns" : match[1]!.toLowerCase(),
        value: match[2] ?? "",
      });
      continue;
    }
    if (rows.length > 0) {
      const last = rows[rows.length - 1]!;
      last.value = last.value ? `${last.value} ${line}` : line;
      continue;
    }
    desc = desc ? `${desc} ${line}` : line;
  }

  if (rows.length === 0 && !desc) return;

  const parts = [
    `<div class="grapheme-hover-title">${escapeHtml(title)}</div>`,
  ];
  if (desc) {
    parts.push(`<div class="grapheme-hover-blurb">${escapeHtml(desc)}</div>`);
  }
  for (const row of rows) {
    parts.push(
      `<div class="grapheme-hover-row"><span class="grapheme-hover-label">${escapeHtml(row.label)}</span><span class="grapheme-hover-value">${escapeHtml(row.value)}</span></div>`,
    );
  }

  el.classList.add("grapheme-hover");
  el.innerHTML = parts.join("");
}

export function observeGraphemeHovers(root: HTMLElement): () => void {
  const enhanceAll = () => {
    root
      .querySelectorAll<HTMLElement>(".cm-lsp-hover-tooltip:not([data-grapheme-hover='1'])")
      .forEach(enhanceGraphemeHoverElement);
    // Tooltips often mount on document.body
    document
      .querySelectorAll<HTMLElement>(".cm-lsp-hover-tooltip:not([data-grapheme-hover='1'])")
      .forEach(enhanceGraphemeHoverElement);
  };

  enhanceAll();
  const observer = new MutationObserver(enhanceAll);
  observer.observe(document.body, { childList: true, subtree: true });
  return () => observer.disconnect();
}
