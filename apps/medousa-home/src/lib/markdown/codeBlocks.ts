import { haptic } from "$lib/haptics";

import { highlightCodeBlocks } from "./highlight";
import { codeCopyContent, copyCodeText } from "./codeBlockPresentation";

const resetTimers = new WeakMap<HTMLButtonElement, number>();

/** Delegated controls also work on a streaming tail whose HTML is replaced. */
export function handleCodeBlockControlClick(event: MouseEvent): void {
  const button = (event.target as Element | null)?.closest<HTMLButtonElement>(
    ".markdown-code-copy, .markdown-code-wrap, .markdown-code-expand",
  );
  const block = button?.closest<HTMLElement>(".markdown-code-block");
  const code = block?.querySelector("code");
  if (!button || !block || !code) return;
  if (button.classList.contains("markdown-code-copy")) {
    void copyCode(button, code.textContent ?? "");
  } else if (button.classList.contains("markdown-code-wrap")) {
    button.setAttribute("aria-pressed", String(block.classList.toggle("markdown-code-wrapped")));
  } else {
    const collapsed = block.classList.toggle("markdown-code-collapsed");
    button.setAttribute("aria-expanded", String(!collapsed));
    button.textContent = collapsed ? "Show all" : "Show less";
  }
}

async function copyCode(button: HTMLButtonElement, source: string): Promise<void> {
  const ok = await copyCodeText(source);
  const resetTimer = resetTimers.get(button);
  if (resetTimer !== undefined) window.clearTimeout(resetTimer);
  button.innerHTML = codeCopyContent(ok ? "copied" : "failed");
  button.classList.toggle("markdown-code-copy-done", ok);
  button.title = ok ? "Code copied" : "Could not copy code";
  if (ok) haptic("light");
  resetTimers.set(button, window.setTimeout(() => {
    button.innerHTML = codeCopyContent();
    button.classList.remove("markdown-code-copy-done");
    button.title = "Copy code";
    resetTimers.delete(button);
  }, 1500));
}

function attachCodeBlockControls(root: HTMLElement): void {
  root.querySelectorAll<HTMLElement>(".markdown-code-block").forEach((block) => {
    if (block.dataset.copyHydrated === "1") return;

    const code = block.querySelector("code");
    if (!code) return;

    let header = block.querySelector<HTMLElement>(".markdown-code-header");
    if (!header) {
      header = document.createElement("div");
      header.className = "markdown-code-header";
      block.insertBefore(header, block.firstChild);
    }

    if (!header.querySelector(".markdown-code-lang")) {
      const fallback = document.createElement("span");
      fallback.className = "markdown-code-lang markdown-code-lang-muted";
      fallback.textContent = "Code";
      header.insertBefore(fallback, header.firstChild);
    }

    const button = header.querySelector<HTMLButtonElement>(".markdown-code-copy") ?? document.createElement("button");
    button.type = "button";
    button.className = "markdown-code-copy";
    button.setAttribute("aria-label", "Copy code");
    button.title = "Copy code";
    button.innerHTML = codeCopyContent();
    button.addEventListener("click", handleCodeBlockControlClick);
    if (!button.parentElement) header.appendChild(button);
    const wrap = block.querySelector<HTMLButtonElement>(".markdown-code-wrap");
    wrap?.addEventListener("click", handleCodeBlockControlClick);
    const expand = block.querySelector<HTMLButtonElement>(".markdown-code-expand");
    expand?.addEventListener("click", handleCodeBlockControlClick);
    block.dataset.copyHydrated = "1";
  });
}

/** Highlight fenced blocks and wire copy controls. */
export async function hydrateCodeBlocks(root: HTMLElement): Promise<void> {
  if (typeof window === "undefined") return;
  attachCodeBlockControls(root);
  try {
    await highlightCodeBlocks(root);
  } catch {
    // Highlighting is optional; plain code and its controls remain usable.
  }
}
