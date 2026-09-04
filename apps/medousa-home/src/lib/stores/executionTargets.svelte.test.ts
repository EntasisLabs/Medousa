import { describe, expect, it } from "vitest";
import type { ExecutionTargetInventory } from "$lib/daemon/runtime";
import { ExecutionTargetStore } from "$lib/stores/executionTargets.svelte";

function inventory(): ExecutionTargetInventory {
  return {
    schema_version: 1,
    parent_runtime_id: "runtime-phone",
    default_runtime_id: "runtime-mac-mini",
    targets: [
      {
        runtime_id: "runtime-phone",
        label: "This iPhone",
        capabilities: ["assistant.work"],
        user_selectable: true,
        agent_selectable: false,
      },
      {
        runtime_id: "runtime-mac-mini",
        label: "Mac mini",
        capabilities: ["assistant.work", "shell.execute"],
        user_selectable: true,
        agent_selectable: true,
      },
    ],
  };
}

describe("ExecutionTargetStore", () => {
  it("shows a remote default and carries an exact user choice into the turn", async () => {
    const store = new ExecutionTargetStore(async () => inventory());
    store.activateWorkshopScope("personal-test");
    await store.refresh();

    expect(store.shouldShow("session-1")).toBe(true);
    expect(store.selectionLabel("session-1")).toBe("Mac mini");

    store.setSelection("session-1", {
      kind: "exact",
      runtime_id: "runtime-phone",
    });
    expect(store.turnSelection("session-1")).toEqual({
      kind: "exact",
      runtime_id: "runtime-phone",
    });
  });

  it("adds a stable per-session key to Auto without widening its candidates", async () => {
    const store = new ExecutionTargetStore(async () => inventory());
    store.activateWorkshopScope("personal-test");
    await store.refresh();
    store.setSelection("session-2", { kind: "auto" });

    expect(store.agentTargets().map((target) => target.runtime_id)).toEqual([
      "runtime-mac-mini",
    ]);
    expect(store.turnSelection("session-2")).toEqual({
      kind: "auto",
      requirements: { selection_key: "session:session-2" },
    });
  });

  it("keeps a stale exact choice visible so admission rejects instead of falling back", async () => {
    const store = new ExecutionTargetStore(async () => inventory());
    store.activateWorkshopScope("personal-test");
    await store.refresh();
    store.setSelection("session-3", {
      kind: "exact",
      runtime_id: "runtime-offline",
    });

    expect(store.selectionUnavailable("session-3")).toBe(true);
    expect(store.turnSelection("session-3")).toEqual({
      kind: "exact",
      runtime_id: "runtime-offline",
    });
  });

  it("uses no native override for the parent but preserves an exact remote authority", async () => {
    const store = new ExecutionTargetStore(async () => inventory());
    store.activateWorkshopScope("personal-test");
    await store.refresh();

    expect(store.transportRuntimeId("runtime-phone")).toBeNull();
    expect(store.transportRuntimeId("runtime-mac-mini")).toBe("runtime-mac-mini");
    expect(store.transportRuntimeId("runtime-offline")).toBe("runtime-offline");
  });
});
