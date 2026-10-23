/** @vitest-environment happy-dom */
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { mount, unmount, flushSync, tick } from "svelte";
import { selectEditableText } from "$lib/create/creativeRichText";
import CreatePanel from "./CreatePanel.svelte";
import { applySceneOperations, createNode } from "$lib/create/creativeScene";
import { reconcileCreativeProposal } from "$lib/create/creativeProposal";
import { dragShape } from "$lib/create/creativeInteraction";
import { lineGeometry } from "$lib/create/creativeLine";
import { createBrowser } from "$lib/create/createBrowser.svelte";
import type {
  CreativeDocument,
  CreativeOperation,
} from "$lib/types/generated/daemon_api";
const { request, openCreative } = vi.hoisted(() => ({
  request: vi.fn(),
  openCreative: vi.fn(),
}));
vi.mock("$lib/daemon/contractClient", () => ({ daemonUnary: request }));
vi.mock("$lib/create/creativeCapture", () => ({ captureCreativeSelection: vi.fn().mockResolvedValue("cG5n") }));
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
it("persists line styles and rotation through the shared transaction session", async () => {
  const seed = createNode({ kind: "shape", shape: "line", fill: "transparent", stroke: "#123456", stroke_width: 4 }, { x: 0, y: 0, width: 1, height: 1 }, "frame");
  const arrow = dragShape(seed, "arrow", { x: 100, y: 200 }, { x: 400, y: 200 }, "#123456", 4, "none");
  doc.scene.nodes.push(arrow);
  const first = render(), second = render(); await settle(); await settle();
  button("Toggle inspector", first).click(); flushSync();
  button("Arrow", first).click(); flushSync();
  const palette = first.querySelector('[aria-label="Style palette"]')!;
  const pattern = palette.querySelector<HTMLSelectElement>('[aria-label="Line pattern"]')!;
  pattern.value = "dashed"; pattern.dispatchEvent(new Event("change", { bubbles: true })); await settle();
  expect(transactions).toHaveLength(1);
  expect(lineGeometry(doc.scene.nodes.find(n => n.id === arrow.id)!)).toMatchObject({ color: "#123456", dashed: true, end_arrow: true });
  expect(second.querySelector(`[data-node-id="${arrow.id}"] line`)!.getAttribute("stroke-dasharray")).toBe("12 8");
  const rotation = first.querySelector<HTMLInputElement>('[aria-label="rotation"]')!;
  rotation.value = "45"; rotation.dispatchEvent(new Event("change", { bubbles: true })); await settle();
  expect(transactions).toHaveLength(2);
  expect(doc.scene.nodes.find(n => n.id === arrow.id)!.transform.rotation).toBeCloseTo(45);
  expect(first.querySelector(`[data-node-id="${arrow.id}"]`)!.getAttribute("transform")).toBe(second.querySelector(`[data-node-id="${arrow.id}"]`)!.getAttribute("transform"));
  const color = first.querySelector<HTMLInputElement>('[aria-label="Selection and layers"] input[type="color"]')!;
  color.value = "#ff0000"; color.dispatchEvent(new Event("input", { bubbles: true })); color.dispatchEvent(new Event("change", { bubbles: true })); await settle();
  expect(lineGeometry(doc.scene.nodes.find(n => n.id === arrow.id)!)?.color).toBe("#ff0000");
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
it("floats Details inside the workarea and remembers collapsed sections when reopened", async () => {
  const root = render(); await settle(); await settle();
  const canvas = root.querySelector('svg[aria-label="Editable creative project"]')!, box = canvas.getAttribute("viewBox");
  button("Toggle inspector", root).click(); flushSync();
  expect(root.querySelector('.create-workarea > [aria-label="Selection and layers"]')).toBeTruthy();
  expect(root.querySelector('[aria-label="Style palette"]')).toBeTruthy();
  button("Properties", root).click(); button("Layers", root).click(); flushSync();
  button("Close inspector", root).click(); flushSync();
  button("Toggle inspector", root).click(); flushSync();
  expect(button("Properties", root).getAttribute("aria-expanded")).toBe("false");
  expect(button("Layers", root).getAttribute("aria-expanded")).toBe("false");
  expect(root.querySelector('svg[aria-label="Editable creative project"]')).toBe(canvas);
  expect(canvas.getAttribute("viewBox")).toBe(box);
  expect(transactions).toEqual([]);
  const old = mounted.pop()!; await unmount(old); root.remove();
  const reopened = render(); await settle(); await settle();
  button("Toggle inspector", reopened).click(); flushSync();
  expect(button("Properties", reopened).getAttribute("aria-expanded")).toBe("false");
  expect(button("Layers", reopened).getAttribute("aria-expanded")).toBe("false");
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
it.each([false, true])("previews and applies a scoped job with later canvas movement: %s", async (moveAfterGeneration) => {
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
          base_nodes: [structuredClone(doc.scene.nodes[1])],
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
      if (op === "create.projects.by_id.assets.post") return { id: "visual-test", mime: "image/png", sha256: "a".repeat(64), byte_size: 3 };
      if (op === "create.projects.by_id.jobs.by_job_id.decision.post") {
        expect(body).toEqual({ accept: true, expected_revision: moveAfterGeneration ? 1 : 0 });
        doc = {
          ...doc,
          revision: doc.revision + 1,
          scene: applySceneOperations(doc.scene, reconcileCreativeProposal(doc.scene, doc.revision, proposed).operations),
        };
        return { ...proposed, status: "accepted", applied_revision: doc.revision };
      }
      if (op === "create.projects.by_id.transactions.post") {
        const transaction = body as { operations: CreativeOperation[] };
        transactions.push(transaction);
        doc = { ...doc, revision: doc.revision + 1, scene: applySceneOperations(doc.scene, transaction.operations) };
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
    visual_asset_id: "visual-test",
  });
  expect(admitted[3]).toBeNull();
  expect(admitted[5]).toBe("ui-test");
  if (moveAfterGeneration) {
    root.querySelector('[aria-label="Creative canvas"]')!.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }));
    await settle(); await settle();
    expect(root.textContent).toContain("Keeps your latest canvas edits");
  }
  button("Preview", root).click();
  flushSync();
  expect(
    root.querySelector('svg[aria-label="Editable creative project"]')
      ?.textContent,
  ).toContain("Previewed title");
  expect(doc.revision).toBe(moveAfterGeneration ? 1 : 0);
  expect(transactions).toHaveLength(moveAfterGeneration ? 1 : 0);
  button("Use this", root).click();
  await settle();
  await settle();
  expect(doc.revision).toBe(moveAfterGeneration ? 2 : 1);
  expect(doc.scene.nodes[1].transform.x).toBe(moveAfterGeneration ? 61 : 60);
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
  const input = first.querySelector<HTMLDivElement>(
    '[aria-label="Canvas text"]',
  )!;
  expect(input.textContent).toBe("Headline");
  input.textContent = "A direct headline";
  selectEditableText(input,(input.textContent??"").length); input.dispatchEvent(new Event("input", { bubbles: true }));
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
it("remembers tool settings and selection across tab remounts, with Details up top", async () => {
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
  const input = root.querySelector<HTMLDivElement>('[aria-label="Canvas text"]')!;
  input.textContent = "Graphic Design\nis my Passion"; selectEditableText(input,(input.textContent??"").length); input.dispatchEvent(new Event("input", { bubbles: true })); flushSync();
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

it("keeps the project browser in sync when undo changes a project's name and pin", async () => {
  doc.title = "Renamed"; doc.pinned = true; doc.revision = 1;
  const root = render(); await settle(); await settle();
  expect(createBrowser("ui-test").projects.find(p => p.id === doc.id)).toMatchObject({ title: "Renamed", pinned: true });
  request.mockImplementationOnce(async () => {
    doc = { ...doc, title: "Original", pinned: false, revision: 2 };
    return { document: structuredClone(doc), applied_revision: 2, duplicate: false, can_undo: false, can_redo: true };
  });
  button("Undo", root).click(); await settle();
  expect(createBrowser("ui-test").projects.find(p => p.id === doc.id)).toMatchObject({ title: "Original", pinned: false });
});

it("keeps View and Style mutually exclusive, including reopening Style from the tool strip", async () => {
  const root = render(); await settle(); await settle();
  const view = root.querySelector<HTMLElement>('[aria-label="Canvas view options"]')!;
  const menu = view.closest("details")!;
  expect(root.querySelector('[aria-label="Style palette"]')).not.toBeNull();
  view.click(); await settle();
  expect(menu.open).toBe(true);
  expect(root.querySelector('[aria-label="Style palette"]')).toBeNull();
  button("Show style palette", root).click(); await settle();
  expect(menu.open).toBe(false);
  expect(root.querySelector('[aria-label="Style palette"]')).not.toBeNull();
  view.click(); await settle();
  button("Select", root).click(); await settle();
  expect(menu.open).toBe(false);
  expect(root.querySelector('[aria-label="Style palette"]')).not.toBeNull();
  expect(transactions).toEqual([]);
});
