import { describe, expect, it } from "vitest";

import { classifyCspBlockedSource } from "./cspDiagnostics";

describe("CSP diagnostics", () => {
  it("reports only a safe source class", () => {
    expect(classifyCspBlockedSource("https://example.test/private?token=secret")).toBe("https");
    expect(classifyCspBlockedSource("file:///Users/alice/private.txt")).toBe("other-scheme");
    expect(classifyCspBlockedSource("inline")).toBe("inline");
  });
});
