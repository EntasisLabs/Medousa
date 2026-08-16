import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const homeRoot = join(dirname(fileURLToPath(import.meta.url)), "../../..");

describe("ShellPane destination loaders", () => {
  it("statically imports chat chrome and not dormant destination panels", () => {
    const source = readFileSync(
      join(homeRoot, "src/lib/components/shell/ShellPane.svelte"),
      "utf8",
    );
    expect(source).toMatch(/import ChatSessionView from/);
    expect(source).toMatch(/import ChatPaneIdle from/);
    expect(source).not.toMatch(/import LmePanel from/);
    expect(source).not.toMatch(/import WorkPanel from/);
    expect(source).not.toMatch(/import HumanBrowserPanel from/);
    expect(source).not.toMatch(/import SettingsPanel from/);
    expect(source).not.toMatch(/import TerminalPane from/);
    expect(source).not.toMatch(/import CalendarPanel from/);
    expect(source).toContain("loadLmePanel");
    expect(source).not.toMatch(/void import\(/);
    expect(source).not.toMatch(/onMount\(/);
  });
});
