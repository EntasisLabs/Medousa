/** Give authored HTML disclosures the same presentation as Liquid accordions.
 * The input has already passed through the shared Markdown sanitizer. Template
 * content stays inert; only known wrappers/classes are added, never new source. */
export function presentDocumentMarkdown(html: string): string {
  if (typeof document === "undefined") return html;
  const template = document.createElement("template");
  template.innerHTML = html;
  const details = [...template.content.querySelectorAll<HTMLDetailsElement>("details")];
  const hasOpenSection = details.some(detail => detail.hasAttribute("open"));
  let first = true;
  for (const detail of details) {
    const summary = [...detail.children].find(child => child.tagName === "SUMMARY");
    if (!summary) continue;
    detail.classList.add("liquid-accordion-item");
    summary.classList.add("liquid-accordion-trigger");
    const label = document.createElement("span");
    label.className = "liquid-accordion-label";
    label.append(...summary.childNodes);
    const chevron = document.createElement("span");
    chevron.className = "liquid-accordion-chevron";
    chevron.setAttribute("aria-hidden", "true");
    chevron.textContent = "▾";
    summary.replaceChildren(label, chevron);
    const body = document.createElement("div");
    body.className = "liquid-accordion-panel liquid-accordion-body";
    body.append(...[...detail.childNodes].filter(child => child !== summary));
    detail.replaceChildren(summary, body);
    if (first && !hasOpenSection) detail.setAttribute("open", "");
    first = false;
  }
  // Group only adjacent disclosures. Prose between them keeps its exact place.
  for (const detail of details) {
    if (detail.parentElement?.classList.contains("liquid-accordion-list") || !detail.classList.contains("liquid-accordion-item")) continue;
    const group: HTMLDetailsElement[] = [detail];
    let next = detail.nextSibling;
    while (next) {
      if (next.nodeType === Node.TEXT_NODE && !next.textContent?.trim()) { next = next.nextSibling; continue; }
      if (!(next instanceof HTMLDetailsElement) || !next.classList.contains("liquid-accordion-item")) break;
      group.push(next); next = next.nextSibling;
    }
    const wrapper = document.createElement("div");
    wrapper.className = "liquid-accordion";
    const list = document.createElement("div");
    list.className = "liquid-accordion-list";
    detail.before(wrapper);
    list.append(...group);
    wrapper.append(list);
  }
  return template.innerHTML;
}
