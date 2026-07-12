import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { preprocessLiquidEmbeds } from "./liquidEmbeds";

const mountMock = vi.fn(() => ({}));
const unmountMock = vi.fn(async () => {});

vi.mock("svelte", async () => {
  const actual = await vi.importActual<typeof import("svelte")>("svelte");
  return {
    ...actual,
    mount: (...args: unknown[]) => mountMock(...args),
    unmount: (...args: unknown[]) => unmountMock(...args),
  };
});

vi.mock("./LiquidMdHost.svelte", () => ({
  default: { name: "LiquidMdHost" },
}));

type FakeEl = {
  dataset: Record<string, string | undefined>;
  children: FakeEl[];
  querySelectorAll: (sel: string) => FakeEl[];
  replaceChildren: () => void;
};

function el(attrs: Record<string, string> = {}): FakeEl {
  const dataset: Record<string, string | undefined> = {};
  for (const [k, v] of Object.entries(attrs)) {
    if (k.startsWith("data-")) {
      // data-liquid-embed → liquidEmbed
      const key = k
        .slice(5)
        .split("-")
        .map((part, i) => (i === 0 ? part : part[0].toUpperCase() + part.slice(1)))
        .join("");
      dataset[key] = v;
    }
  }
  const node: FakeEl = {
    dataset,
    children: [],
    querySelectorAll(sel: string) {
      const out: FakeEl[] = [];
      const walk = (n: FakeEl) => {
        for (const child of n.children) {
          if (sel === "[data-liquid-embed]" && child.dataset.liquidEmbed) out.push(child);
          if (sel === "[data-liquid-icon]" && child.dataset.liquidIcon) out.push(child);
          walk(child);
        }
      };
      walk(node);
      return out;
    },
    replaceChildren() {
      this.children = [];
    },
  };
  return node;
}

function treeFromPlaceholders(html: string): FakeEl {
  const root = el();
  const embedRe =
    /<(div|span)\s+class="[^"]*"\s+data-liquid-(embed|icon)="([^"]+)"(?:\s+data-liquid-props="([^"]*)")?[^>]*>/g;
  let match: RegExpExecArray | null;
  while ((match = embedRe.exec(html)) !== null) {
    const [, , kindAttr, value, props] = match;
    if (kindAttr === "embed") {
      const child = el({
        "data-liquid-embed": value,
        ...(props ? { "data-liquid-props": props } : {}),
      });
      root.children.push(child);
    } else {
      root.children.push(el({ "data-liquid-icon": value }));
    }
  }
  return root;
}

describe("hydrateLiquidEmbeds", () => {
  beforeEach(() => {
    // hydrateLiquidEmbeds no-ops without window
    (globalThis as { window?: unknown }).window = globalThis;
  });

  afterEach(() => {
    mountMock.mockClear();
    unmountMock.mockClear();
    delete (globalThis as { window?: unknown }).window;
  });

  it("mounts card, carousel, actions, and icon hosts from fixture markdown", async () => {
    const md = [
      "{{icon:sparkles}} Models",
      "",
      "```card",
      "title: Sol",
      "body: Flagship",
      "emoji: 🧠",
      "```",
      "",
      "```carousel",
      "title: Sol | body: Flagship | emoji: 🧠",
      "title: Terra | body: Mid | emoji: ⚖️",
      "```",
      "",
      "```actions",
      "Which one is best for coding? | coding",
      "Compare Sol vs Terra | compare",
      "```",
    ].join("\n");

    const html = preprocessLiquidEmbeds(md);
    expect(html).toContain('data-liquid-embed="card"');
    expect(html).toContain('data-liquid-embed="carousel"');
    expect(html).toContain('data-liquid-embed="actions"');
    expect(html).toContain('data-liquid-icon="sparkles"');

    const root = treeFromPlaceholders(html) as unknown as HTMLElement;
    const { hydrateLiquidEmbeds, destroyLiquidEmbeds } = await import("./hydrateLiquidEmbeds");
    hydrateLiquidEmbeds(root, { openLinksInWeb: false });

    expect(mountMock).toHaveBeenCalledTimes(4);

    const kinds = mountMock.mock.calls.map((call) => {
      const props = (call[1] as { props: { kind: string; payload: unknown } }).props;
      return props.kind;
    });
    expect(kinds).toEqual(["card", "carousel", "actions", "icon"]);

    const cardCall = mountMock.mock.calls.find(
      (call) => (call[1] as { props: { kind: string } }).props.kind === "card",
    );
    expect((cardCall![1] as { props: { payload: { title: string } } }).props.payload.title).toBe(
      "Sol",
    );

    const carouselCall = mountMock.mock.calls.find(
      (call) => (call[1] as { props: { kind: string } }).props.kind === "carousel",
    );
    const carouselPayload = (carouselCall![1] as { props: { payload: { items: unknown[] } } })
      .props.payload;
    expect(carouselPayload.items).toHaveLength(2);

    const actionsCall = mountMock.mock.calls.find(
      (call) => (call[1] as { props: { kind: string } }).props.kind === "actions",
    );
    const actionsPayload = (
      actionsCall![1] as { props: { payload: { actions: { intent?: string }[] } } }
    ).props.payload;
    expect(actionsPayload.actions[0].intent).toBe("coding");

    destroyLiquidEmbeds(root);
    expect(unmountMock).toHaveBeenCalledTimes(4);
  });

  it("mounts wave-1 embeds from fixture markdown", async () => {
    const md = [
      "```callout",
      "tone: note",
      "body: Aside text",
      "```",
      "",
      "```section",
      "title: Family",
      "---",
      "Prose inside",
      "```",
      "",
      "```chips",
      "Ultra | tone: accent",
      "Fast",
      "```",
      "",
      "```media",
      "src: https://example.com/x.png",
      "alt: X",
      "```",
    ].join("\n");

    const html = preprocessLiquidEmbeds(md);
    const root = treeFromPlaceholders(html) as unknown as HTMLElement;
    const { hydrateLiquidEmbeds, destroyLiquidEmbeds } = await import("./hydrateLiquidEmbeds");
    hydrateLiquidEmbeds(root, {});

    const kinds = mountMock.mock.calls.map(
      (call) => (call[1] as { props: { kind: string } }).props.kind,
    );
    expect(kinds).toEqual(["callout", "section", "chips", "media"]);
    destroyLiquidEmbeds(root);
    expect(unmountMock).toHaveBeenCalledTimes(4);
  });

  it("mounts cite host from fixture markdown", async () => {
    const md = [
      "```cite",
      "title: Source",
      "url: https://example.com",
      "quote: Excerpt",
      "source: web search",
      "```",
    ].join("\n");

    const html = preprocessLiquidEmbeds(md);
    expect(html).toContain('data-liquid-embed="cite"');
    const root = treeFromPlaceholders(html) as unknown as HTMLElement;
    const { hydrateLiquidEmbeds, destroyLiquidEmbeds } = await import("./hydrateLiquidEmbeds");
    hydrateLiquidEmbeds(root, {});

    const kinds = mountMock.mock.calls.map(
      (call) => (call[1] as { props: { kind: string } }).props.kind,
    );
    expect(kinds).toEqual(["cite"]);
    const citeCall = mountMock.mock.calls.find(
      (call) => (call[1] as { props: { kind: string } }).props.kind === "cite",
    );
    const citePayload = (citeCall![1] as { props: { payload: { quote?: string; title?: string } } })
      .props.payload;
    expect(citePayload.quote).toBe("Excerpt");
    expect(citePayload.title).toBe("Source");
    destroyLiquidEmbeds(root);
    expect(unmountMock).toHaveBeenCalledTimes(1);
  });

  it("mounts compare host from fixture markdown", async () => {
    const md = [
      "```compare",
      "title: Laptops",
      "recommendation: Sol",
      "",
      "| | Sol | Terra |",
      "| --- | --- | --- |",
      "| Speed | Fast | Faster |",
      "```",
    ].join("\n");

    const html = preprocessLiquidEmbeds(md);
    expect(html).toContain('data-liquid-embed="compare"');
    const root = treeFromPlaceholders(html) as unknown as HTMLElement;
    const { hydrateLiquidEmbeds, destroyLiquidEmbeds } = await import("./hydrateLiquidEmbeds");
    hydrateLiquidEmbeds(root, {});

    const kinds = mountMock.mock.calls.map(
      (call) => (call[1] as { props: { kind: string } }).props.kind,
    );
    expect(kinds).toEqual(["compare"]);
    const compareCall = mountMock.mock.calls.find(
      (call) => (call[1] as { props: { kind: string } }).props.kind === "compare",
    );
    const comparePayload = (
      compareCall![1] as {
        props: {
          payload: {
            title?: string;
            entities: { label: string }[];
            axes: { label: string }[];
          };
        };
      }
    ).props.payload;
    expect(comparePayload.title).toBe("Laptops");
    expect(comparePayload.entities.map((e) => e.label)).toEqual(["Sol", "Terra"]);
    expect(comparePayload.axes.map((a) => a.label)).toEqual(["Speed"]);
    destroyLiquidEmbeds(root);
    expect(unmountMock).toHaveBeenCalledTimes(1);
  });

  it("mounts plan host from fixture markdown", async () => {
    const md = [
      "```plan",
      "title: Trip flow",
      "",
      "---",
      "label: Arrive",
      "time: Day 1",
      "---",
      "label: Explore",
      "time: Days 2–4",
      "```",
    ].join("\n");

    const html = preprocessLiquidEmbeds(md);
    expect(html).toContain('data-liquid-embed="plan"');
    const root = treeFromPlaceholders(html) as unknown as HTMLElement;
    const { hydrateLiquidEmbeds, destroyLiquidEmbeds } = await import("./hydrateLiquidEmbeds");
    hydrateLiquidEmbeds(root, {});

    const kinds = mountMock.mock.calls.map(
      (call) => (call[1] as { props: { kind: string } }).props.kind,
    );
    expect(kinds).toEqual(["plan"]);
    const planCall = mountMock.mock.calls.find(
      (call) => (call[1] as { props: { kind: string } }).props.kind === "plan",
    );
    const planPayload = (
      planCall![1] as {
        props: {
          payload: {
            title?: string;
            segments: { label: string; time?: string }[];
          };
        };
      }
    ).props.payload;
    expect(planPayload.title).toBe("Trip flow");
    expect(planPayload.segments.map((s) => s.label)).toEqual(["Arrive", "Explore"]);
    destroyLiquidEmbeds(root);
    expect(unmountMock).toHaveBeenCalledTimes(1);
  });

  it("mounts timeline host from fixture markdown", async () => {
    const md = [
      "```timeline",
      "title: Ship log",
      "",
      "---",
      "ts: Day 1",
      "label: Arrive",
      "---",
      "ts: Day 2",
      "label: Explore",
      "```",
    ].join("\n");

    const html = preprocessLiquidEmbeds(md);
    expect(html).toContain('data-liquid-embed="timeline"');
    const root = treeFromPlaceholders(html) as unknown as HTMLElement;
    const { hydrateLiquidEmbeds, destroyLiquidEmbeds } = await import("./hydrateLiquidEmbeds");
    hydrateLiquidEmbeds(root, {});

    const kinds = mountMock.mock.calls.map(
      (call) => (call[1] as { props: { kind: string } }).props.kind,
    );
    expect(kinds).toEqual(["timeline"]);
    const timelineCall = mountMock.mock.calls.find(
      (call) => (call[1] as { props: { kind: string } }).props.kind === "timeline",
    );
    const timelinePayload = (
      timelineCall![1] as {
        props: {
          payload: {
            title?: string;
            events: { label: string; ts?: string }[];
          };
        };
      }
    ).props.payload;
    expect(timelinePayload.title).toBe("Ship log");
    expect(timelinePayload.events.map((e) => e.label)).toEqual(["Arrive", "Explore"]);
    destroyLiquidEmbeds(root);
    expect(unmountMock).toHaveBeenCalledTimes(1);
  });

  it("mounts shortlist host from fixture markdown", async () => {
    const md = [
      "```shortlist",
      "title: Picks",
      "",
      "---",
      "label: Shinjuku",
      "score: 9.2",
      "---",
      "label: Asakusa",
      "score: 8.4",
      "```",
    ].join("\n");

    const html = preprocessLiquidEmbeds(md);
    expect(html).toContain('data-liquid-embed="shortlist"');
    const root = treeFromPlaceholders(html) as unknown as HTMLElement;
    const { hydrateLiquidEmbeds, destroyLiquidEmbeds } = await import("./hydrateLiquidEmbeds");
    hydrateLiquidEmbeds(root, {});

    const kinds = mountMock.mock.calls.map(
      (call) => (call[1] as { props: { kind: string } }).props.kind,
    );
    expect(kinds).toEqual(["shortlist"]);
    const shortlistCall = mountMock.mock.calls.find(
      (call) => (call[1] as { props: { kind: string } }).props.kind === "shortlist",
    );
    const shortlistPayload = (
      shortlistCall![1] as {
        props: {
          payload: {
            title?: string;
            items: { label: string; score?: string }[];
          };
        };
      }
    ).props.payload;
    expect(shortlistPayload.title).toBe("Picks");
    expect(shortlistPayload.items.map((i) => i.label)).toEqual(["Shinjuku", "Asakusa"]);
    destroyLiquidEmbeds(root);
    expect(unmountMock).toHaveBeenCalledTimes(1);
  });

  it("mounts decision host from fixture markdown", async () => {
    const md = [
      "```decision",
      "title: Pick",
      "recommendation: Sol",
      "",
      "---",
      "label: Sol",
      "pros: Fast | Smart",
      "cons: Price",
      "---",
      "label: Terra",
      "pros: Cheap",
      "cons: Slow",
      "```",
    ].join("\n");

    const html = preprocessLiquidEmbeds(md);
    expect(html).toContain('data-liquid-embed="decision"');
    const root = treeFromPlaceholders(html) as unknown as HTMLElement;
    const { hydrateLiquidEmbeds, destroyLiquidEmbeds } = await import("./hydrateLiquidEmbeds");
    hydrateLiquidEmbeds(root, {});

    const kinds = mountMock.mock.calls.map(
      (call) => (call[1] as { props: { kind: string } }).props.kind,
    );
    expect(kinds).toEqual(["decision"]);
    const decisionCall = mountMock.mock.calls.find(
      (call) => (call[1] as { props: { kind: string } }).props.kind === "decision",
    );
    const decisionPayload = (
      decisionCall![1] as {
        props: {
          payload: {
            title?: string;
            recommendation?: string;
            options: { label: string; pros: string[] }[];
          };
        };
      }
    ).props.payload;
    expect(decisionPayload.title).toBe("Pick");
    expect(decisionPayload.recommendation).toBe("Sol");
    expect(decisionPayload.options.map((o) => o.label)).toEqual(["Sol", "Terra"]);
    expect(decisionPayload.options[0].pros).toEqual(["Fast", "Smart"]);
    destroyLiquidEmbeds(root);
    expect(unmountMock).toHaveBeenCalledTimes(1);
  });

  it("mounts brief host from fixture markdown", async () => {
    const md = [
      "```brief",
      "title: Why Tokyo first",
      "",
      "---",
      "heading: Easy logistics",
      "body: One simple route",
      "",
      "===",
      "---",
      "title: JNTO",
      "url: https://example.com",
      "```",
    ].join("\n");

    const html = preprocessLiquidEmbeds(md);
    expect(html).toContain('data-liquid-embed="brief"');
    const root = treeFromPlaceholders(html) as unknown as HTMLElement;
    const { hydrateLiquidEmbeds, destroyLiquidEmbeds } = await import("./hydrateLiquidEmbeds");
    hydrateLiquidEmbeds(root, {});

    const kinds = mountMock.mock.calls.map(
      (call) => (call[1] as { props: { kind: string } }).props.kind,
    );
    expect(kinds).toEqual(["brief"]);
    const briefCall = mountMock.mock.calls.find(
      (call) => (call[1] as { props: { kind: string } }).props.kind === "brief",
    );
    const briefPayload = (
      briefCall![1] as {
        props: {
          payload: {
            title?: string;
            sections: { heading: string }[];
            sources?: { title: string }[];
          };
        };
      }
    ).props.payload;
    expect(briefPayload.title).toBe("Why Tokyo first");
    expect(briefPayload.sections.map((s) => s.heading)).toEqual(["Easy logistics"]);
    expect(briefPayload.sources?.map((s) => s.title)).toEqual(["JNTO"]);
    destroyLiquidEmbeds(root);
    expect(unmountMock).toHaveBeenCalledTimes(1);
  });

  it("hydrates a dashboard embed", async () => {
    const md = [
      "```dashboard",
      "title: Trip pulse",
      "",
      "---",
      "label: Days locked",
      "value: 7",
      "tone: success",
      "---",
      "label: Budget used",
      "value: 42%",
      "```",
    ].join("\n");

    const html = preprocessLiquidEmbeds(md);
    expect(html).toContain('data-liquid-embed="dashboard"');
    const root = treeFromPlaceholders(html) as unknown as HTMLElement;
    const { hydrateLiquidEmbeds, destroyLiquidEmbeds } = await import("./hydrateLiquidEmbeds");
    hydrateLiquidEmbeds(root, {});

    const kinds = mountMock.mock.calls.map(
      (call) => (call[1] as { props: { kind: string } }).props.kind,
    );
    expect(kinds).toEqual(["dashboard"]);
    const dashCall = mountMock.mock.calls.find(
      (call) => (call[1] as { props: { kind: string } }).props.kind === "dashboard",
    );
    const dashPayload = (
      dashCall![1] as {
        props: {
          payload: {
            title?: string;
            tiles: { label: string; value: string }[];
          };
        };
      }
    ).props.payload;
    expect(dashPayload.title).toBe("Trip pulse");
    expect(dashPayload.tiles.map((t) => t.label)).toEqual(["Days locked", "Budget used"]);
    destroyLiquidEmbeds(root);
    expect(unmountMock).toHaveBeenCalledTimes(1);
  });
});
