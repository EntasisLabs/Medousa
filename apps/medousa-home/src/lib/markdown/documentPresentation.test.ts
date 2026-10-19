/** @vitest-environment happy-dom */
import { describe, expect, it } from "vitest";
import { presentDocumentMarkdown } from "./documentPresentation";

describe("document Markdown presentation", () => {
  it("groups native disclosures using Liquid's classes and opens the first section", () => {
    const root = document.createElement("div");
    root.innerHTML = presentDocumentMarkdown('<p>Introduction</p><details><summary>Release <strong>notes</strong></summary><p>Real release text</p></details>\n<details><summary>Commits</summary><pre><code>commit</code></pre></details><p>Closing prose</p><details><summary>Commands</summary><p>Help</p></details>');
    expect(root.querySelectorAll(".liquid-accordion")).toHaveLength(2);
    expect(root.querySelector("details")?.open).toBe(true);
    expect(root.querySelectorAll("details[open]")).toHaveLength(1);
    expect(root.querySelector("summary strong")?.textContent).toBe("notes");
    expect(root.querySelector(".liquid-accordion-panel p")?.textContent).toBe("Real release text");
    expect(root.querySelector("code")?.textContent).toBe("commit");
    expect([...root.children].map(el => el.tagName)).toEqual(["P", "DIV", "P", "DIV"]);
    expect(root.querySelector("summary .liquid-accordion-chevron")?.getAttribute("aria-hidden")).toBe("true");
  });
  it("preserves authored open sections and nested disclosure structure", () => {
    const root = document.createElement("div");
    root.innerHTML = presentDocumentMarkdown('<details><summary>First</summary><p>First body</p></details><details open><summary>Second</summary><details><summary>Nested</summary><p>Nested body</p></details></details>');
    expect(root.querySelector("details")?.open).toBe(false);
    expect(root.querySelectorAll("details[open]")).toHaveLength(1);
    expect(root.querySelector("details[open] .liquid-accordion-panel details p")?.textContent).toBe("Nested body");
  });
  it("does not move text between adjacent elements into a disclosure group", () => {
    const html = presentDocumentMarkdown('<details><summary>First</summary><p>One</p></details>Meaningful text<details><summary>Second</summary><p>Two</p></details>');
    const root = document.createElement("div"); root.innerHTML = html;
    expect(root.querySelectorAll(".liquid-accordion")).toHaveLength(2);
    expect(root.childNodes[1].textContent).toBe("Meaningful text");
  });
});
