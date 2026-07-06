import { describe, expect, it } from "vitest";

import { terminalWorkerCardsNeedingRecovery } from "$lib/utils/workerSynthesisRecovery";
import type { WorkCard } from "$lib/types/workspace";

function card(id: string, column: WorkCard["column"]): WorkCard {
  return {
    id,
    column,
    title: id,
    status_label: column,
    created_at_utc: "2026-01-01T00:00:00Z",
    updated_at_utc: "2026-01-01T00:00:00Z",
  };
}

describe("terminalWorkerCardsNeedingRecovery", () => {
  it("includes cards that newly reached a terminal column", () => {
    const cards = [card("work-1", "done")];
    const previous = new Map([["work-1", "in_flight"]]);
    const pending = new Set<string>();

    expect(terminalWorkerCardsNeedingRecovery(cards, previous, pending)).toEqual(
      cards,
    );
  });

  it("includes terminal cards with pending synthesis even when column unchanged", () => {
    const cards = [card("work-2", "done")];
    const previous = new Map([["work-2", "done"]]);
    const pending = new Set(["work-2"]);

    expect(terminalWorkerCardsNeedingRecovery(cards, previous, pending)).toEqual(
      cards,
    );
  });

  it("excludes settled terminal cards with no column change", () => {
    const cards = [card("work-3", "done")];
    const previous = new Map([["work-3", "done"]]);
    const pending = new Set<string>();

    expect(terminalWorkerCardsNeedingRecovery(cards, previous, pending)).toEqual(
      [],
    );
  });

  it("excludes ask job cards", () => {
    const cards = [card("medousa-daemon-ask-abc", "done")];
    const previous = new Map([["medousa-daemon-ask-abc", "in_flight"]]);
    const pending = new Set(["medousa-daemon-ask-abc"]);

    expect(terminalWorkerCardsNeedingRecovery(cards, previous, pending)).toEqual(
      [],
    );
  });

  it("includes all pending terminal cards beyond the old 12-card cap", () => {
    const cards = Array.from({ length: 15 }, (_, index) =>
      card(`work-${index}`, "done"),
    );
    const previous = new Map(
      cards.map((item) => [item.id, "in_flight"] as const),
    );
    const pending = new Set(cards.map((item) => item.id));

    expect(terminalWorkerCardsNeedingRecovery(cards, previous, pending)).toHaveLength(
      15,
    );
  });
});
