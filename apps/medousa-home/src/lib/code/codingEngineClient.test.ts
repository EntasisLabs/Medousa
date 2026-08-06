import { describe, expect, it } from "vitest";
import { codeWorkspaceLspPoolKey } from "./codingEngineClient";

describe("coding-engine workspace client identity", () => {
  it("reuses a project-language client for equivalent root URIs", () => {
    expect(
      codeWorkspaceLspPoolKey(
        "work-1",
        "TypeScript",
        "FILE://LOCALHOST/repo/packages/app",
      ),
    ).toBe(
      codeWorkspaceLspPoolKey(
        "work-1",
        "typescript",
        "file:///repo/packages/app",
      ),
    );
  });

  it("keeps nested language roots and governed projects isolated", () => {
    const app = codeWorkspaceLspPoolKey(
      "work-1",
      "typescript",
      "file:///repo/packages/app",
    );
    expect(
      codeWorkspaceLspPoolKey(
        "work-1",
        "typescript",
        "file:///repo/packages/api",
      ),
    ).not.toBe(app);
    expect(
      codeWorkspaceLspPoolKey(
        "work-2",
        "typescript",
        "file:///repo/packages/app",
      ),
    ).not.toBe(app);
  });
});
