/** @vitest-environment happy-dom */
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { mount, unmount, flushSync, tick } from "svelte";
import CreateCanvas from "./CreateCanvas.svelte";
import { layoutText } from "$lib/create/creativeText";
import { dragShape } from "$lib/create/creativeInteraction";
import { lineGeometry } from "$lib/create/creativeLine";
import { selectionFrame } from "$lib/create/creativeSelection";
import { defaultLayout } from "$lib/create/creativeLayout";
import { cloneCreativeSvg } from "$lib/create/creativeRender";
import { createNode, nodeDimensions } from "$lib/create/creativeScene";
import type {
  CreativeNode,
  CreativeOperation,
  CreativeScene,
} from "$lib/types/generated/daemon_api";
let components: ReturnType<typeof mount>[] = [];
const commit = vi.fn(),
  select = vi.fn(),
  error = vi.fn();
const title = () =>
  createNode(
    {
      kind: "text",
      text: "Original",
      color: "#123456",
      font_size: 32,
      font_family: "sans-serif",
    },
    { x: 20, y: 30, width: 200, height: 60 },
  );
function render(
  nodes: CreativeNode[] = [],
  options: Record<string, unknown> = {},
) {
  const target = document.createElement("div");
  document.body.append(target);
  const scene: CreativeScene = {
    schema: "medousa-create",
    version: 3,
    background: "transparent",
    nodes,
  };
  components.push(
    mount(CreateCanvas, {
      target,
      props: {
        scene,
        images: {},
        oncommit: commit,
        onselect: select,
        onerror: error,
        layout: { ...defaultLayout, smartGuides: false },
        ...options,
      },
    }),
  );
  flushSync();
  return target;
}
function pointer(
  target: Element,
  type: string,
  x: number,
  y: number,
  shiftKey = false,
  altKey = false,
  ctrlKey = false,
) {
  target.dispatchEvent(
    new PointerEvent(type, {
      bubbles: true,
      clientX: x,
      clientY: y,
      pointerId: 1,
      button: 0,
      pointerType: "mouse",
      shiftKey,
      altKey,
      ctrlKey,
    }),
  );
  flushSync();
}
beforeEach(() => {
  commit.mockReset();
  select.mockReset();
  error.mockReset();
  SVGElement.prototype.setPointerCapture = vi.fn();
  SVGElement.prototype.hasPointerCapture = () => false;
});
afterEach(async () => {
  for (const c of components) await unmount(c);
  components = [];
  document.body.replaceChildren();
});
it("types directly without committing placeholder text, then saves one complete edit", async () => {
  const root = render([], { tool: "text" }),
    svg = root.querySelector("svg")!;
  pointer(svg, "pointerdown", 80, 90);
  await tick();
  flushSync();
  const input = root.querySelector<HTMLTextAreaElement>(
    '[aria-label="Canvas text"]',
  )!;
  expect(input).toBeTruthy();
  expect(commit).not.toHaveBeenCalled();
  input.value = "Hello\nCanvas";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  input.dispatchEvent(
    new KeyboardEvent("keydown", { key: "Escape", bubbles: true }),
  );
  flushSync();
  expect(commit).toHaveBeenCalledTimes(1);
  expect(commit.mock.calls[0]).toMatchObject([
    "Add text",
    [
      {
        op: "put_node",
        node: { content: { kind: "text", text: "Hello\nCanvas" } },
      },
    ],
  ]);
});
it("blocks native SVG drag and selection chrome while retaining normal text editing", async () => {
  const node = title(), root = render([node]);
  const text = root.querySelector(`[data-node-id="${node.id}"] text`)!;
  for (const type of ["dragstart", "selectstart", "contextmenu"]) {
    const event = new Event(type, { bubbles: true, cancelable: true });
    text.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(true);
  }
  text.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
  await tick(); flushSync();
  const input = root.querySelector<HTMLTextAreaElement>('[aria-label="Canvas text"]')!;
  const event = new Event("selectstart", { bubbles: true, cancelable: true });
  input.dispatchEvent(event);
  expect(event.defaultPrevented).toBe(false);
  input.value = "Editable label"; input.dispatchEvent(new Event("input", { bubbles: true })); flushSync();
  input.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })); flushSync();
  expect(commit).toHaveBeenCalledWith("Edit text", [expect.objectContaining({ node: expect.objectContaining({ content: expect.objectContaining({ text: "Editable label" }) }) })]);
});
it("double-click edits existing text without opening an inspector or adding a second node", async () => {
  const node = title(),
    root = render([node]);
  root
    .querySelector(`[data-node-id="${node.id}"] text`)!
    .dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
  await tick();
  flushSync();
  const input = root.querySelector<HTMLTextAreaElement>("textarea")!;
  input.value = "Edited";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  input.dispatchEvent(new FocusEvent("blur"));
  flushSync();
  const ops = commit.mock.calls[0][1] as CreativeOperation[];
  expect(ops).toMatchObject([
    { op: "put_node", node: { id: node.id, content: { text: "Edited" } } },
  ]);
  expect(node.content).toMatchObject({ text: "Original" });
});
it("lasso selection excludes locked nodes and commits no edit", () => {
  const first = title(),
    locked = { ...title(), locked: true },
    root = render([first, locked], { selectionMode: "lasso" }),
    svg = root.querySelector("svg")!;
  pointer(svg, "pointerdown", 0, 0);
  pointer(svg, "pointermove", 250, 0);
  pointer(svg, "pointermove", 250, 150);
  pointer(svg, "pointermove", 0, 150);
  pointer(svg, "pointermove", 0, 0);
  pointer(svg, "pointerup", 0, 0);
  expect(select).toHaveBeenLastCalledWith([first.id]);
  expect(commit).not.toHaveBeenCalled();
});
it("preserves the last saved text when an empty new text entry is abandoned", async () => {
  const root = render([], { tool: "text" });
  pointer(root.querySelector("svg")!, "pointerdown", 10, 10);
  await tick();
  flushSync();
  root.querySelector("textarea")!.dispatchEvent(new FocusEvent("blur"));
  flushSync();
  expect(commit).not.toHaveBeenCalled();
  expect(root.querySelector("textarea")).toBeNull();
});
it("erases in one durable transaction per gesture and supports cancellation", () => {
  const node = title(),
    root = render([node], { tool: "erase", eraserMode: "stroke" }),
    svg = root.querySelector("svg")!;
  pointer(svg, "pointerdown", 30, 40);
  pointer(svg, "pointercancel", 30, 40);
  expect(commit).not.toHaveBeenCalled();
  pointer(svg, "pointerdown", 30, 40);
  pointer(svg, "pointerup", 30, 40);
  expect(commit).toHaveBeenCalledWith("Erase strokes", [
    { op: "remove_node", id: node.id },
  ]);
});
it("recognizes double taps despite SVG pointer capture, without saving a no-op move", async () => {
  const node = title(),
    root = render([node]),
    hit = root.querySelector(`[data-node-id="${node.id}"] text`)!;
  const svg = root.querySelector("svg")!;
  pointer(hit, "pointerdown", 30, 40);
  pointer(svg, "pointerup", 30, 40);
  pointer(hit, "pointerdown", 30, 40);
  pointer(svg, "pointerup", 30, 40);
  await tick();
  flushSync();
  expect(root.querySelector<HTMLTextAreaElement>("textarea")?.value).toBe(
    "Original",
  );
  expect(commit).not.toHaveBeenCalled();
});
it("keeps Create shape placement and resizing alongside the shared drawing controller", () => {
  const root = render([], { tool: "shape", shape: "ellipse" }),
    svg = root.querySelector("svg")!;
  pointer(svg, "pointerdown", 20, 30);
  pointer(svg, "pointermove", 120, 90);
  pointer(svg, "pointerup", 120, 90);
  expect(commit).toHaveBeenCalledTimes(1);
  expect(commit.mock.calls[0]).toMatchObject([
    "Add shape",
    [
      {
        op: "put_node",
        node: {
          content: { kind: "shape", shape: "ellipse" },
          transform: { x: 20, y: 30, width: 100, height: 60 },
        },
      },
    ],
  ]);
});

it("resizes text and its measured bounds together through corner handles", () => {
  const node = title(), before = nodeDimensions(node), root = render([node], { selected: [node.id] }), svg = root.querySelector("svg")!;
  const corner = { x: node.transform.x + before.width, y: node.transform.y + before.height };
  pointer(root.querySelector('[data-handle="se"]')!, "pointerdown", corner.x, corner.y);
  pointer(svg, "pointermove", corner.x + before.width, corner.y + before.height);
  pointer(svg, "pointerup", corner.x + before.width, corner.y + before.height);
  expect(commit.mock.calls[0][0]).toBe("Resize selection");
  const next = commit.mock.calls[0][1][0].node as CreativeNode;
  expect(next.content).toMatchObject({ font_size: 64 });
  expect(next.transform.width).toBeCloseTo(nodeDimensions(next).width);
  expect(next.transform.height).toBeCloseTo(nodeDimensions(next).height);
  expect(next.transform.x).toBeCloseTo(20); expect(next.transform.y).toBeCloseTo(30);
});

it("defaults to a box, adds with Shift, and keeps explicit lasso available", () => {
  const a = title(), b = title();
  b.transform.x = 350;
  const root = render([a, b]), svg = root.querySelector("svg")!;
  pointer(svg, "pointerdown", 5, 5);
  pointer(svg, "pointermove", 250, 110);
  expect(root.querySelector('[aria-label="Box selection"]')).toBeTruthy();
  pointer(svg, "pointerup", 250, 110);
  expect(select).toHaveBeenLastCalledWith([a.id]);
  pointer(svg, "pointerdown", 330, 5, true);
  pointer(svg, "pointermove", 570, 110, true);
  pointer(svg, "pointerup", 570, 110, true);
  expect(select).toHaveBeenLastCalledWith([a.id, b.id]);
  const lasso = render([a], { selectionMode: "lasso" }), path = lasso.querySelector("svg")!;
  pointer(path, "pointerdown", 5, 5);
  pointer(path, "pointermove", 250, 5);
  pointer(path, "pointermove", 250, 110);
  pointer(path, "pointermove", 5, 110);
  expect(lasso.querySelector('[aria-label="Lasso selection"]')).toBeTruthy();
  pointer(path, "pointerup", 5, 5);
  expect(select).toHaveBeenLastCalledWith([a.id]);
});

it("uses the release point for box selection and collapses boxes dragged back to their start", () => {
  const node = title(), root = render([node]), svg = root.querySelector("svg")!;
  pointer(svg, "pointerdown", 5, 5);
  pointer(svg, "pointerup", 250, 110);
  expect(select).toHaveBeenLastCalledWith([node.id]);
  pointer(svg, "pointerdown", 5, 5);
  pointer(svg, "pointermove", 250, 110);
  pointer(svg, "pointerup", 5, 5);
  expect(select).toHaveBeenLastCalledWith([]);
});

it("duplicates a frame with remapped descendants and deletes only the duplicate root", () => {
  const frame = createNode({ kind: "frame", name: "Original", background: "#fff" }, { x: 0, y: 0, width: 500, height: 300 });
  const child = title(); child.parent_id = frame.id;
  const root = render([frame, child], { selected: [frame.id] });
  const stage = root.querySelector('[aria-label="Creative canvas"]')!;
  stage.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, key: "d", metaKey: true }));
  flushSync();
  expect(commit).toHaveBeenCalledTimes(1);
  const operations = commit.mock.calls[0][1] as CreativeOperation[];
  const copies = operations.flatMap(op => op.op === "put_node" ? [op.node] : []);
  const copy = copies.find(n => n.content.kind === "frame")!;
  expect(copies).toHaveLength(2);
  expect(copies.find(n => n.content.kind === "text")!.parent_id).toBe(copy.id);
  expect(copy.id).not.toBe(frame.id);
  // A newly rendered acknowledged scene, selected as after duplication.
  const updated = render([frame, child, ...copies], { selected: [copy.id] });
  updated.querySelector('[aria-label="Creative canvas"]')!.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, key: "Delete" }));
  flushSync();
  expect(commit).toHaveBeenLastCalledWith("Delete selection", [{ op: "remove_node", id: copy.id }]);
});

it("picks and moves a group as one object, enters on double-click, and exits with Escape", async () => {
  const group = createNode({ kind: "group" }, { x: 0, y: 0, width: 300, height: 200 }); group.name = "Smiley";
  const child = title(); child.parent_id = group.id;
  const scope = vi.fn(), root = render([group, child], { onScope: scope }), svg = root.querySelector("svg")!;
  pointer(svg, "pointerdown", 40, 50); pointer(svg, "pointermove", 70, 80); pointer(svg, "pointerup", 70, 80);
  expect(commit).toHaveBeenLastCalledWith("Move selection", [expect.objectContaining({ op: "put_node", node: expect.objectContaining({ id: group.id, transform: expect.objectContaining({ x: 30, y: 30 }) }) })]);
  expect(select).toHaveBeenLastCalledWith([group.id]);
  root.querySelector(`[data-node-id="${child.id}"] text`)!.dispatchEvent(new MouseEvent("dblclick", { bubbles: true, clientX: 40, clientY: 50 }));
  await tick(); flushSync();
  expect(scope).toHaveBeenLastCalledWith("Smiley");
  expect(select).toHaveBeenLastCalledWith([child.id]);
  root.querySelector('[aria-label="Creative canvas"]')!.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, key: "Escape" })); flushSync();
  expect(scope).toHaveBeenLastCalledWith(null);
  expect(select).toHaveBeenLastCalledWith([group.id]);
});
it("renders one proportional resize box for a group and scales its contents", () => {
  const group = createNode({ kind: "group" }, { x: 0, y: 0, width: 300, height: 200 }), child = title(); child.parent_id = group.id;
  const root = render([group, child], { selected: [group.id] }), svg = root.querySelector("svg")!;
  expect(root.querySelectorAll("[data-selection-box]")).toHaveLength(1);
  expect(root.querySelectorAll("[data-resize]")).toHaveLength(4);
  const bounds = nodeDimensions(child), x = child.transform.x, y = child.transform.y;
  pointer(root.querySelector('[data-handle="se"]')!, "pointerdown", x + bounds.width, y + bounds.height);
  pointer(svg, "pointermove", x + bounds.width * 2, y + bounds.height * 2); pointer(svg, "pointerup", x + bounds.width * 2, y + bounds.height * 2);
  const operations = commit.mock.calls[0][1] as CreativeOperation[];
  expect(operations).toHaveLength(2);
  expect(operations.find(op => op.op === "put_node" && op.node.id === child.id)).toMatchObject({ node: { transform: { width: 400, height: 120 }, content: { font_size: 64 } } });
});
it("Option-drag duplicates a whole group and Escape cancels an unfinished duplicate", () => {
  const group = createNode({ kind: "group" }, { x: 0, y: 0, width: 300, height: 200 }), child = title(); child.parent_id = group.id;
  const root = render([group, child]), svg = root.querySelector("svg")!;
  pointer(svg, "pointerdown", 40, 50, false, true); pointer(svg, "pointermove", 80, 90); pointer(svg, "pointerup", 80, 90);
  expect(commit).toHaveBeenCalledTimes(1);
  expect(commit.mock.calls[0][0]).toBe("Duplicate selection");
  const copies = (commit.mock.calls[0][1] as CreativeOperation[]).flatMap(op => op.op === "put_node" ? [op.node] : []);
  expect(copies).toHaveLength(2);
  const copy = copies.find(n => n.content.kind === "group")!;
  expect(copy.transform).toMatchObject({ x: 40, y: 40 });
  expect(copies.find(n => n.content.kind === "text")!.parent_id).toBe(copy.id);
  pointer(svg, "pointerdown", 40, 50, false, true);
  root.querySelector('[aria-label="Creative canvas"]')!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })); flushSync();
  expect(commit).toHaveBeenCalledTimes(1);
  expect(root.querySelectorAll("[data-node-id]")).toHaveLength(2);
});
it("preserves a single object's aspect ratio with Shift-resize", () => {
  const node = title(); node.content = { kind: "shape", shape: "rectangle", fill: "#fff", stroke: "#000", stroke_width: 2 };
  const root = render([node], { selected: [node.id] }), svg = root.querySelector("svg")!;
  pointer(root.querySelector('[data-handle="se"]')!, "pointerdown", 220, 90);
  pointer(svg, "pointermove", 320, 150, true); pointer(svg, "pointerup", 320, 150, true);
  expect(commit.mock.calls[0][1]).toMatchObject([{ node: { transform: { x: 20, y: 30, width: 400, height: 120 } } }]);
});

it("creates new text inside the entered group after selecting a child", async () => {
  const group = createNode({ kind: "group" }, { x: 100, y: 100, width: 300, height: 200 }), child = title(); child.parent_id = group.id;
  const root = render([group, child]), svg = root.querySelector("svg")!;
  root.querySelector(`[data-node-id="${child.id}"] text`)!.dispatchEvent(new MouseEvent("dblclick", { bubbles: true, clientX: 140, clientY: 150 }));
  await tick(); flushSync();
  svg.dispatchEvent(new MouseEvent("dblclick", { bubbles: true, clientX: 380, clientY: 260 }));
  await tick(); flushSync();
  const input = root.querySelector<HTMLTextAreaElement>('[aria-label="Canvas text"]')!;
  input.value = "New group caption"; input.dispatchEvent(new Event("input", { bubbles: true })); flushSync();
  input.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })); flushSync();
  const operations = commit.mock.calls[0][1] as CreativeOperation[];
  expect(operations).toMatchObject([{ op: "put_node", node: { parent_id: group.id, content: { kind: "text", text: "New group caption" } } }]);
});

it("uses measured multiline text for rendering, selection, editing, and movement despite legacy dimensions", async () => {
  const node = title();
  node.content = { ...node.content, kind: "text", text: "Wide title\n\nSmall", color: "#123456", font_family: "sans-serif", font_size: 32 };
  node.transform.width = 900; node.transform.height = 10;
  const layout = layoutText(node.content), root = render([node], { selected: [node.id] }), svg = root.querySelector("svg")!;
  const box = root.querySelector("[data-selection-box] > rect")!;
  expect(Number(box.getAttribute("width"))).toBeCloseTo(layout.width);
  expect(Number(box.getAttribute("height"))).toBeCloseTo(layout.height);
  const lines = root.querySelectorAll(`[data-node-id="${node.id}"] tspan`);
  expect(lines).toHaveLength(3);
  expect(Number(lines[0].getAttribute("y"))).toBeCloseTo(layout.baseline);
  expect(Number(lines[2].getAttribute("y"))).toBeLessThan(layout.height);
  root.querySelector(`[data-node-id="${node.id}"] text`)!.dispatchEvent(new MouseEvent("dblclick", { bubbles: true, clientX: 40, clientY: 50 }));
  await tick(); flushSync();
  const input = root.querySelector<HTMLTextAreaElement>('[aria-label="Canvas text"]')!;
  expect(parseFloat(input.style.width)).toBeCloseTo(layout.width);
  expect(parseFloat(input.style.height)).toBeCloseTo(layout.height);
  expect(input.getAttribute("wrap")).toBe("off");
  input.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, key: "Escape" })); flushSync();
  pointer(svg, "pointerdown", 40, 50); pointer(svg, "pointermove", 80, 80); pointer(svg, "pointerup", 80, 80);
  expect(commit).toHaveBeenLastCalledWith("Move selection", [expect.objectContaining({ node: expect.objectContaining({ transform: expect.objectContaining({ x: 60, y: 60 }), content: node.content }) })]);
});

it("uses two endpoints for arrows, with one undoable endpoint gesture and cancellation", () => {
  const node = dragShape(title(), "arrow", { x: 50, y: 80 }, { x: 250, y: 80 }, "#000", 4, "none");
  const root = render([node], { selected: [node.id] }), svg = root.querySelector("svg")!;
  expect(root.querySelectorAll("[data-endpoint]")).toHaveLength(2);
  expect(root.querySelectorAll("[data-resize]")).toHaveLength(0);
  expect(root.querySelectorAll("[data-arrowhead]")).toHaveLength(1);
  pointer(root.querySelector('[data-endpoint="end"]')!, "pointerdown", 250, 80);
  pointer(svg, "pointermove", 210, 150); pointer(svg, "pointerup", 210, 150);
  expect(commit).toHaveBeenCalledTimes(1); expect(commit.mock.calls[0][0]).toBe("Move endpoint");
  const next = commit.mock.calls[0][1][0].node as CreativeNode, line = lineGeometry(next)!;
  expect(next.transform.x + line.start.x).toBeCloseTo(50); expect(next.transform.y + line.start.y).toBeCloseTo(80);
  expect(next.transform.x + line.end.x).toBeCloseTo(210); expect(next.transform.y + line.end.y).toBeCloseTo(150);
  pointer(root.querySelector('[data-endpoint="start"]')!, "pointerdown", 50, 80); pointer(svg, "pointermove", 10, 10);
  root.querySelector('[aria-label="Creative canvas"]')!.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, key: "Escape" })); flushSync();
  expect(commit).toHaveBeenCalledTimes(1);
  expect(root.querySelectorAll("[data-endpoint]")).toHaveLength(2);
});
it("snaps new arrows to 45 degree directions and persists a real arrowhead", () => {
  const root = render([], { tool: "shape", shape: "arrow" }), svg = root.querySelector("svg")!;
  pointer(svg, "pointerdown", 30, 40); pointer(svg, "pointermove", 220, 100, true); pointer(svg, "pointerup", 220, 100, true);
  expect(commit).toHaveBeenCalledTimes(1);
  const node = commit.mock.calls[0][1][0].node as CreativeNode, line = lineGeometry(node)!;
  expect(line.end.y).toBeCloseTo(line.start.y);
  expect(node.content).toMatchObject({ kind: "shape", shape: "line", line: { end_arrow: true } });
});
it("rotates around the visible center with 15 degree Shift snapping and cancels a draft", () => {
  const node = createNode({ kind: "shape", shape: "rectangle", fill: "#fff", stroke: "#000", stroke_width: 2 }, { x: 0, y: 0, width: 200, height: 100 });
  const root = render([node], { selected: [node.id] }), svg = root.querySelector("svg")!;
  pointer(root.querySelector('[data-rotate]')!, "pointerdown", 100, -28);
  pointer(svg, "pointermove", 180, -10, true); pointer(svg, "pointerup", 180, -10, true);
  expect(commit).toHaveBeenCalledTimes(1); expect(commit.mock.calls[0][0]).toBe("Rotate selection");
  const next = commit.mock.calls[0][1][0].node as CreativeNode, frame = selectionFrame([node.id], [next])!;
  expect(next.transform.rotation).toBeCloseTo(60);
  expect(frame.center.x).toBeCloseTo(100); expect(frame.center.y).toBeCloseTo(50);
  pointer(root.querySelector('[data-rotate]')!, "pointerdown", 100, -28); pointer(svg, "pointermove", 170, 50);
  root.querySelector('[aria-label="Creative canvas"]')!.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, key: "Escape" })); flushSync();
  expect(commit).toHaveBeenCalledTimes(1);
  expect(root.querySelector('[data-selection-box]')!.getAttribute("transform")).toBe("matrix(1 0 0 1 0 0)");
});
it("keeps click-only rotation feedback outside the image SVG and clears it without an edit", () => {
  const image = createNode({ kind: "image", asset_id: "background" }, { x: 0, y: 0, width: 400, height: 300 });
  const arrow = dragShape(title(), "arrow", { x: 50, y: 80 }, { x: 250, y: 80 }, "#000", 4, "none");
  const root = render([image, arrow], { selected: [arrow.id], images: { background: "data:image/png;base64,AA==" } }), svg = root.querySelector("svg")!;
  const bitmap = svg.querySelector("image")!;
  pointer(root.querySelector('[data-rotate]')!, "pointerdown", 150, 52);
  const badge = root.querySelector<HTMLElement>('[aria-label="Rotation angle"]')!;
  expect(badge.textContent).toBe("0°");
  expect(badge.closest("svg")).toBeNull();
  expect(badge.style.left).toBe("162px");
  expect(badge.style.top).toBe("52px");
  pointer(svg, "pointerup", 150, 52);
  expect(root.querySelector('[aria-label="Rotation angle"]')).toBeNull();
  expect(svg.querySelector("image")).toBe(bitmap);
  expect(commit).not.toHaveBeenCalled();
});
it("lets touch reshape an endpoint, but cancels the draft when a second finger starts navigation", () => {
  const node = dragShape(title(), "arrow", { x: 50, y: 80 }, { x: 250, y: 80 }, "#123456", 4, "none");
  const root = render([node], { selected: [node.id] }), svg = root.querySelector("svg")!;
  const touch = (target: Element, type: string, id: number, x: number, y: number) => {
    target.dispatchEvent(new PointerEvent(type, { bubbles: true, pointerType: "touch", pointerId: id, button: 0, clientX: x, clientY: y })); flushSync();
  };
  touch(root.querySelector('[data-endpoint="end"]')!, "pointerdown", 10, 250, 80);
  touch(svg, "pointermove", 10, 300, 140);
  expect(root.querySelector('[data-selection-box]')!.getAttribute("transform")).not.toBe("matrix(1 0 0 1 50 80)");
  touch(svg, "pointerdown", 11, 400, 200);
  touch(svg, "pointermove", 11, 420, 210);
  touch(svg, "pointerup", 10, 300, 140); touch(svg, "pointerup", 11, 420, 210);
  expect(commit).not.toHaveBeenCalled();
  expect(root.querySelector('[data-selection-box]')!.getAttribute("transform")).toBe("matrix(1 0 0 1 50 80)");
});
it("keeps the multi-selection frame and angle attached while rotating its objects", () => {
  const content = { kind: "shape", shape: "rectangle", fill: "#fff", stroke: "#000", stroke_width: 2 } as const;
  const a = createNode(content, { x: 0, y: 0, width: 100, height: 100 }), b = createNode(content, { x: 200, y: 0, width: 100, height: 100 });
  const root = render([a, b], { selected: [a.id, b.id] }), svg = root.querySelector("svg")!;
  pointer(root.querySelector('[data-rotate]')!, "pointerdown", 150, -28);
  pointer(svg, "pointermove", 228, 50, true);
  const box = root.querySelector('[data-selection-box]')!;
  expect(box.querySelector("text")).toBeNull();
  const label = root.querySelector('[aria-label="Rotation angle"]')!;
  expect(label.textContent).toBe("90°");
  expect(label.closest("svg")).toBeNull();
  expect(box.querySelector("rect")!.getAttribute("width")).toBe("300");
  expect(box.querySelector("rect")!.getAttribute("height")).toBe("100");
  pointer(svg, "pointerup", 228, 50, true);
  expect(root.querySelector('[aria-label="Rotation angle"]')).toBeNull();
  expect(commit).toHaveBeenCalledTimes(1);
  const operations = commit.mock.calls[0][1] as { node: CreativeNode }[];
  expect(operations).toHaveLength(2);
  expect(operations.every(op => Math.abs(op.node.transform.rotation - 90) < .001)).toBe(true);
});

it("shows a world grid over frames without exporting it or selection guides", () => {
  const frame = createNode({ kind: "frame", name: "Paper", background: "#fff" }, { x: 0, y: 0, width: 400, height: 300 });
  const root = render([frame], { layout: { ...defaultLayout, grid: "dots" } });
  const svg = root.querySelector("svg")!;
  expect(svg.querySelector('[aria-label="Canvas grid"]')).not.toBeNull();
  const clone = cloneCreativeSvg(svg, { x: 0, y: 0, width: 400, height: 300 });
  expect(clone.querySelector("[data-layout-overlay]")).toBeNull();
  expect(clone.querySelector(`[data-node-id="${frame.id}"]`)).not.toBeNull();
  expect(commit).not.toHaveBeenCalled();
});
it("snaps an actual pointer drag, commits once, clears guides, and supports Ctrl bypass", () => {
  const a = createNode({ kind: "shape", shape: "rectangle", fill: "#fff", stroke: "#000", stroke_width: 1 }, { x: 0, y: 0, width: 20, height: 20 });
  const b = { ...a, id: "target", transform: { ...a.transform, x: 100, y: 50 } };
  const root = render([a, b], { layout: { ...defaultLayout } }), svg = root.querySelector("svg")!;
  pointer(svg, "pointerdown", 10, 10); pointer(svg, "pointermove", 87, 39);
  expect(svg.querySelector('[aria-label="Alignment guide"]')).not.toBeNull();
  expect(commit).not.toHaveBeenCalled();
  pointer(svg, "pointerup", 87, 39);
  expect(commit).toHaveBeenCalledTimes(1);
  expect(commit.mock.calls[0][1]).toMatchObject([{ node: { transform: { x: 80, y: 30 } } }]);
  expect(svg.querySelector('[aria-label="Alignment guide"]')).toBeNull();
  pointer(svg, "pointerdown", 10, 10); pointer(svg, "pointermove", 87, 39, false, false, true); pointer(svg, "pointerup", 87, 39, false, false, true);
  expect(commit.mock.calls[1][1]).toMatchObject([{ node: { transform: { x: 77, y: 29 } } }]);
});

it("snaps new shape endpoints to the grid without changing freehand behavior", () => {
  const root = render([], { tool: "shape", layout: { ...defaultLayout, snapGrid: true, smartGuides: false } }), svg = root.querySelector("svg")!;
  pointer(svg, "pointerdown", 13, 21); pointer(svg, "pointermove", 65, 79); pointer(svg, "pointerup", 65, 79);
  expect(commit.mock.calls[0][1]).toMatchObject([{ node: { transform: { x: 16, y: 16, width: 48, height: 64 } } }]);
});
