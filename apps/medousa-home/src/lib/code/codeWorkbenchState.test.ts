import { beforeEach, describe, expect, it } from "vitest";

import {
  codeWorkbenchState,
  normalizeCodeWorkbenchLayout,
  visibleCodeTabsInGroup,
} from "./codeWorkbenchState.svelte";

describe("codeWorkbenchState", () => {
  beforeEach(() => codeWorkbenchState.reset());

  it("keeps independent stacks per work id", () => {
    codeWorkbenchState.record("a", "a.ts", 1, "g1");
    codeWorkbenchState.record("b", "b.ts", 2, "g2");
    expect(codeWorkbenchState.entriesFor("a")).toHaveLength(1);
    expect(codeWorkbenchState.entriesFor("b")).toHaveLength(1);
    expect(codeWorkbenchState.canNavigate("a", -1)).toBe(false);
  });

  it("normalizes and stores contextual layout", () => {
    expect(normalizeCodeWorkbenchLayout(null)).toEqual({
      context_panel: null,
      terminal: false,
      tests: false,
      search: false,
      changes: false,
      primary_task: null,
    });
    expect(
      normalizeCodeWorkbenchLayout({
        context_panel: "problems",
        terminal: true,
        tests: 1,
        junk: true,
      }),
    ).toEqual({
      context_panel: "problems",
      terminal: true,
      tests: false,
      search: false,
      changes: false,
      primary_task: null,
    });

    codeWorkbenchState.applyLayout("work-1", {
      context_panel: "outline",
      terminal: true,
    });
    expect(codeWorkbenchState.layoutFor("work-1")).toEqual({
      context_panel: "outline",
      terminal: true,
      tests: false,
      search: false,
      changes: false,
      primary_task: null,
    });
    codeWorkbenchState.setTestsOpen("work-1", true);
    expect(codeWorkbenchState.layoutFor("work-1").tests).toBe(true);
    codeWorkbenchState.setSearchOpen("work-1", true);
    expect(codeWorkbenchState.layoutFor("work-1").search).toBe(true);
    codeWorkbenchState.setChangesOpen("work-1", true);
    expect(codeWorkbenchState.layoutFor("work-1").changes).toBe(true);
    codeWorkbenchState.setPrimaryTask("work-1", "npm-dev");
    expect(codeWorkbenchState.layoutFor("work-1").primary_task).toBe("npm-dev");
  });

  it("lists group-local Code tabs by composing shell and LME identities", () => {
    const visible = visibleCodeTabsInGroup({
      workId: "work-1",
      groupTabs: [
        { id: "shell-a", kind: "lme", lmeTabId: "code-file:work-1:a.ts" },
        { id: "shell-chat", kind: "chat" },
        { id: "shell-other", kind: "lme", lmeTabId: "code-file:work-2:b.ts" },
        { id: "shell-b", kind: "lme", lmeTabId: "code-file:work-1:b.ts" },
      ],
      lmeTabs: [
        {
          tabId: "code-file:work-1:a.ts",
          kind: "code",
          workId: "work-1",
          resource: { kind: "file", path: "a.ts" },
        },
        {
          tabId: "code-file:work-1:b.ts",
          kind: "code",
          workId: "work-1",
          resource: { kind: "file", path: "b.ts" },
        },
        {
          tabId: "code-file:work-2:b.ts",
          kind: "code",
          workId: "work-2",
          resource: { kind: "file", path: "b.ts" },
        },
      ],
    });
    expect(visible).toEqual([
      {
        shellTabId: "shell-a",
        lmeTabId: "code-file:work-1:a.ts",
        path: "a.ts",
      },
      {
        shellTabId: "shell-b",
        lmeTabId: "code-file:work-1:b.ts",
        path: "b.ts",
      },
    ]);
  });
});
