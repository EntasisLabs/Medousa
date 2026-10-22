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
  localStorage.clear();
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

it("keeps a finished gesture visible during delayed autosave without refreshing the rail or replacing the canvas", async () => {
  const root = render();
  await settle(); await settle();
  button("Toggle inspector", root).click(); flushSync();
  button("Headline", root).click(); flushSync();
  let finish!: () => void;
  request.mockImplementationOnce(() => new Promise(resolve => {
    finish = () => resolve({ document: structuredClone(doc), applied_revision: 1, duplicate: false, can_undo: true, can_redo: false });
  }));
  const originalSvg = root.querySelector('svg[aria-label="Editable creative project"]')!;
  const originalText = root.querySelector('[data-node-id="title"] text')!;
  const input = root.querySelector<HTMLTextAreaElement>('[aria-label="Text content"]')!;
  input.value = "Optimistic, not a flash";
  input.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  expect(originalSvg.textContent).toContain("Optimistic, not a flash");
  expect(root.textContent).toContain("Saving…");
  doc = { ...doc, revision: 1, scene: applySceneOperations(doc.scene, (request.mock.calls.find(c => c[0] === "create.projects.by_id.transactions.post")![2] as { operations: CreativeOperation[] }).operations) };
  finish(); await settle();
  expect(root.querySelector('svg[aria-label="Editable creative project"]')).toBe(originalSvg);
  expect(root.querySelector('[data-node-id="title"] text')).toBe(originalText);
  expect(originalSvg.textContent).toContain("Optimistic, not a flash");
  expect(request.mock.calls.filter(c => c[0] === "create.projects.get")).toHaveLength(0);
  expect(root.textContent).toContain("Saved");
});
it("remembers tool settings and selection across tab remounts, with Details beside Style", async () => {
  const first = render(); await settle(); await settle();
  expect(first.querySelector('.create-canvas-controls [aria-label="Toggle inspector"]')).toBeTruthy();
  expect(first.querySelector('footer [aria-label="Toggle inspector"]')).toBeNull();
  button("Toggle inspector", first).click(); flushSync();
  button("Headline", first).click(); flushSync();
  button("Erase", first).click(); flushSync();
  button("Whole object", first).click(); flushSync();
  button("Draw", first).click(); flushSync();
  button("Pencil", first).click(); flushSync();
  button("Blue", first).click(); flushSync();
  expect(transactions).toEqual([]);
  expect(doc.scene.nodes[1].content).toMatchObject({ color: "#000" });
  const svg = first.querySelector('svg[aria-label="Editable creative project"]')!;
  svg.dispatchEvent(new WheelEvent("wheel", { bubbles: true, deltaX: 40, deltaY: 25 }));
  flushSync();
  const camera = first.querySelector("[data-camera]")!.getAttribute("transform");
  await unmount(mounted.pop()!); first.remove();
  const second = render(); await settle(); await settle();
  expect(second.querySelector("[data-camera]")!.getAttribute("transform")).toBe(camera);
  expect(button("Pencil", second).getAttribute("aria-pressed")).toBe("true");
  button("Erase", second).click(); flushSync();
  expect(button("Whole object", second).getAttribute("aria-pressed")).toBe("true");
  button("Select", second).click(); flushSync();
  expect(second.querySelector('.create-selection-actions')?.textContent).toContain("1 selected");
  button("Lasso", second).click(); flushSync();
  expect(button("Lasso", second).getAttribute("aria-pressed")).toBe("true");
});

it("groups and renames a drawing through durable transactions, then ungroups its contents", async () => {
  const subtitle = structuredClone(doc.scene.nodes[1]); subtitle.id = "subtitle";
  if (subtitle.content.kind === "text") subtitle.content.text = "Subtitle";
  subtitle.transform.y += 150;
  doc.scene.nodes.push(subtitle);
  const root = render(); await settle(); await settle();
  button("Toggle inspector", root).click(); flushSync();
  button("Headline", root).click(); flushSync();
  button("Subtitle", root).dispatchEvent(new MouseEvent("click", { bubbles: true, shiftKey: true })); flushSync();
  expect(root.querySelectorAll("[data-selection-box]")).toHaveLength(1);
  button("Group selection", root).click(); await settle();
  const group = doc.scene.nodes.find(n => n.content.kind === "group")!;
  expect(group.parent_id).toBe("frame");
  expect(doc.scene.nodes.filter(n => n.parent_id === group.id)).toHaveLength(2);
  expect(button("Expand Group 1", root)).toBeTruthy();
  button("Group 1", root).dispatchEvent(new MouseEvent("dblclick", { bubbles: true })); await tick(); flushSync();
  const rename = root.querySelector<HTMLInputElement>('[aria-label="Layer name"]')!;
  rename.value = "Smiley"; rename.dispatchEvent(new Event("input", { bubbles: true })); flushSync();
  rename.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })); await settle();
  expect(doc.scene.nodes.find(n => n.id === group.id)!.name).toBe("Smiley");
  root.querySelector('[data-node-id="title"] text')!.dispatchEvent(new MouseEvent("dblclick", { bubbles: true, clientX: 40, clientY: 50 })); await settle();
  expect(button("Exit group", root)).toBeTruthy();
  button("Exit group", root).click(); await settle();
  expect(root.querySelector('[aria-label="Exit group"]')).toBeNull();
  button("Ungroup selection", root).click(); await settle();
  expect(doc.scene.nodes.some(n => n.content.kind === "group")).toBe(false);
  expect(doc.scene.nodes.find(n => n.id === "title")!.parent_id).toBe("frame");
  expect(doc.scene.nodes.find(n => n.id === "subtitle")!.parent_id).toBe("frame");
  expect(transactions).toHaveLength(3);
});

it("keeps a text draft and live font controls together in one durable edit", async () => {
  const root = render(); await settle(); await settle();
  button("Toggle inspector", root).click(); flushSync();
  root.querySelector('[data-node-id="title"] text')!.dispatchEvent(new MouseEvent("dblclick", { bubbles: true, clientX: 40, clientY: 50 }));
  await tick(); flushSync();
  const input = root.querySelector<HTMLTextAreaElement>('[aria-label="Canvas text"]')!;
  input.value = "Graphic Design\nis my Passion"; input.dispatchEvent(new Event("input", { bubbles: true })); flushSync();
  const controls = root.querySelector('[aria-label="Style palette"]')!;
  const size = controls.querySelector<HTMLInputElement>('[aria-label="Font size"]')!;
  input.dispatchEvent(new FocusEvent("blur", { relatedTarget: size })); flushSync();
  expect(root.querySelector('[aria-label="Canvas text"]')).toBe(input);
  size.value = "64"; size.dispatchEvent(new Event("input", { bubbles: true })); flushSync();
  const family = controls.querySelector<HTMLSelectElement>('[aria-label="Font family"]')!;
  family.value = "serif"; family.dispatchEvent(new Event("change", { bubbles: true })); flushSync();
  button("Bold", root).click(); button("Italic", root).click(); flushSync();
  expect(transactions).toHaveLength(0);
  expect(input.style.fontSize).toBe("64px");
  expect(input.style.fontFamily).toBe("serif");
  expect(input.style.fontWeight).toBe("700");
  expect(input.style.fontStyle).toBe("italic");
  const detailsSize = root.querySelector<HTMLInputElement>('[aria-label="Selection and layers"] [aria-label="Font size"]')!;
  expect(detailsSize.value).toBe("64");
  detailsSize.value = "80"; detailsSize.dispatchEvent(new Event("input", { bubbles: true })); flushSync();
  expect(input.style.fontSize).toBe("80px");
  expect(transactions).toHaveLength(0);
  const width = parseFloat(input.style.width), height = parseFloat(input.style.height);
  input.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, key: "Escape" })); await settle();
  expect(transactions).toHaveLength(1);
  const text = doc.scene.nodes.find(n => n.id === "title")!;
  expect(text.content).toMatchObject({ text: "Graphic Design\nis my Passion", font_size: 80, font_family: "serif", font_weight: 700, font_style: "italic" });
  expect(text.transform.width).toBeCloseTo(width);
  expect(text.transform.height).toBeCloseTo(height);
  expect(Number(root.querySelector('[data-selection-box] > rect')!.getAttribute("width"))).toBeCloseTo(width);
  const before = text.transform.height;
  size.value = "16"; size.dispatchEvent(new Event("input", { bubbles: true })); await settle();
  expect(doc.scene.nodes.find(n => n.id === "title")!.transform.height).toBeLessThan(before);
});
