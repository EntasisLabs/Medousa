import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { preprocessLiquidEmbeds } from "./liquidEmbeds";

type MountHostProps = {
  kind: string;
  payload: unknown;
  context?: unknown;
};

type MountOptions = {
  target?: unknown;
  props: MountHostProps;
};

const mountMock = vi.fn((_component: unknown, _options: MountOptions) => ({}));
const unmountMock = vi.fn(async (_instance: unknown) => {});

vi.mock("svelte", async () => {
  const actual = await vi.importActual<typeof import("svelte")>("svelte");
  return {
    ...actual,
    mount: mountMock as unknown as typeof actual.mount,
    unmount: unmountMock as unknown as typeof actual.unmount,
  };
});

vi.mock("./LiquidMdHost.svelte", () => ({
  default: { name: "LiquidMdHost" },
}));

function hostProps(call: (typeof mountMock.mock.calls)[number]): MountHostProps {
  return call[1].props;
}

function findHostCall(kind: string) {
  return mountMock.mock.calls.find((call) => hostProps(call).kind === kind);
}

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

    const kinds = mountMock.mock.calls.map((call) => hostProps(call).kind);
    expect(kinds).toEqual(["card", "carousel", "actions", "icon"]);

    const cardCall = findHostCall("card");
    expect((hostProps(cardCall!).payload as { title: string }).title).toBe("Sol");

    const carouselCall = findHostCall("carousel");
    const carouselPayload = hostProps(carouselCall!).payload as { items: unknown[] };
    expect(carouselPayload.items).toHaveLength(2);

    const actionsCall = findHostCall("actions");
    const actionsPayload = hostProps(actionsCall!).payload as {
      actions: { intent?: string }[];
    };
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

    const kinds = mountMock.mock.calls.map((call) => hostProps(call).kind);
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

    const kinds = mountMock.mock.calls.map((call) => hostProps(call).kind);
    expect(kinds).toEqual(["cite"]);
    const citePayload = hostProps(findHostCall("cite")!).payload as {
      quote?: string;
      title?: string;
    };
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

    const kinds = mountMock.mock.calls.map((call) => hostProps(call).kind);
    expect(kinds).toEqual(["compare"]);
    const comparePayload = hostProps(findHostCall("compare")!).payload as {
      title?: string;
      entities: { label: string }[];
      axes: { label: string }[];
    };
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

    const kinds = mountMock.mock.calls.map((call) => hostProps(call).kind);
    expect(kinds).toEqual(["plan"]);
    const planPayload = hostProps(findHostCall("plan")!).payload as {
      title?: string;
      segments: { label: string; time?: string }[];
    };
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

    const kinds = mountMock.mock.calls.map((call) => hostProps(call).kind);
    expect(kinds).toEqual(["timeline"]);
    const timelinePayload = hostProps(findHostCall("timeline")!).payload as {
      title?: string;
      events: { label: string; ts?: string }[];
    };
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

    const kinds = mountMock.mock.calls.map((call) => hostProps(call).kind);
    expect(kinds).toEqual(["shortlist"]);
    const shortlistPayload = hostProps(findHostCall("shortlist")!).payload as {
      title?: string;
      items: { label: string; score?: string }[];
    };
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

    const kinds = mountMock.mock.calls.map((call) => hostProps(call).kind);
    expect(kinds).toEqual(["decision"]);
    const decisionPayload = hostProps(findHostCall("decision")!).payload as {
      title?: string;
      recommendation?: string;
      options: { label: string; pros: string[] }[];
    };
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

    const kinds = mountMock.mock.calls.map((call) => hostProps(call).kind);
    expect(kinds).toEqual(["brief"]);
    const briefPayload = hostProps(findHostCall("brief")!).payload as {
      title?: string;
      sections: { heading: string }[];
      sources?: { title: string }[];
    };
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

    const kinds = mountMock.mock.calls.map((call) => hostProps(call).kind);
    expect(kinds).toEqual(["dashboard"]);
    const dashPayload = hostProps(findHostCall("dashboard")!).payload as {
      title?: string;
      tiles: { label: string; value: string }[];
    };
    expect(dashPayload.title).toBe("Trip pulse");
    expect(dashPayload.tiles.map((t) => t.label)).toEqual(["Days locked", "Budget used"]);
    destroyLiquidEmbeds(root);
    expect(unmountMock).toHaveBeenCalledTimes(1);
  });

  it("hydrates a chart embed", async () => {
    const md = [
      "```chart",
      "type: bar",
      "title: Visitors",
      "",
      "| Month | Desktop | Mobile |",
      "| ----- | ------- | ------ |",
      "| Jan   | 186     | 80     |",
      "| Feb   | 305     | 200    |",
      "```",
    ].join("\n");

    const html = preprocessLiquidEmbeds(md);
    expect(html).toContain('data-liquid-embed="chart"');
    const root = treeFromPlaceholders(html) as unknown as HTMLElement;
    const { hydrateLiquidEmbeds, destroyLiquidEmbeds } = await import("./hydrateLiquidEmbeds");
    hydrateLiquidEmbeds(root, {});

    const kinds = mountMock.mock.calls.map((call) => hostProps(call).kind);
    expect(kinds).toEqual(["chart"]);
    const chartPayload = hostProps(findHostCall("chart")!).payload as {
      type: string;
      title?: string;
      categories: string[];
      series: { label: string; values: number[] }[];
    };
    expect(chartPayload.type).toBe("bar");
    expect(chartPayload.title).toBe("Visitors");
    expect(chartPayload.categories).toEqual(["Jan", "Feb"]);
    expect(chartPayload.series.map((s) => s.label)).toEqual(["Desktop", "Mobile"]);
    destroyLiquidEmbeds(root);
    expect(unmountMock).toHaveBeenCalledTimes(1);
  });
});
