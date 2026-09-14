import { describe, expect, it, vi } from "vitest";
import {
  MobileDrawControls,
  type MobileDrawControlSet,
} from "./mobileDrawControls.svelte";

function controls(tool: MobileDrawControlSet["tool"]): MobileDrawControlSet {
  return {
    tool,
    optionsOpen: false,
    canUndo: false,
    canRedo: false,
    setTool: vi.fn(),
    openOptions: vi.fn(),
    undo: vi.fn(),
    redo: vi.fn(),
  };
}

describe("mobile drawing controls", () => {
  it("updates the active surface without letting background drawings steal chrome", () => {
    const registry = new MobileDrawControls();
    const first = {};
    const second = {};
    registry.register(first, controls("ink"));
    registry.register(second, controls("eraser"));
    expect(registry.current?.tool).toBe("ink");

    registry.activate(second);
    expect(registry.current?.tool).toBe("eraser");
    registry.register(first, controls("select"));
    expect(registry.current?.tool).toBe("eraser");

    registry.unregister(second);
    expect(registry.current?.tool).toBe("select");
    registry.unregister(first);
    expect(registry.current).toBeNull();
  });
});
