/** @vitest-environment happy-dom */
import { describe, expect, it } from "vitest";
import { attachHostCollapse } from "./liveHostCollapse";

describe("attachHostCollapse", () => {
  it("toggles collapsed class and restores on expand", () => {
    const host = document.createElement("div");
    host.innerHTML = "<table><tr><td>x</td></tr></table>";
    document.body.appendChild(host);
    const dispose = attachHostCollapse(host, { label: "Table" });
    expect(host.classList.contains("vault-live-collapsible-host")).toBe(true);
    const btn = host.querySelector(".vault-live-host-collapse-btn") as HTMLButtonElement;
    expect(btn).toBeTruthy();
    btn.click();
    expect(host.classList.contains("vault-live-collapsible-host--collapsed")).toBe(true);
    btn.click();
    expect(host.classList.contains("vault-live-collapsible-host--collapsed")).toBe(false);
    dispose();
    expect(host.querySelector(".vault-live-host-collapse-bar")).toBeNull();
    host.remove();
  });

  it("no-ops for missing host", () => {
    expect(() => attachHostCollapse(null)).not.toThrow();
  });
});
