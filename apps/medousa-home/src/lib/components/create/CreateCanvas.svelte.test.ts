/** @vitest-environment happy-dom */
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { mount, unmount, flushSync, tick } from "svelte";
import CreateCanvas from "./CreateCanvas.svelte";
import { createNode } from "$lib/create/creativeScene";
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
    root = render([first, locked]),
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

it("keeps resizing available through Create's existing handles", () => {
  const node=title(),root=render([node],{selected:[node.id]}),svg=root.querySelector("svg")!,handle=root.querySelector("[data-resize]")!;
  pointer(handle,"pointerdown",220,90);pointer(svg,"pointermove",320,150);pointer(svg,"pointerup",320,150);
  expect(commit).toHaveBeenCalledWith("Resize selection",[expect.objectContaining({op:"put_node",node:expect.objectContaining({id:node.id,transform:expect.objectContaining({width:300,height:120})})})]);
});
