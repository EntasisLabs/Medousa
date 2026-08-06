import { describe, expect, it } from "vitest";
import {
  canonicalCodeDocumentUri,
  pathToFileUri,
  workspaceRelativePathFromUri,
} from "./codeDocumentUri";

describe("code document URIs", () => {
  it("encodes file path characters that would otherwise become URL syntax", () => {
    expect(pathToFileUri("/remote/repo/a b#draft?.ts")).toBe(
      "file:///remote/repo/a%20b%23draft%3F.ts",
    );
    expect(pathToFileUri("C:\\repo\\a b.ts")).toBe(
      "file:///C:/repo/a%20b.ts",
    );
    expect(pathToFileUri("\\\\server\\share\\a b.ts")).toBe(
      "file://server/share/a%20b.ts",
    );
  });

  it("canonicalizes equivalent file URI spellings", () => {
    expect(canonicalCodeDocumentUri("FILE:///Repo/a b.ts?x=1#symbol")).toBe(
      "file:///Repo/a%20b.ts",
    );
    expect(canonicalCodeDocumentUri("file://LOCALHOST/Repo/a.ts")).toBe(
      "file:///Repo/a.ts",
    );
  });

  it("resolves only descendants of the authoritative workshop root", () => {
    expect(
      workspaceRelativePathFromUri(
        "file:///remote/repo/src/a%20b.ts",
        "/remote/repo",
      ),
    ).toBe("src/a b.ts");
    expect(
      workspaceRelativePathFromUri("file:///remote/repository/a.ts", "/remote/repo"),
    ).toBe(null);
    expect(
      workspaceRelativePathFromUri("file:///remote/repo", "/remote/repo"),
    ).toBe(null);
    expect(
      workspaceRelativePathFromUri("https://example.test/a.ts", "/remote/repo"),
    ).toBe(null);
  });

  it("supports Windows and UNC workshop roots without consulting Home paths", () => {
    expect(
      workspaceRelativePathFromUri("file:///c:/Repo/src/a.ts", "C:\\Repo"),
    ).toBe("src/a.ts");
    expect(
      workspaceRelativePathFromUri(
        "file://SERVER/share/repo/src/a.ts",
        "\\\\server\\share\\repo",
      ),
    ).toBe("src/a.ts");
  });

  it("rejects encoded separators that could change project boundaries", () => {
    expect(
      workspaceRelativePathFromUri(
        "file:///remote/repo/src%2Foutside.ts",
        "/remote/repo",
      ),
    ).toBe(null);
  });
});
