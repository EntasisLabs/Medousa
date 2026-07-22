import { describe, expect, it } from "vitest";
import { matchVaultHotkey } from "./vaultHotkeys";

function key(
  keyName: string,
  mods: { meta?: boolean; ctrl?: boolean; shift?: boolean; alt?: boolean } = {},
): KeyboardEvent {
  return {
    key: keyName,
    metaKey: Boolean(mods.meta),
    ctrlKey: Boolean(mods.ctrl),
    shiftKey: Boolean(mods.shift),
    altKey: Boolean(mods.alt),
  } as KeyboardEvent;
}

describe("matchVaultHotkey", () => {
  it("matches save / find / new note / plane / export / board", () => {
    expect(matchVaultHotkey(key("s", { meta: true }))).toBe("save");
    expect(matchVaultHotkey(key("f", { ctrl: true }))).toBe("find");
    expect(matchVaultHotkey(key("n", { meta: true }))).toBe("newNote");
    expect(matchVaultHotkey(key("n", { ctrl: true }))).toBe("newNote");
    expect(matchVaultHotkey(key("e", { meta: true, shift: true }))).toBe("togglePlane");
    expect(matchVaultHotkey(key("p", { meta: true, shift: true }))).toBe("exportPdf");
    expect(matchVaultHotkey(key("b", { meta: true, shift: true }))).toBe("toggleBoard");
  });

  it("matches bare e / Escape", () => {
    expect(matchVaultHotkey(key("e"))).toBe("enterEdit");
    expect(matchVaultHotkey(key("Escape"))).toBe("enterPreview");
  });
});
