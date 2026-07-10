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
});
