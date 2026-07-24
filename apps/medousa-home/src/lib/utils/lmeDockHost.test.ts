import { afterEach, describe, expect, it, vi } from "vitest";
import {
  getLmeDockHost,
  popLmeDockHost,
  portLmeDock,
  pushLmeDockHost,
  setLmeDockHost,
} from "./lmeDockHost";

function fakeHost(id: string): HTMLElement {
  const children: HTMLElement[] = [];
  const el = {
    id,
    get children() {
      return children;
    },
    appendChild(node: HTMLElement) {
      const parent = (node as HTMLElement & { parentElement: HTMLElement | null })
        .parentElement;
      if (parent && "removeChild" in parent) {
        (parent as HTMLElement & { removeChild: (n: HTMLElement) => void }).removeChild(
          node,
        );
      }
      children.push(node);
      (node as HTMLElement & { parentElement: HTMLElement | null }).parentElement =
        el as unknown as HTMLElement;
      return node;
    },
    removeChild(node: HTMLElement) {
      const at = children.indexOf(node);
      if (at >= 0) children.splice(at, 1);
      (node as HTMLElement & { parentElement: HTMLElement | null }).parentElement = null;
      return node;
    },
  };
  return el as unknown as HTMLElement;
}

function fakeDock(id: string, parent: HTMLElement): HTMLElement {
  const el = {
    id,
    parentElement: parent as HTMLElement | null,
    classList: {
      add: vi.fn(),
      remove: vi.fn(),
    },
    remove() {
      if (el.parentElement && "removeChild" in el.parentElement) {
        (
          el.parentElement as HTMLElement & { removeChild: (n: HTMLElement) => void }
        ).removeChild(el as unknown as HTMLElement);
      }
    },
  };
  (parent as HTMLElement & { appendChild: (n: HTMLElement) => HTMLElement }).appendChild(
    el as unknown as HTMLElement,
  );
  return el as unknown as HTMLElement;
}

describe("lmeDockHost", () => {
  afterEach(() => {
    popLmeDockHost();
    popLmeDockHost();
    setLmeDockHost(null);
  });

  it("keeps the dock in its rail footer until a popover overlay is pushed", () => {
    const rail = fakeHost("rail-footer");
    const popover = fakeHost("popover");
    const dock = fakeDock("dock", rail);

    portLmeDock(dock);
    expect(dock.parentElement).toBe(rail);
    expect(getLmeDockHost()).toBe(rail);

    pushLmeDockHost(popover);
    expect(dock.parentElement).toBe(popover);
    expect(getLmeDockHost()).toBe(popover);

    popLmeDockHost();
    expect(dock.parentElement).toBe(rail);
  });

  it("ignores setLmeDockHost while a popover overlay is active", () => {
    const rail = fakeHost("rail-footer");
    const popover = fakeHost("popover");
    const status = fakeHost("status");
    const dock = fakeDock("dock", rail);

    portLmeDock(dock);
    pushLmeDockHost(popover);
    expect(dock.parentElement).toBe(popover);

    setLmeDockHost(status);
    expect(dock.parentElement).toBe(popover);

    popLmeDockHost();
    expect(dock.parentElement).toBe(rail);
  });
});
