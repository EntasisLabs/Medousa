import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { preprocessLiquidEmbeds } from "./liquidEmbeds";

const { mountMock, unmountMock } = vi.hoisted(() => ({
  mountMock: vi.fn((_component: unknown, _options: unknown) => ({})),
  unmountMock: vi.fn(async (_instance: unknown) => {}),
}));

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

vi.mock("./codeBlocks", () => ({
  hydrateCodeBlocks: vi.fn(async () => {}),
}));

vi.mock("./mermaid", () => ({
  hydrateMermaid: vi.fn(async () => {}),
}));

vi.mock("./hydrateLocalImages", () => ({
  hydrateLocalImages: vi.fn(async () => {}),
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
      root.children.push(
        el({
          "data-liquid-embed": value,
          ...(props ? { "data-liquid-props": props } : {}),
        }),
      );
    } else {
      root.children.push(el({ "data-liquid-icon": value }));
    }
  }
  return root;
}

describe("hydrateMarkdownContainer", () => {
  beforeEach(() => {
    (globalThis as { window?: unknown }).window = globalThis;
    mountMock.mockClear();
  });

  afterEach(() => {
    delete (globalThis as { window?: unknown }).window;
  });

  it("fingerprints embed placeholders", async () => {
    const { liquidPlaceholderFingerprint } = await import("./hydrateMarkdownContainer");
    const html = preprocessLiquidEmbeds(
      ["```card", "title: Hello", "body: World", "```"].join("\n"),
    );
    const root = treeFromPlaceholders(html) as unknown as HTMLElement;
    const fp = liquidPlaceholderFingerprint(root);
    expect(fp).toContain("card:");
  });

  it("hydrates liquid after optional code/mermaid steps", async () => {
    const { hydrateMarkdownContainer } = await import("./hydrateMarkdownContainer");
    const { destroyLiquidEmbeds } = await import("./hydrateLiquidEmbeds");
    const html = preprocessLiquidEmbeds(
      ["```card", "title: Hello", "body: World", "```"].join("\n"),
    );
    const root = treeFromPlaceholders(html) as unknown as HTMLElement;
    await hydrateMarkdownContainer(root, {
      liquidContext: { openLinksInWeb: false },
      code: true,
      mermaid: true,
      liquid: true,
      animate: false,
    });
    expect(mountMock).toHaveBeenCalled();
    const props = mountMock.mock.calls[0]![1] as { props: { animate?: boolean; kind: string } };
    expect(props.props.kind).toBe("card");
    expect(props.props.animate).toBe(false);
    destroyLiquidEmbeds(root);
  });
});
