/** @vitest-environment happy-dom */
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { mount, unmount, flushSync, tick } from "svelte";
import CreatePanel from "./CreatePanel.svelte";
import { applySceneOperations } from "$lib/create/creativeScene";
import type {
  CreativeDocument,
  CreativeOperation,
} from "$lib/types/generated/daemon_api";
const { request, openCreative } = vi.hoisted(() => ({
  request: vi.fn(),
  openCreative: vi.fn(),
}));
vi.mock("$lib/daemon/contractClient", () => ({ daemonUnary: request }));
vi.mock("$lib/stores/shellTabs.svelte", () => ({
  shellTabs: {
    tabs: [],
    activeTab: null,
    patchTitle: vi.fn(),
    openSurface: vi.fn(),
    openCreative,
    openChat: vi.fn(),
  },
}));
let mounted: ReturnType<typeof mount>[] = [];
let doc: CreativeDocument;
let transactions: unknown[] = [];
let fail = false;
async function settle() {
  await tick();
  await new Promise((r) => setTimeout(r, 0));
  flushSync();
  await tick();
}
function button(label: string, root: ParentNode = document) {
  const result = [...root.querySelectorAll<HTMLButtonElement>("button")].find(
    (b) =>
      b.getAttribute("aria-label") === label || b.textContent?.trim() === label,
  );
  expect(result, `button ${label}`).toBeTruthy();
  return result!;
}
function render() {
  const target = document.createElement("div");
  document.body.append(target);
  mounted.push(
    mount(CreatePanel, {
      target,
      props: { documentId: doc.id, workshopId: "ui-test" },
    }),
  );
  flushSync();
  return target;
}
beforeEach(() => {
  transactions = [];
  fail = false;
  doc = {
    id: crypto.randomUUID(),
    title: "Release",
    revision: 0,
    pinned: false,
    created_at: "now",
    updated_at: "now",
    assets: {},
    scene: {
      schema: "medousa-create",
      version: 3,
      background: "transparent",
      nodes: [
        {
          id: "frame",
          parent_id: null,
          transform: { x: 0, y: 0, width: 1200, height: 920, rotation: 0 },
          opacity: 1,
          visible: true,
          locked: false,
          content: { kind: "frame", name: "Frame", background: "#fff" },
        },
        {
          id: "title",
          parent_id: "frame",
          transform: { x: 60, y: 80, width: 600, height: 100, rotation: 0 },
          opacity: 1,
          visible: true,
          locked: false,
          content: {
            kind: "text",
            text: "Headline",
            font_family: "sans-serif",
            font_size: 64,
            color: "#000",
          },
        },
      ],
    },
  };
  request.mockImplementation(
    async (op: string, _params: unknown, body: unknown) => {
      if (op === "create.projects.by_id.jobs.get") return { jobs: [] };
      if (op === "create.projects.get")
        return { projects: [], next_cursor: null };
      if (op === "create.projects.by_id.transactions.post") {
        transactions.push(structuredClone(body));
        if (fail) {
          fail = false;
          throw new Error("Connection lost");
        }
        doc = {
          ...doc,
          revision: doc.revision + 1,
          scene: applySceneOperations(
            doc.scene,
            (body as { operations: CreativeOperation[] }).operations,
          ),
        };
      }
      return {
        document: structuredClone(doc),
        applied_revision: doc.revision,
        duplicate: false,
        can_undo: doc.revision > 0,
        can_redo: false,
      };
    },
  );
});
afterEach(async () => {
  for (const component of mounted) await unmount(component);
  mounted = [];
  document.body.replaceChildren();
  request.mockClear();
  openCreative.mockClear();
});
it("shares edits across split views through one durable session", async () => {
  const first = render(),
    second = render();
  await settle();
  await settle();
  button("Toggle inspector", first).click();
  flushSync();
  button("Headline", first).click();
  flushSync();
  const text = first.querySelector<HTMLTextAreaElement>(
    '[aria-label="Text content"]',
  )!;
  text.value = "A shared headline";
  text.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  expect(transactions).toHaveLength(1);
  expect(
    first.querySelector('svg[aria-label="Editable creative project"]')
      ?.textContent,
  ).toContain("A shared headline");
  expect(
    second.querySelector('svg[aria-label="Editable creative project"]')
      ?.textContent,
  ).toContain("A shared headline");
  expect(button("Undo", first).disabled).toBe(false);
  expect(button("Undo", second).disabled).toBe(false);
});
it("keeps a failed edit visible after tab remount and retries the exact request", async () => {
  const first = render();
  await settle();
  await settle();
  button("Toggle inspector", first).click();
  flushSync();
  button("Headline", first).click();
  flushSync();
  const text = first.querySelector<HTMLTextAreaElement>(
    '[aria-label="Text content"]',
  )!;
  fail = true;
  text.value = "Retained draft";
  text.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  expect(first.textContent).toContain("Connection lost");
  const old = mounted.pop()!;
  await unmount(old);
  first.remove();
  const second = render();
  await settle();
  expect(
    second.querySelector('svg[aria-label="Editable creative project"]')
      ?.textContent,
  ).toContain("Retained draft");
  expect(second.textContent).toContain("An edit needs your attention");
  button("Retry", second).click();
  await settle();
  expect(transactions).toHaveLength(2);
  expect(transactions[0]).toEqual(transactions[1]);
  expect(second.textContent).toContain("Saved");
});
it("uses the scoped durable-job API and previews before explicit acceptance", async () => {
  const root = render();
  await settle();
  await settle();
  button("Toggle inspector", root).click();
  flushSync();
  button("Headline", root).click();
  flushSync();
  button("Close inspector", root).click();
  flushSync();
  let proposed: import("$lib/types/generated/daemon_api").CreativeJob;
  request.mockImplementation(
    async (op: string, _params: unknown, body: unknown) => {
      if (op === "create.projects.by_id.jobs.post") {
        const changed = structuredClone(doc.scene.nodes[1]);
        if (changed.content.kind === "text")
          changed.content.text = "Previewed title";
        proposed = {
          document_id: doc.id,
          request:
            body as import("$lib/types/generated/daemon_api").CreativeJobRequest,
          status: "ready",
          session_id: "test_session",
          turn_id: "test_turn",
          assets: [],
          created_at: "now",
          updated_at: "now",
          context: {
            document_id: doc.id,
            revision: 0,
            selected_ids: ["title"],
            nodes: [],
            assets: [],
            fingerprint: "test",
          },
          result: {
            id: "result",
            expected_revision: 0,
            selected_ids: ["title"],
            label: "A new direction",
            operations: [{ op: "put_node", node: changed }],
          },
        };
        return proposed;
      }
      if (op === "create.projects.by_id.jobs.by_job_id.decision.post") {
        expect(body).toEqual({ accept: true, expected_revision: 0 });
        doc = {
          ...doc,
          revision: 1,
          scene: applySceneOperations(doc.scene, proposed.result!.operations),
        };
        return { ...proposed, status: "accepted", applied_revision: 1 };
      }
      return {
        document: structuredClone(doc),
        applied_revision: doc.revision,
        duplicate: false,
        can_undo: doc.revision > 0,
        can_redo: false,
      };
    },
  );
  button("Ask Medousa", root).click();
  flushSync();
  const input = root.querySelector<HTMLInputElement>(
    'input[aria-label="Ask about selected work"]',
  )!;
  input.value = "Change this title";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  root
    .querySelector("form")!
    .dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  await settle();
  const admitted = request.mock.calls.find(
    (call) => call[0] === "create.projects.by_id.jobs.post",
  )!;
  expect(admitted[2]).toMatchObject({
    expected_revision: 0,
    selected_ids: ["title"],
    prompt: "Change this title",
  });
  expect(admitted[3]).toBeNull();
  expect(admitted[5]).toBe("ui-test");
  button("Preview", root).click();
  flushSync();
  expect(
    root.querySelector('svg[aria-label="Editable creative project"]')
      ?.textContent,
  ).toContain("Previewed title");
  expect(doc.revision).toBe(0);
  expect(transactions).toEqual([]);
  button("Use this", root).click();
  await settle();
  await settle();
  expect(doc.revision).toBe(1);
  expect(root.textContent).toContain("Applied to the canvas");
  expect(button("Undo", root).disabled).toBe(false);
});
it("shares direct text and swatch edits through the existing transaction session", async () => {
  const first = render(),
    second = render();
  await settle();
  await settle();
  first
    .querySelector('[data-node-id="title"] text')!
    .dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
  await tick();
  flushSync();
  const input = first.querySelector<HTMLTextAreaElement>(
    '[aria-label="Canvas text"]',
  )!;
  expect(input.value).toBe("Headline");
  input.value = "A direct headline";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  button("Blue", first).click();
  flushSync();
  expect(transactions).toEqual([]);
  input.dispatchEvent(
    new KeyboardEvent("keydown", { key: "Escape", bubbles: true }),
  );
  await settle();
  expect(transactions).toHaveLength(1);
  expect(doc.scene.nodes[1].content).toMatchObject({
    text: "A direct headline",
    color: "#4e75d9",
  });
  expect(
    second.querySelector('svg[aria-label="Editable creative project"]')
      ?.textContent,
  ).toContain("A direct headline");
});
it("keeps the Ask draft when its floating panel is collapsed", async () => {
  const root = render();
  await settle();
  await settle();
  button("Ask Medousa", root).click();
  flushSync();
  const input = root.querySelector<HTMLInputElement>(
    'input[aria-label="Ask about selected work"]',
  )!;
  input.value = "Keep this direction";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  button("Close Ask Medousa", root).click();
  flushSync();
  expect(
    root.querySelector('input[aria-label="Ask about selected work"]'),
  ).toBeNull();
  button("Ask Medousa", root).click();
  flushSync();
  expect(
    root.querySelector<HTMLInputElement>(
      'input[aria-label="Ask about selected work"]',
    )?.value,
  ).toBe("Keep this direction");
  expect(transactions).toEqual([]);
});
