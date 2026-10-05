/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from "vitest";
import { flushSync, mount, tick, unmount } from "svelte";
import { fromStore, writable } from "svelte/store";
import type { ChatMessage } from "$lib/types/chat";
import { chatMessageToScene } from "./messageToScene";
import { registerComponent } from "$lib/liquid/render/componentRegistry";
import SceneRenderer from "$lib/liquid/render/SceneRenderer.svelte";
import Document from "$lib/liquid/archetypes/organisms/document/Document.svelte";
import Prose from "$lib/liquid/archetypes/atoms/prose/Prose.svelte";

vi.mock("$lib/haptics", () => ({ haptic: vi.fn() }));
// Happy DOM drops outer wrappers when DOMPurify parses the fragment.
vi.mock("dompurify", () => ({ default: { sanitize: (html: string) => html } }));
registerComponent("document", Document);
registerComponent("prose", Prose);
let component: ReturnType<typeof mount> | undefined;
afterEach(async () => {
  if (component) await unmount(component);
  component = undefined;
  vi.unstubAllGlobals();
  document.body.replaceChildren();
});

it.each(["legacy", "segments"])("copies completed %s chat history and updated content through its real scene renderer", async (mode) => {
  const writeText = vi.fn().mockResolvedValue(undefined);
  vi.stubGlobal("navigator", { clipboard: { writeText } });
  const source = Array.from({ length: 15 }, (_, i) => `  echo line${i}`).join("\n");
  const message = (code: string): ChatMessage => ({
    id: "history", role: "assistant", content: `Run this:\n\n\`\`\`sh\n${code}\n\`\`\``, streaming: false,
    ...(mode === "segments" ? { segments: [{ kind: "text", segmentId: "answer", modelRound: 1, committed: true, markdown: `\`\`\`sh\n${code}\n\`\`\`` }] } : {}),
  });
  const store = writable(message(source));
  const state = fromStore(store);
  component = mount(SceneRenderer, {
    target: document.body,
    props: { get node() { return chatMessageToScene(state.current); } },
  });
  flushSync();
  await vi.waitFor(() => expect(document.querySelector(".liquid-prose code")?.textContent).toBe(source));
  const copy = document.querySelector<HTMLButtonElement>(".markdown-code-copy")!;
  // Clicking the icon must work as well as clicking the button's text.
  copy.querySelector("svg")!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  await vi.waitFor(() => expect(writeText).toHaveBeenCalledExactlyOnceWith(source));
  await vi.waitFor(() => expect(copy.textContent).toBe("Copied"));
  document.querySelector<HTMLButtonElement>(".markdown-code-wrap")!.click();
  expect(document.querySelector(".markdown-code-wrapped")).not.toBeNull();
  document.querySelector<HTMLButtonElement>(".markdown-code-expand")!.click();
  expect(document.querySelector(".markdown-code-collapsed")).toBeNull();

  store.set(message("  echo restored history"));
  await tick();
  document.querySelector<HTMLButtonElement>(".markdown-code-copy")!.click();
  await vi.waitFor(() => expect(writeText).toHaveBeenLastCalledWith("  echo restored history"));
  expect(writeText).toHaveBeenCalledTimes(2);
});
