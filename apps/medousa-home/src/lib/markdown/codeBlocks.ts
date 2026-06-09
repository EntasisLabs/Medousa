import { haptic } from "$lib/haptics";

import { highlightCodeBlocks } from "./highlight";

async function copyCode(text: string): Promise<boolean> {
  if (typeof navigator === "undefined" || !navigator.clipboard?.writeText) {
    return false;
  }
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}

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
      fallback.textContent = "code";
      header.insertBefore(fallback, header.firstChild);
    }

    if (header.querySelector(".markdown-code-copy")) {
      block.dataset.copyHydrated = "1";
      return;
    }

    const button = document.createElement("button");
    button.type = "button";
    button.className = "markdown-code-copy";
    button.setAttribute("aria-label", "Copy code");
    button.title = "Copy code";
    button.textContent = "Copy";
    button.addEventListener("click", async () => {
      const ok = await copyCode(code.textContent ?? "");
      if (ok) {
        haptic("light");
        button.textContent = "Copied";
        button.classList.add("markdown-code-copy-done");
        window.setTimeout(() => {
          button.textContent = "Copy";
          button.classList.remove("markdown-code-copy-done");
        }, 1500);
        return;
      }
      button.textContent = "Failed";
      window.setTimeout(() => {
        button.textContent = "Copy";
      }, 1500);
    });
    header.appendChild(button);
    block.dataset.copyHydrated = "1";
  });
}

/** Highlight fenced blocks and wire copy controls. */
export async function hydrateCodeBlocks(root: HTMLElement): Promise<void> {
  if (typeof window === "undefined") return;
  await highlightCodeBlocks(root);
  attachCopyButtons(root);
}
