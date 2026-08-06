import { describe, expect, it } from "vitest";

import { isHttpDaemonBase } from "$lib/code/taskPreviewUrl";

describe("task preview url", () => {
  it("accepts http workshop bases for proxy handoff", () => {
    expect(isHttpDaemonBase("http://192.168.1.10:7420")).toBe(true);
    expect(isHttpDaemonBase("https://workshop.example")).toBe(true);
    expect(isHttpDaemonBase("iroh://ticket")).toBe(false);
    expect(isHttpDaemonBase("")).toBe(false);
  });
});
