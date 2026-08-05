import {
  decodeLiquidProps,
  type LiquidEmbedKind,
  type LiquidFeedDatatype,
} from "../liquidEmbeds.js";
import {
  renderLiquidEmbedHtml,
  renderLiquidIconHtml,
  type LiquidBrowserStaticRenderOptions,
} from "./render.js";
import { installLiquidMarkdownStyles } from "./styles.js";

export interface LiquidActionEvent {
  intent: string;
  label: string;
  element: HTMLButtonElement;
}

export interface LiquidFeedRequest {
  feedId: string;
  datatype: LiquidFeedDatatype;
}

export interface LiquidBrowserHydrateOptions extends LiquidBrowserStaticRenderOptions {
  /** Host Markdown renderer for nested prose. It should sanitize any generated HTML it accepts. */
  renderMarkdown?: (markdown: string, target: HTMLElement) => void | Promise<void>;
  onAction?: (event: LiquidActionEvent) => void | Promise<void>;
  openLink?: (url: string) => void | Promise<void>;
  copyText?: (text: string) => void | Promise<void>;
  loadFeed?: (request: LiquidFeedRequest) => unknown | Promise<unknown>;
  onError?: (error: unknown, context: string) => void;
  /** Install the shared stylesheet into the root document. Defaults to true. */
  installStyles?: boolean;
  styleTarget?: Document | ShadowRoot;
}

export interface LiquidHydrationHandle {
  /** Settles after initial embeds, nested Markdown, and load-on-mount feeds render. */
  ready: Promise<void>;
  destroy(): void;
}

type ActiveHydration = LiquidHydrationHandle & { dispose: () => void };

const activeHydrations = new WeakMap<HTMLElement, ActiveHydration>();

function reportError(
  options: LiquidBrowserHydrateOptions,
  error: unknown,
  context: string,
): void {
  if (options.onError) {
    options.onError(error, context);
    return;
  }
  console.warn(`[liquid-markdown] ${context}`, error);
}

function isElement(value: EventTarget | null): value is Element {
  return value != null && typeof (value as Element).closest === "function";
}

function csvRows(source: string): string[][] {
  const rows: string[][] = [];
  let row: string[] = [];
  let field = "";
  let quoted = false;
  for (let index = 0; index < source.length; index += 1) {
    const char = source[index]!;
    if (char === '"') {
      if (quoted && source[index + 1] === '"') {
        field += '"';
        index += 1;
      } else {
        quoted = !quoted;
      }
    } else if (char === "," && !quoted) {
      row.push(field);
      field = "";
    } else if ((char === "\n" || char === "\r") && !quoted) {
      if (char === "\r" && source[index + 1] === "\n") index += 1;
      row.push(field);
      if (row.some((value) => value.length > 0)) rows.push(row);
      row = [];
      field = "";
    } else {
      field += char;
    }
  }
  row.push(field);
  if (row.some((value) => value.length > 0)) rows.push(row);
  return rows.slice(0, 200).map((values) => values.slice(0, 50));
}

function feedContent(value: unknown): unknown {
  if (!value || typeof value !== "object" || Array.isArray(value)) return value;
  const object = value as Record<string, unknown>;
  if ("content" in object) return object.content;
  if ("value" in object) return object.value;
  if ("data" in object) return object.data;
  if ("url" in object) return object.url;
  return value;
}

async function defaultCopy(text: string): Promise<void> {
  if (typeof navigator !== "undefined" && navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(text);
  }
}

function activateTab(container: HTMLElement, index: number): void {
  const tabs = container.querySelectorAll<HTMLButtonElement>("[data-liquid-tab]");
  const panels = container.querySelectorAll<HTMLElement>("[data-liquid-tab-panel]");
  if (index < 0 || index >= tabs.length || index >= panels.length) return;
  tabs.forEach((tab, tabIndex) => {
    tab.setAttribute("aria-selected", String(tabIndex === index));
    tab.tabIndex = tabIndex === index ? 0 : -1;
  });
  panels.forEach((panel, panelIndex) => {
    panel.hidden = panelIndex !== index;
  });
}

function moveSlide(container: HTMLElement, direction: number): void {
  const slides = Array.from(container.querySelectorAll<HTMLElement>("[data-liquid-slide]"));
  if (slides.length === 0) return;
  const current = Math.max(0, slides.findIndex((slide) => !slide.hidden));
  const next = (current + direction + slides.length) % slides.length;
  slides.forEach((slide, index) => {
    slide.hidden = index !== next;
  });
  const count = container.querySelector<HTMLElement>("[data-liquid-slide-count]");
  if (count) count.textContent = `${next + 1} / ${slides.length}`;
}

/** Hydrate every inert placeholder under a host-owned Markdown container. */
export function hydrateLiquidEmbeds(
  root: HTMLElement,
  options: LiquidBrowserHydrateOptions = {},
): LiquidHydrationHandle {
  destroyLiquidEmbeds(root);

  if (options.installStyles !== false) {
    const rootNode = root.getRootNode();
    const styleTarget = options.styleTarget
      ?? (rootNode && "host" in rootNode ? rootNode as ShadowRoot : root.ownerDocument);
    installLiquidMarkdownStyles(styleTarget);
  }

  let disposed = false;
  const cleanups: Array<() => void> = [];
  const feedLoads = new WeakMap<HTMLElement, number>();

  const hydrateScope = async (scope: ParentNode): Promise<void> => {
    if (disposed) return;

    const embeds = Array.from(scope.querySelectorAll<HTMLElement>("[data-liquid-embed]"));
    for (const element of embeds) {
      if (disposed || element.dataset.liquidHydrated === "1") continue;
      const kind = element.dataset.liquidEmbed as LiquidEmbedKind | undefined;
      const encoded = element.dataset.liquidProps;
      if (!kind || !encoded) continue;
      const payload = decodeLiquidProps(encoded);
      const html = payload == null ? null : renderLiquidEmbedHtml(kind, payload, options);
      if (!html) {
        element.classList.add("liquid-md-embed--error");
        element.textContent = `[${kind} could not be rendered]`;
        element.dataset.liquidHydrated = "error";
        continue;
      }
      element.innerHTML = html;
      element.dataset.liquidHydrated = "1";
    }

    const icons = Array.from(scope.querySelectorAll<HTMLElement>("[data-liquid-icon]:not([data-liquid-icon-id])"));
    for (const element of icons) {
      if (disposed || element.dataset.liquidHydrated === "1") continue;
      const id = element.dataset.liquidIcon;
      const html = id ? renderLiquidIconHtml(id) : null;
      if (!html) continue;
      element.innerHTML = html;
      element.dataset.liquidHydrated = "1";
    }

    const markdownTargets = Array.from(scope.querySelectorAll<HTMLElement>("[data-liquid-markdown]:not([data-liquid-markdown-hydrated])"));
    const nested: HTMLElement[] = [];
    for (const target of markdownTargets) {
      if (disposed) break;
      target.dataset.liquidMarkdownHydrated = "1";
      if (!options.renderMarkdown) continue;
      const source = target.textContent ?? "";
      target.replaceChildren();
      try {
        await options.renderMarkdown(source, target);
        nested.push(target);
      } catch (error) {
        target.textContent = source;
        reportError(options, error, "nested Markdown render failed");
      }
    }

    for (const target of nested) await hydrateScope(target);

    const feeds = Array.from(scope.querySelectorAll<HTMLElement>("[data-liquid-feed]:not([data-liquid-feed-ready])"));
    for (const feed of feeds) {
      feed.dataset.liquidFeedReady = "1";
      if (feed.dataset.liquidFeedRefresh === "load") await loadFeed(feed);
    }
  };

  const renderFeedValue = async (
    feed: HTMLElement,
    datatype: LiquidFeedDatatype,
    rawValue: unknown,
  ): Promise<void> => {
    const state = feed.querySelector<HTMLElement>(".medousa-liquid__feed-state");
    if (!state) return;
    const value = feedContent(rawValue);
    state.replaceChildren();
    state.classList.add("medousa-liquid__feed-content");

    if (datatype === "md") {
      const body = state.ownerDocument.createElement("div");
      body.className = "medousa-liquid__markdown";
      body.dataset.liquidMarkdown = "";
      body.textContent = typeof value === "string" ? value : JSON.stringify(value, null, 2);
      state.appendChild(body);
      await hydrateScope(state);
      return;
    }

    if (datatype === "image") {
      const source = typeof value === "string" ? value : "";
      const resolved = options.resolveMediaUrl?.(source) ?? source;
      if (!resolved || /^(?:javascript|vbscript):/i.test(resolved)) {
        state.textContent = feed.dataset.liquidFeedEmpty || "No image available.";
        return;
      }
      const image = state.ownerDocument.createElement("img");
      image.src = resolved;
      image.alt = feed.dataset.liquidFeedId || "Feed image";
      image.loading = "lazy";
      image.style.maxWidth = "100%";
      state.appendChild(image);
      return;
    }

    if (datatype === "csv") {
      const rows = csvRows(typeof value === "string" ? value : "");
      if (rows.length === 0) {
        state.textContent = feed.dataset.liquidFeedEmpty || "No feed data yet.";
        return;
      }
      const wrap = state.ownerDocument.createElement("div");
      wrap.className = "medousa-liquid__feed-table";
      const table = state.ownerDocument.createElement("table");
      table.className = "medousa-liquid__table";
      rows.forEach((row, rowIndex) => {
        const tr = state.ownerDocument.createElement("tr");
        row.forEach((cell) => {
          const element = state.ownerDocument.createElement(rowIndex === 0 ? "th" : "td");
          element.textContent = cell;
          tr.appendChild(element);
        });
        table.appendChild(tr);
      });
      wrap.appendChild(table);
      state.appendChild(wrap);
      return;
    }

    const text = datatype === "json" && typeof value !== "string"
      ? JSON.stringify(value, null, 2)
      : typeof value === "string"
        ? value
        : JSON.stringify(value, null, 2);
    if (datatype === "json") {
      const pre = state.ownerDocument.createElement("pre");
      const code = state.ownerDocument.createElement("code");
      code.textContent = text;
      pre.appendChild(code);
      state.appendChild(pre);
    } else {
      state.textContent = text || feed.dataset.liquidFeedEmpty || "No feed data yet.";
    }
  };

  const loadFeed = async (feed: HTMLElement): Promise<void> => {
    const feedId = feed.dataset.liquidFeedId;
    const datatype = feed.dataset.liquidFeedType as LiquidFeedDatatype | undefined;
    const state = feed.querySelector<HTMLElement>(".medousa-liquid__feed-state");
    if (!feedId || !datatype || !state) return;
    if (!options.loadFeed) {
      state.textContent = feed.dataset.liquidFeedEmpty || "Live feeds are unavailable on this surface.";
      return;
    }

    const request = (feedLoads.get(feed) ?? 0) + 1;
    feedLoads.set(feed, request);
    state.textContent = "Loading…";
    try {
      const value = await options.loadFeed({ feedId, datatype });
      if (disposed || feedLoads.get(feed) !== request) return;
      await renderFeedValue(feed, datatype, value);
    } catch (error) {
      if (disposed || feedLoads.get(feed) !== request) return;
      state.textContent = "Feed could not be loaded.";
      reportError(options, error, `feed ${feedId} failed`);
    }
  };

  const onClick = (event: Event): void => {
    if (!isElement(event.target)) return;
    const target = event.target;

    const tab = target.closest<HTMLButtonElement>("[data-liquid-tab]");
    if (tab && root.contains(tab)) {
      const container = tab.closest<HTMLElement>("[data-liquid-tabs]");
      const index = Number.parseInt(tab.dataset.liquidTab ?? "", 10);
      if (container && Number.isFinite(index)) activateTab(container, index);
      return;
    }

    const carousel = target.closest<HTMLButtonElement>("[data-liquid-carousel-nav]");
    if (carousel && root.contains(carousel)) {
      const container = carousel.closest<HTMLElement>("[data-liquid-carousel]");
      const track = container?.querySelector<HTMLElement>("[data-liquid-carousel-track]");
      track?.scrollBy({ left: (carousel.dataset.liquidCarouselNav === "prev" ? -1 : 1) * Math.max(180, track.clientWidth * 0.8), behavior: "smooth" });
      return;
    }

    const slide = target.closest<HTMLButtonElement>("[data-liquid-slide-nav]");
    if (slide && root.contains(slide)) {
      const container = slide.closest<HTMLElement>("[data-liquid-slides]");
      if (container) moveSlide(container, slide.dataset.liquidSlideNav === "prev" ? -1 : 1);
      return;
    }

    const copy = target.closest<HTMLButtonElement>("[data-liquid-copy]");
    if (copy && root.contains(copy)) {
      const code = copy.closest(".medousa-liquid__code")?.querySelector<HTMLElement>("[data-liquid-code]");
      const text = code?.textContent ?? "";
      const operation = options.copyText ? options.copyText(text) : defaultCopy(text);
      void Promise.resolve(operation).then(() => {
        const previous = copy.textContent;
        copy.textContent = "Copied";
        setTimeout(() => { if (copy.isConnected) copy.textContent = previous; }, 1_200);
      }).catch((error) => reportError(options, error, "copy failed"));
      return;
    }

    const load = target.closest<HTMLButtonElement>("[data-liquid-feed-load]");
    if (load && root.contains(load)) {
      const feed = load.closest<HTMLElement>("[data-liquid-feed]");
      if (feed) void loadFeed(feed);
      return;
    }

    const action = target.closest<HTMLButtonElement>("[data-liquid-action]");
    if (action && root.contains(action)) {
      const detail: LiquidActionEvent = {
        intent: action.dataset.liquidAction ?? "",
        label: action.dataset.liquidActionLabel ?? action.textContent?.trim() ?? "",
        element: action,
      };
      const CustomEventConstructor = root.ownerDocument.defaultView?.CustomEvent ?? CustomEvent;
      root.dispatchEvent(new CustomEventConstructor("medousa-liquid-action", { detail, bubbles: true }));
      if (options.onAction) {
        void Promise.resolve(options.onAction(detail)).catch((error) => reportError(options, error, "action failed"));
      }
      return;
    }

    const link = target.closest<HTMLAnchorElement>("[data-liquid-link]");
    if (link && root.contains(link) && options.openLink) {
      event.preventDefault();
      const url = link.dataset.liquidLink;
      if (url) void Promise.resolve(options.openLink(url)).catch((error) => reportError(options, error, "link open failed"));
    }
  };

  const onKeyDown = (event: KeyboardEvent): void => {
    if (!isElement(event.target) || !["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
    const tab = event.target.closest<HTMLButtonElement>("[data-liquid-tab]");
    const container = tab?.closest<HTMLElement>("[data-liquid-tabs]");
    if (!tab || !container || !root.contains(tab)) return;
    const tabs = Array.from(container.querySelectorAll<HTMLButtonElement>("[data-liquid-tab]"));
    const current = tabs.indexOf(tab);
    const next = event.key === "Home" ? 0 : event.key === "End" ? tabs.length - 1 : (current + (event.key === "ArrowLeft" ? -1 : 1) + tabs.length) % tabs.length;
    event.preventDefault();
    activateTab(container, next);
    tabs[next]?.focus();
  };

  const onToggle = (event: Event): void => {
    if (!isElement(event.target) || event.target.tagName !== "DETAILS") return;
    const details = event.target as HTMLDetailsElement;
    const container = details.closest<HTMLElement>("[data-liquid-accordion]");
    if (!container || !root.contains(container) || !details.open || container.dataset.liquidAccordionMultiple === "true") return;
    container.querySelectorAll<HTMLDetailsElement>("[data-liquid-accordion-item]").forEach((item) => {
      if (item !== details) item.open = false;
    });
  };

  root.addEventListener("click", onClick);
  root.addEventListener("keydown", onKeyDown);
  root.addEventListener("toggle", onToggle, true);
  cleanups.push(() => root.removeEventListener("click", onClick));
  cleanups.push(() => root.removeEventListener("keydown", onKeyDown));
  cleanups.push(() => root.removeEventListener("toggle", onToggle, true));

  const ready = hydrateScope(root).catch((error) => reportError(options, error, "hydration failed"));
  const active: ActiveHydration = {
    ready,
    destroy: () => {
      if (disposed) return;
      disposed = true;
      cleanups.splice(0).forEach((cleanup) => cleanup());
      if (activeHydrations.get(root) === active) activeHydrations.delete(root);
    },
    dispose: () => {
      disposed = true;
      cleanups.splice(0).forEach((cleanup) => cleanup());
    },
  };
  activeHydrations.set(root, active);
  return active;
}

/** Alias that reads naturally at host integration sites. */
export const hydrateLiquidMarkdown = hydrateLiquidEmbeds;

/** Remove event handlers and invalidate pending async work for a container. */
export function destroyLiquidEmbeds(root: HTMLElement): void {
  const active = activeHydrations.get(root);
  if (!active) return;
  active.dispose();
  activeHydrations.delete(root);
}
