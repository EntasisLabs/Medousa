import { afterEach, describe, expect, it } from "vitest";
import {
  getLmeDockHost,
  popLmeDockHost,
  pushLmeDockHost,
  setLmeDockHost,
} from "./lmeDockHost";

describe("lmeDockHost stack", () => {
  afterEach(() => {
    setLmeDockHost(null);
    popLmeDockHost();
    setLmeDockHost(null);
  });

  it("restores the previous host after push/pop", () => {
    const status = { id: "status" } as unknown as HTMLElement;
    const popover = { id: "popover" } as unknown as HTMLElement;
    setLmeDockHost(status);
    expect(getLmeDockHost()).toBe(status);

    pushLmeDockHost(popover);
    expect(getLmeDockHost()).toBe(popover);

    popLmeDockHost();
    expect(getLmeDockHost()).toBe(status);
  });
});
