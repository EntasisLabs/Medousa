/** @vitest-environment happy-dom */
import { describe, expect, it, vi } from "vitest";

vi.mock("./liveOrganismHost", () => ({
  mountLiquidFence: vi.fn(),
  unmountLiquidFence: vi.fn(),
}));

import { mountCompareSurface } from "./liveCompareSurface";

const COMPARE_RAW = `\`\`\`compare
title: Head to head
subtitle: Live polish
mode: faceoff
recommendation: Alpha

| | Alpha | Beta |
| --- | --- | --- |
| Speed | Fast | Slow |
\`\`\`
`;

describe("mountCompareSurface", () => {
  it("exposes table configure for the liquid bridge", () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    const handles = mountCompareSurface(host, COMPARE_RAW, {}, () => {});
    const btn = host.querySelector<HTMLButtonElement>(
      "[data-live-liquid-configure][data-live-liquid-lang='compare']",
    );
    expect(btn?.textContent).toBe("table");
    expect(host.querySelector(".vault-live-compare__title")?.textContent).toBe(
      "Head to head",
    );
    handles.destroy();
    host.remove();
  });

  it("toggles mode and writes fence raw", () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    const onChange = vi.fn<(raw: string) => void>();
    const handles = mountCompareSurface(host, COMPARE_RAW, {}, onChange);

    const matrix = host.querySelector<HTMLButtonElement>(
      '.vault-live-compare__mode[data-mode="matrix"]',
    );
    expect(matrix).toBeTruthy();
    matrix?.click();

    expect(onChange).toHaveBeenCalled();
    const next = onChange.mock.calls.at(-1)?.[0] ?? "";
    expect(next).toContain("```compare");
    expect(next).not.toContain("mode: faceoff");

    handles.destroy();
    host.remove();
  });
});
