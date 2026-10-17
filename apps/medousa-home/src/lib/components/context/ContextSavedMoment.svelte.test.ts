/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from "vitest";
import { mount, unmount, flushSync } from "svelte";
import ContextSavedMoment from "./ContextSavedMoment.svelte";
import { contextShell } from "$lib/context/contextShell.svelte";
import { contextThreads } from "$lib/context/contextThreads.svelte";
const api = vi.hoisted(() => ({ openChat: vi.fn(), switchSession: vi.fn(), moment: vi.fn() }));
vi.mock("$lib/stores/chat.svelte", () => ({ chat: { sessions: [{ session_id: "real-chat" }], switchSession: api.switchSession } }));
vi.mock("$lib/stores/shellTabs.svelte", () => ({ shellTabs: { openChat: api.openChat } }));
vi.mock("$lib/stores/lmeWorkspace.svelte", () => ({ lmeWorkspace: { openNote: vi.fn() } }));
vi.mock("$lib/daemon", () => ({ listLocusNodes: vi.fn(), getLocusNode: api.moment, listVaultNotes: vi.fn() }));
const node = { sync_key: "one", session_id: "coder:synthetic", tier: "raw", timestamp: "2026-10-17T00:00:00Z", context_summary: "verification: Check baseline", rho: 0, kappa: 0, psi: 0 };
const fields = [
  { name: "memory_kind", value: "verification" }, { name: "summary", value: "Check baseline" },
  { name: "details", value: "tool=cognition_coder_shell_run; outcome=completed" },
  { name: "author_session", value: "real-chat" }, { name: "repo_id", value: "/workspace/Medousa/.git" },
  { name: "branch", value: "feature/context" }, { name: "evidence_refs", value: ["engineering:call:123"] },
  { name: "observations", value: { fact: "A saved observation", remaining: ["Check test output"] } },
  { name: "vibe_signature", value: "Injected posture metadata" },
];
let component: ReturnType<typeof mount>;
afterEach(async () => { if (component) await unmount(component); document.body.replaceChildren(); contextThreads.resetForWorkshopSwitch(); contextShell.selectMapNode(null); vi.clearAllMocks(); });
it("renders structured content and provenance, resolves real source and memory links, and keeps raw collapsed", async () => {
  contextThreads.nodes = [{ ...node, sync_key: "previous", context_summary: "decision: Keep evidence" }];
  api.moment.mockResolvedValue({ node: contextThreads.nodes[0], raw: "Saved connected memory" });
  component = mount(ContextSavedMoment, { target: document.body, props: { detail: { node, raw: `⊕⟨ ⏣0{ compression_depth: 1, semantic_links: [{ rel: "derived_from", target: "previous" }, { rel: "supersedes", target: "unloaded-record" }] } ⟩\n◈⟨ ⏣0{ ${fields.map((field) => `${field.name}(.99): ${JSON.stringify(field.value)}`).join(", ")}, marker(.99): "RAW_STTP_SENTINEL" } ⟩` }, sessionLabels: { "real-chat": "Baseline work" } } }); flushSync();
  expect(document.querySelector("h2")?.textContent).toBe("Check baseline");
  expect(document.querySelector('[aria-label="Recorded activity"]')?.textContent).toContain("Shell tool execution completed");
  expect(document.querySelector('[aria-label="Recorded activity"]')?.textContent).toContain("A check verdict is not included");
  expect(document.querySelector('[aria-label="Observations"]')?.textContent).toContain("A saved observation");
  expect(document.querySelector('[aria-label="Observations"]')?.textContent).toContain("Check test output");
  expect([...document.querySelectorAll("h3")].some((heading) => heading.textContent === "Vibe signature")).toBe(false);
  expect(document.querySelector('[aria-label="Working context"]')?.textContent).toContain("Medousa");
  expect(document.querySelector('[aria-label="Working context"]')?.textContent).toContain("feature/context");
  expect(document.querySelector('[aria-label="Evidence and references"]')?.textContent).toContain("Tool execution reference");
  const record = [...document.querySelectorAll("details")].find((entry) => entry.querySelector("summary")?.textContent === "Record details")!;
  expect(record.open).toBe(false); expect(record.textContent).toContain("RAW_STTP_SENTINEL");
  const buttons = [...document.querySelectorAll<HTMLButtonElement>("button")];
  buttons.find((button) => button.textContent?.includes("Open source session"))!.click(); flushSync(); await Promise.resolve();
  expect(api.openChat).toHaveBeenCalledWith("real-chat", { activate: true }); expect(api.switchSession).toHaveBeenCalledWith("real-chat");
  buttons.find((button) => button.textContent?.includes("Keep evidence"))!.click(); flushSync();
  expect(contextShell.selectedMapNodeId).toBe("thread:previous");
  expect(api.moment).toHaveBeenCalledWith("previous");
  expect(document.querySelector('[aria-label="Connected memory"]')?.textContent).toContain("outside the loaded list");
});
