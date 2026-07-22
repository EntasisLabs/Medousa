/** @vitest-environment happy-dom */
import { describe, expect, it } from "vitest";
import { resolveMedousaViewHostClick } from "./medousaViewHostClick";

function mountViewChrome() {
  const root = document.createElement("div");
  root.className = "medousa-view";
  root.setAttribute("data-edit-view-index", "3");
  root.innerHTML = `
    <button type="button" class="medousa-view-copy-csv" data-copy-view-csv="a%2Cb" data-view-csv="a%2Cb">Copy CSV</button>
    <button type="button" class="medousa-view-configure" data-edit-view-index="3">Configure</button>
    <table><tbody><tr><td class="cell">x</td></tr></tbody></table>
  `;
  document.body.appendChild(root);
  return root;
}

describe("resolveMedousaViewHostClick", () => {
  it("prefers Copy CSV over wrapper data-edit-view-index", () => {
    const root = mountViewChrome();
    const csv = root.querySelector("[data-copy-view-csv]")!;
    expect(resolveMedousaViewHostClick(csv)).toEqual({
      kind: "copyCsv",
      payload: "a%2Cb",
    });
    root.remove();
  });

  it("resolves Configure from the configure button only", () => {
    const root = mountViewChrome();
    const btn = root.querySelector(".medousa-view-configure")!;
    expect(resolveMedousaViewHostClick(btn)).toEqual({
      kind: "configure",
      index: 3,
    });
    root.remove();
  });

  it("ignores clicks on the table body even when wrapper has data-edit-view-index", () => {
    const root = mountViewChrome();
    const cell = root.querySelector(".cell")!;
    expect(resolveMedousaViewHostClick(cell)).toEqual({ kind: "none" });
    root.remove();
  });

  it("returns none for null / non-element", () => {
    expect(resolveMedousaViewHostClick(null)).toEqual({ kind: "none" });
  });
});
