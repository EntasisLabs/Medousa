import { haptic } from "$lib/haptics";

import { highlightCodeBlocks } from "./highlight";
import { codeCopyContent, copyCodeText } from "./codeBlockPresentation";

const resetTimers = new WeakMap<HTMLButtonElement, number>();
const handledClicks = new WeakSet<MouseEvent>();

/** Controls for parse-only Markdown surfaces; survive replacement of raw HTML. */
export function codeBlockControls(root: HTMLElement): { destroy(): void } {
  const handleClick = (event: MouseEvent) => {
    const block = (event.target as Element | null)?.closest<HTMLElement>(".markdown-code-block");
    // Hydrated Markdown and Liquid snippets already own their controls.
    if (block?.dataset.copyHydrated === "1") return;
    handleCodeBlockControlClick(event);
  };
  root.addEventListener("click", handleClick);
  return { destroy: () => root.removeEventListener("click", handleClick) };
}

/** Delegated controls also work on a streaming tail whose HTML is replaced. */
export function handleCodeBlockControlClick(event: MouseEvent): void {
  if (handledClicks.has(event)) return;
  const button = (event.target as Element | null)?.closest<HTMLButtonElement>(
    ".markdown-code-copy, .markdown-code-wrap, .markdown-code-expand",
  );
  const block = button?.closest<HTMLElement>(".markdown-code-block");
  const code = block?.querySelector("code");
  if (!button || !block || !code) return;
  handledClicks.add(event);
  if (button.classList.contains("markdown-code-copy")) {
    void copyCode(button, code.textContent ?? "");
  } else if (button.classList.contains("markdown-code-wrap")) {
    const wrapped = block.classList.toggle("markdown-code-wrapped");
    block.dataset.codeWrapped = String(wrapped);
    button.setAttribute("aria-pressed", String(wrapped));
  } else {
    const collapsed = block.classList.toggle("markdown-code-collapsed");
    block.dataset.codeExpanded = String(!collapsed);
    button.setAttribute("aria-expanded", String(!collapsed));
    button.textContent = collapsed ? "Show all" : "Show less";
  }
}

export interface CodeBlockState {
  source: string;
  wrapped?: string;
  expanded?: string;
  scrollTop: number;
  scrollLeft: number;
}

/** Code order remains stable in an append-only Markdown stream. */
export function captureCodeBlockState(root: HTMLElement): CodeBlockState[] {
  return [...root.querySelectorAll<HTMLElement>(".markdown-code-block")].map((block) => {
    const pre = block.querySelector("pre");
    return {
      source: block.querySelector("code")?.textContent ?? "",
      wrapped: block.dataset.codeWrapped,
      expanded: block.dataset.codeExpanded,
      scrollTop: pre?.scrollTop ?? 0,
      scrollLeft: pre?.scrollLeft ?? 0,
    };
  });
}

export function restoreCodeBlockState(root: HTMLElement, states: CodeBlockState[]): void {
  root.querySelectorAll<HTMLElement>(".markdown-code-block").forEach((block, index) => {
    const state = states[index];
    const source = block.querySelector("code")?.textContent ?? "";
    // Canonical replacements must not inherit choices from unrelated code.
    if (!state || !source.startsWith(state.source)) return;
    if (state.wrapped !== undefined) {
      block.dataset.codeWrapped = state.wrapped;
      block.classList.toggle("markdown-code-wrapped", state.wrapped === "true");
      block.querySelector(".markdown-code-wrap")?.setAttribute("aria-pressed", state.wrapped);
    }
    if (state.expanded !== undefined) {
      block.dataset.codeExpanded = state.expanded;
      block.classList.toggle("markdown-code-collapsed", state.expanded !== "true");
      const expand = block.querySelector(".markdown-code-expand");
      if (expand) {
        expand.setAttribute("aria-expanded", state.expanded);
        expand.textContent = state.expanded === "true" ? "Show less" : "Show all";
      }
    }
    const pre = block.querySelector("pre");
    if (pre) {
      pre.scrollTop = state.scrollTop;
      pre.scrollLeft = state.scrollLeft;
    }
  });
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
