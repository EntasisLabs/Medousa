import { describe, expect, it } from "vitest";
import { codeFileIconForPath, codeFileIconSrc } from "./codeFileIcons";

describe("codeFileIconForPath", () => {
  it("maps language files like Material Icon Theme", () => {
    expect(codeFileIconForPath("src/main.rs").id).toBe("rust");
    expect(codeFileIconForPath("lib/util.ts").id).toBe("typescript");
    expect(codeFileIconForPath("app/page.tsx").id).toBe("react_ts");
    expect(codeFileIconForPath("Button.svelte").id).toBe("svelte");
    expect(codeFileIconForPath("scripts/run.sh").id).toBe("console");
  });

  it("prefers special filenames", () => {
    expect(codeFileIconForPath("package.json").id).toBe("nodejs");
    expect(codeFileIconForPath("Dockerfile").id).toBe("docker");
    expect(codeFileIconForPath(".gitignore").id).toBe("git");
    expect(codeFileIconForPath("README.md").id).toBe("readme");
    expect(codeFileIconForPath("LICENSE").id).toBe("license");
    expect(codeFileIconForPath("CONTRIBUTING.md").id).toBe("contributing");
  });

  it("uses toml for Cargo.toml", () => {
    expect(codeFileIconForPath("Cargo.toml").id).toBe("toml");
  });

  it("falls back to generic file", () => {
    expect(codeFileIconForPath("notes.weird").id).toBe("file");
    expect(codeFileIconSrc("rust")).toBe("/file-icons/rust.svg");
  });
});
