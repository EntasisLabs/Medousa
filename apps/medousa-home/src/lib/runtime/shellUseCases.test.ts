import { afterEach, describe, expect, it } from "vitest";
import {
  openVaultNote,
  openWorkCard,
  setShellUseCasePortsForTests,
} from "./shellUseCases";

afterEach(() => {
  setShellUseCasePortsForTests({});
});

describe("shell use cases", () => {
  it("openVaultNote uses injected ports instead of vault stores", async () => {
    const opened: string[] = [];
    setShellUseCasePortsForTests({
      openVaultNote: async (path) => {
        opened.push(path);
      },
    });
    await openVaultNote("notes/a.md");
    expect(opened).toEqual(["notes/a.md"]);
  });

  it("openWorkCard uses injected ports instead of workspace store", async () => {
    const opened: string[] = [];
    setShellUseCasePortsForTests({
      openWorkCard: async (id) => {
        opened.push(id);
      },
    });
    await openWorkCard("card-1");
    expect(opened).toEqual(["card-1"]);
  });
});
