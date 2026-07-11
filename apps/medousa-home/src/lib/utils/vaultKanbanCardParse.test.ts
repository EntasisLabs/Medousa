import { describe, expect, it } from "vitest";
import { parseKanbanCardText, parseWikilinkInner } from "./vaultKanbanCardParse";

describe("parseWikilinkInner", () => {
  it("uses alias when present", () => {
    expect(parseWikilinkInner("projects/bit-llm|Bit LLM")).toEqual({
      target: "projects/bit-llm",
      label: "Bit LLM",
    });
  });

  it("falls back to basename", () => {
    expect(parseWikilinkInner("projects/bit-llm")).toEqual({
      target: "projects/bit-llm",
      label: "bit-llm",
    });
  });
});

describe("parseKanbanCardText", () => {
  it("splits title and body on newline", () => {
    const view = parseKanbanCardText("Ship kanban polish\nMake chips navigable");
    expect(view.title).toBe("Ship kanban polish");
    expect(view.body).toBe("Make chips navigable");
    expect(view.hasDepth).toBe(true);
  });

  it("splits title — detail on one line", () => {
    const view = parseKanbanCardText("Filter bug — not clearing after click");
    expect(view.title).toBe("Filter bug");
    expect(view.body).toBe("not clearing after click");
  });

  it("extracts emoji, wikilinks, and tags", () => {
    const view = parseKanbanCardText("🚀 [[Polish Places]] #vault #board");
    expect(view.emoji).toBe("🚀");
    expect(view.wikilinks).toEqual([{ target: "Polish Places", label: "Polish Places" }]);
    expect(view.tags).toEqual(["vault", "board"]);
    expect(view.wikilinkPrimary).toBe(true);
    expect(view.title).toBe("Polish Places");
  });

  it("renders alias in title instead of raw wiki syntax", () => {
    const view = parseKanbanCardText("[[projects/bit-llm|Bit LLM]] #medousa");
    expect(view.title).toBe("Bit LLM");
    expect(view.wikilinks[0]).toEqual({
      target: "projects/bit-llm",
      label: "Bit LLM",
    });
    expect(view.wikilinkPrimary).toBe(true);
  });

  it("detects markdown images", () => {
    const view = parseKanbanCardText("Moodboard\n![](./shots/hero.png)");
    expect(view.imageHref).toBe("./shots/hero.png");
  });
});
