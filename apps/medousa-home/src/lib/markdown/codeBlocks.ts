import { haptic } from "$lib/haptics";

import { highlightCodeBlocks } from "./highlight";
import { codeCopyContent, copyCodeText } from "./codeBlockPresentation";

function attachCopyButtons(root: HTMLElement): void {
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
    let resetTimer: number | undefined;
    button.addEventListener("click", async () => {
      const ok = await copyCodeText(code.textContent ?? "");
      if (resetTimer !== undefined) window.clearTimeout(resetTimer);
      button.innerHTML = codeCopyContent(ok ? "copied" : "failed");
      button.classList.toggle("markdown-code-copy-done", ok);
      button.title = ok ? "Code copied" : "Could not copy code";
      if (ok) {
        haptic("light");
      }
      resetTimer = window.setTimeout(() => {
        button.innerHTML = codeCopyContent();
        button.classList.remove("markdown-code-copy-done");
        button.title = "Copy code";
        resetTimer = undefined;
      }, 1500);
    });
    if (!button.parentElement) header.appendChild(button);
    const wrap = block.querySelector<HTMLButtonElement>(".markdown-code-wrap");
    wrap?.addEventListener("click", () => {
      const wrapped = block.classList.toggle("markdown-code-wrapped");
      wrap.setAttribute("aria-pressed", String(wrapped));
    });
    const expand = block.querySelector<HTMLButtonElement>(".markdown-code-expand");
    expand?.addEventListener("click", () => {
      const collapsed = block.classList.toggle("markdown-code-collapsed");
      expand.setAttribute("aria-expanded", String(!collapsed));
      expand.textContent = collapsed ? "Show all" : "Show less";
    });
    block.dataset.copyHydrated = "1";
  });
}

/** Highlight fenced blocks and wire copy controls. */
export async function hydrateCodeBlocks(root: HTMLElement): Promise<void> {
  if (typeof window === "undefined") return;
  attachCopyButtons(root);
  try {
    await highlightCodeBlocks(root);
  } catch {
    // Highlighting is optional; plain code and its controls remain usable.
  }
}
