/** @vitest-environment happy-dom */
import { describe, expect, it, vi } from "vitest";

vi.mock("./liveOrganismHost", () => ({
  mountLiquidFence: vi.fn(),
  unmountLiquidFence: vi.fn(),
}));

import { mountTimelineSurface } from "./liveTimelineSurface";

const TIMELINE_RAW = `\`\`\`timeline
title: Trip log
layout: snapshot

---
ts: Day 1
title: Arrive
body: Landed and checked in
emoji: ✈️
---
ts: Day 2
title: Explore
body: Markets and food
emoji: 🍜
\`\`\`
`;

describe("mountTimelineSurface", () => {
  it("exposes timeline configure for the liquid bridge", () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    const handles = mountTimelineSurface(host, TIMELINE_RAW, {}, () => {});
    const btn = host.querySelector<HTMLButtonElement>(
      "[data-live-liquid-configure][data-live-liquid-lang='timeline']",
    );
    expect(btn?.textContent).toBe("timeline");
    expect(host.querySelector(".vault-live-timeline__title")?.textContent).toBe(
      "Trip log",
    );
    handles.destroy();
    host.remove();
  });

  it("toggles layout and writes fence raw", () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    const onChange = vi.fn<(raw: string) => void>();
    const handles = mountTimelineSurface(host, TIMELINE_RAW, {}, onChange);

    const rail = host.querySelector<HTMLButtonElement>(
      '.vault-live-timeline__layout[data-layout="rail"]',
    );
    expect(rail).toBeTruthy();
    rail?.click();

    expect(onChange).toHaveBeenCalled();
    const next = onChange.mock.calls.at(-1)?.[0] ?? "";
    expect(next).toContain("```timeline");
    expect(next).not.toContain("layout: snapshot");

    handles.destroy();
    host.remove();
  });
});
