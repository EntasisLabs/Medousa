import { describe, expect, it } from "vitest";

// parseFeedBody is not exported; exercise via preprocessLiquidEmbeds placeholder path.
import { decodeLiquidProps, preprocessLiquidEmbeds } from "$lib/markdown/liquidEmbeds";

describe("feed fence", () => {
  it("parses id and datatype into feed embed props", () => {
    const md = preprocessLiquidEmbeds(
      "```feed\nid: summer-ai-digest\ndatatype: md\ntitle: Summer digest\nrefresh: manual\n```",
    );
    expect(md).toContain('data-liquid-embed="feed"');
    const match = md.match(/data-liquid-props="([^"]+)"/);
    expect(match).toBeTruthy();
    const props = decodeLiquidProps<{
      feedId: string;
      datatype: string;
      title?: string;
      refresh?: string;
    }>(match![1]!);
    expect(props?.feedId).toBe("summer-ai-digest");
    expect(props?.datatype).toBe("md");
    expect(props?.title).toBe("Summer digest");
    expect(props?.refresh).toBe("manual");
  });
});
