import { describe, expect, it } from "vitest";
import { liveDocToMarkdown } from "./liveDocToMarkdown";
import { markdownToLiveDoc } from "./markdownToLiveDoc";

describe("Live GFM tables", () => {
  it("round-trips a pipe table through Live JSON", () => {
    const md = `| name | status |
| ---- | ------ |
| alpha | open |
| beta | done |
`;
    const doc = markdownToLiveDoc(md);
    const table = doc.content?.find((node) => node.type === "table");
    expect(table).toBeTruthy();
    expect(table?.content?.length).toBe(3);

    const back = liveDocToMarkdown(doc);
    expect(back).toContain("| name");
    expect(back).toContain("alpha");
    expect(back).toContain("beta");
  });
});
