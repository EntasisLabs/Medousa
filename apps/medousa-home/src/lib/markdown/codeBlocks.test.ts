/** @vitest-environment happy-dom */
import { afterEach, beforeEach, expect, it, vi } from "vitest";
const { highlight, writeText } = vi.hoisted(() => ({ highlight: vi.fn(), writeText: vi.fn() }));
vi.mock("./highlight", () => ({ highlightCodeBlocks: highlight }));
vi.mock("$lib/haptics", () => ({ haptic: vi.fn() }));
// Happy DOM drops the outer element during DOMPurify parsing. Exercise DOM
// controls here; the real sanitization path is verified in browser QA.
vi.mock("dompurify", () => ({ default: { sanitize: (html: string) => html } }));
import { hydrateCodeBlocks } from "./codeBlocks";
import { renderMarkdown } from "./render";

beforeEach(() => {
  highlight.mockReset().mockResolvedValue(undefined);
  writeText.mockReset().mockResolvedValue(undefined);
  vi.stubGlobal("navigator", { clipboard: { writeText } });
});
afterEach(() => { vi.unstubAllGlobals(); vi.useRealTimers(); document.body.replaceChildren(); });

function render(source: string, language = "ts") {
  const root = document.createElement("div");
  root.innerHTML = renderMarkdown(`\`\`\`${language}\n${source}\n\`\`\``);
  document.body.appendChild(root);
  return root;
}

it("renders Copy immediately and wires it while highlighting is still pending", async () => {
  let finish!: () => void;
  highlight.mockReturnValueOnce(new Promise<void>((resolve) => { finish = resolve; }));
  const root = render('const message = "hello";');
  const button = root.querySelector<HTMLButtonElement>(".markdown-code-copy")!;
  expect(button.textContent).toBe("Copy");
  expect(button.querySelector("svg")).not.toBeNull();
  expect(root.querySelector(".markdown-code-lang")?.textContent).toBe("TypeScript");
  const pending = hydrateCodeBlocks(root);
  button.click();
  expect(writeText).toHaveBeenCalledWith('const message = "hello";');
  finish();
  await pending;
});

it("keeps copy working with plain code when highlighting fails", async () => {
  highlight.mockRejectedValueOnce(new Error("highlighter unavailable"));
  const root = render('printf "ready"', "sh");
  await expect(hydrateCodeBlocks(root)).resolves.toBeUndefined();
  root.querySelector<HTMLButtonElement>(".markdown-code-copy")!.click();
  expect(writeText).toHaveBeenCalledWith('printf "ready"');
  expect(root.querySelector(".markdown-code-lang")?.textContent).toBe("Shell");
});

it("expands long snippets without trimming the code and copies the collapsed portion too", async () => {
  const source = Array.from({ length: 20 }, (_, index) => `const line${index} = ${index};`).join("\n");
  const root = render(source);
  const block = root.querySelector(".markdown-code-block")!;
  const expand = root.querySelector<HTMLButtonElement>(".markdown-code-expand")!;
  expect(block.classList.contains("markdown-code-collapsed")).toBe(true);
  expect(root.querySelector("code")?.textContent).toBe(source);
  await hydrateCodeBlocks(root);
  await hydrateCodeBlocks(root);
  root.querySelector<HTMLButtonElement>(".markdown-code-copy")!.click();
  expect(writeText).toHaveBeenCalledExactlyOnceWith(source);
  expand.click();
  expect(block.classList.contains("markdown-code-collapsed")).toBe(false);
  expect(expand.getAttribute("aria-expanded")).toBe("true");
  expect(expand.textContent).toBe("Show less");
  expand.click();
  expect(block.classList.contains("markdown-code-collapsed")).toBe(true);
  expect(root.querySelector("code")?.textContent).toBe(source);
});

it("reports clipboard failure without claiming success", async () => {
  writeText.mockRejectedValueOnce(new Error("denied"));
  const root = render("hello");
  await hydrateCodeBlocks(root);
  root.querySelector<HTMLButtonElement>(".markdown-code-copy")!.click();
  await vi.waitFor(() => expect(root.querySelector(".markdown-code-copy")?.textContent).toBe("Failed"));
});

it("shows success briefly and restarts the feedback timer on another copy", async () => {
  vi.useFakeTimers();
  const root = render("hello");
  await hydrateCodeBlocks(root);
  const button = root.querySelector<HTMLButtonElement>(".markdown-code-copy")!;
  button.click();
  await vi.advanceTimersByTimeAsync(0);
  expect(button.textContent).toBe("Copied");
  expect(button.classList.contains("markdown-code-copy-done")).toBe(true);
  await vi.advanceTimersByTimeAsync(1000);
  button.click();
  await vi.advanceTimersByTimeAsync(1000);
  expect(button.textContent).toBe("Copied");
  await vi.advanceTimersByTimeAsync(500);
  expect(button.textContent).toBe("Copy");
  expect(button.classList.contains("markdown-code-copy-done")).toBe(false);
  expect(writeText).toHaveBeenCalledTimes(2);
});

it("toggles wrapping without changing the source copied to the clipboard", async () => {
  const source = `const description = "${"a".repeat(1500)}";`;
  const root = render(source);
  await hydrateCodeBlocks(root);
  expect(root.querySelector(".markdown-code-expand")).toBeNull();
  const wrap = root.querySelector<HTMLButtonElement>(".markdown-code-wrap")!;
  wrap.click();
  expect(wrap.getAttribute("aria-pressed")).toBe("true");
  expect(root.querySelector(".markdown-code-wrapped")).not.toBeNull();
  root.querySelector<HTMLButtonElement>(".markdown-code-copy")!.click();
  expect(writeText).toHaveBeenCalledExactlyOnceWith(source);
  wrap.click();
  expect(wrap.getAttribute("aria-pressed")).toBe("false");
  expect(root.querySelector(".markdown-code-wrapped")).toBeNull();
});
