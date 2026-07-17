/** @vitest-environment happy-dom */
import { describe, expect, it, vi } from "vitest";
import { mountCardSurface } from "./liveCardSurface";
import type { CardDetailPayload } from "$lib/markdown/liquidEmbeds";

const CARD_RAW = `\`\`\`card
title: Grok 4.5
subtitle: SpaceXAI flagship
emoji: ⚡
body: Token-efficient frontier.
meta: $2 / $6 · 500K
point: Cutoff | February 1, 2026
\`\`\`
`;

describe("mountCardSurface", () => {
  it("opens card detail from the open control", () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    const onOpen = vi.fn<(detail: CardDetailPayload) => void>();
    const handles = mountCardSurface(host, CARD_RAW, () => {}, onOpen);

    const expand = host.querySelector<HTMLButtonElement>(".vault-live-card__expand");
    expect(expand).toBeTruthy();
    expect(expand?.hidden).toBe(false);
    expand?.click();

    expect(onOpen).toHaveBeenCalledTimes(1);
    const detail = onOpen.mock.calls[0]![0];
    expect(detail.title).toBe("Grok 4.5");
    expect(detail.meta).toContain("500K");
    expect(detail.points?.length).toBe(1);

    handles.destroy();
    host.remove();
  });

  it("keeps more configure button for the liquid bridge", () => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    const handles = mountCardSurface(host, CARD_RAW, () => {}, () => {});
    const more = host.querySelector<HTMLButtonElement>(
      "[data-live-liquid-configure][data-live-liquid-lang='card']",
    );
    expect(more?.textContent).toBe("more");
    handles.destroy();
    host.remove();
  });
});
