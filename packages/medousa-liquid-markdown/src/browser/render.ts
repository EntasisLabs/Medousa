import { escapeAttr, escapeHtml } from "../escape.js";
import { normalizeLiquidIconId } from "../icons.js";
import type {
  LiquidAccordionProps,
  LiquidActionProps,
  LiquidBlockProps,
  LiquidBriefProps,
  LiquidCardPoint,
  LiquidCardProps,
  LiquidChartProps,
  LiquidChipProps,
  LiquidCiteProps,
  LiquidCodeProps,
  LiquidCompareProps,
  LiquidDashboardProps,
  LiquidDecisionProps,
  LiquidEmbedKind,
  LiquidFeedProps,
  LiquidMediaProps,
  LiquidPlanProps,
  LiquidReportProps,
  LiquidSectionProps,
  LiquidShortlistProps,
  LiquidSlidesProps,
  LiquidStepsProps,
  LiquidTabsProps,
  LiquidTimelineProps,
  LiquidTreeNode,
  LiquidTreeProps,
} from "../liquidEmbeds.js";
import { styledBlockCssVars } from "../styledBlock.js";
import { renderLiquidChart } from "./chart.js";

export interface LiquidBrowserStaticRenderOptions {
  /** Resolve a model-provided media path into a host-safe URL. Returning null hides it. */
  resolveMediaUrl?: (source: string) => string | null | undefined;
  /** Add the shared enter animation marker. Defaults to true. */
  animate?: boolean;
}

const ICON_GLYPHS: Record<string, string> = {
  sparkles: "✦",
  lock: "▣",
  globe: "◎",
  "message-circle": "◌",
  brain: "◉",
  shield: "◇",
  code: "⌘",
  cpu: "▦",
  zap: "ϟ",
  clock: "◷",
  hourglass: "⌛",
  coins: "◍",
  tag: "◇",
  mic: "●",
  pencil: "✎",
  "file-code": "▤",
  table: "▦",
  layers: "▱",
  rocket: "↗",
  star: "★",
  check: "✓",
  x: "×",
  info: "i",
  "alert-triangle": "!",
  search: "⌕",
  book: "▥",
  map: "⌗",
  compass: "⌖",
  plane: "✈",
  "map-pin": "●",
  hotel: "▥",
  camera: "▣",
  heart: "♥",
  house: "⌂",
  calendar: "▦",
  sun: "☀",
  moon: "◒",
  coffee: "☕",
  train: "▤",
  "train-front": "▤",
  car: "▰",
  "building-2": "▥",
  landmark: "▥",
  mountain: "△",
  utensils: "⋈",
  "shopping-bag": "▢",
  music: "♫",
  users: "◉",
  flag: "⚑",
  navigation: "➤",
  bed: "▰",
};

function stringValue(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function toneClass(value: unknown): string {
  const tone = stringValue(value)?.toLowerCase();
  return tone && /^[a-z][a-z0-9-]*$/.test(tone) ? ` medousa-liquid__chip--${tone}` : "";
}

function markdown(source: unknown, className = ""): string {
  const body = typeof source === "string" ? source : "";
  if (!body.trim()) return "";
  return `<div class="medousa-liquid__markdown${className ? ` ${className}` : ""}" data-liquid-markdown>${escapeHtml(body)}</div>`;
}

function header(title?: string, subtitle?: string, kicker?: string): string {
  if (!title && !subtitle && !kicker) return "";
  return [
    `<header class="medousa-liquid__header">`,
    kicker ? `<p class="medousa-liquid__kicker">${escapeHtml(kicker)}</p>` : "",
    title ? `<h3 class="medousa-liquid__title">${escapeHtml(title)}</h3>` : "",
    subtitle ? `<p class="medousa-liquid__subtitle">${escapeHtml(subtitle)}</p>` : "",
    `</header>`,
  ].join("");
}

function safeMediaUrl(source: unknown, options: LiquidBrowserStaticRenderOptions): string | null {
  const raw = stringValue(source);
  if (!raw) return null;
  const resolved = options.resolveMediaUrl ? options.resolveMediaUrl(raw) : raw;
  if (!resolved?.trim()) return null;
  const url = resolved.trim();
  if (/^[\u0000-\u001f]/.test(url) || /^(?:javascript|vbscript):/i.test(url)) return null;
  if (/^data:/i.test(url) && !/^data:image\/(?:png|jpe?g|gif|webp|avif);base64,/i.test(url)) return null;
  if (!options.resolveMediaUrl && /^[a-z][a-z\d+.-]*:/i.test(url) && !/^(?:https?|blob):/i.test(url) && !/^data:image\//i.test(url)) {
    return null;
  }
  return url;
}

function safeLinkUrl(source: unknown): string | null {
  const raw = stringValue(source);
  if (!raw) return null;
  try {
    const parsed = new URL(raw);
    return ["http:", "https:", "mailto:", "medousa:"].includes(parsed.protocol) ? raw : null;
  } catch {
    return null;
  }
}

function image(source: unknown, alt: unknown, options: LiquidBrowserStaticRenderOptions, className = ""): string {
  const url = safeMediaUrl(source, options);
  if (!url) return "";
  return `<img${className ? ` class="${escapeAttr(className)}"` : ""} src="${escapeAttr(url)}" alt="${escapeAttr(typeof alt === "string" ? alt : "")}" loading="lazy">`;
}

/** Render one allowlisted icon as a portable text glyph. */
export function renderLiquidIconHtml(rawId: string): string | null {
  const id = normalizeLiquidIconId(rawId);
  if (!id) return null;
  return `<span class="medousa-liquid-icon" data-liquid-icon-id="${escapeAttr(id)}" aria-hidden="true" title="${escapeAttr(id)}">${escapeHtml(ICON_GLYPHS[id] ?? "◆")}</span>`;
}

function visual(
  item: { emoji?: string; icon?: string; image?: string },
  options: LiquidBrowserStaticRenderOptions,
): string {
  const media = image(item.image, "", options);
  if (media) return `<span class="medousa-liquid__visual">${media}</span>`;
  const emoji = stringValue(item.emoji);
  if (emoji) return `<span class="medousa-liquid__visual" aria-hidden="true">${escapeHtml(emoji)}</span>`;
  const icon = item.icon ? renderLiquidIconHtml(item.icon) : null;
  return icon ? `<span class="medousa-liquid__visual">${icon}</span>` : "";
}

function pills(items: string[] | undefined, kind: "badge" | "chip" = "badge"): string {
  if (!items?.length) return "";
  return `<div class="medousa-liquid__${kind === "badge" ? "badges" : "chips"}">${items
    .filter((item) => typeof item === "string" && item.trim())
    .map((item) => `<span class="medousa-liquid__${kind}">${escapeHtml(item)}</span>`)
    .join("")}</div>`;
}

function cardPoints(points: LiquidCardPoint[] | undefined, options: LiquidBrowserStaticRenderOptions): string {
  if (!points?.length) return "";
  return `<div class="medousa-liquid__points">${points.map((point) => [
    `<div class="medousa-liquid__point">`,
    visual(point, options) || `<span class="medousa-liquid__rail-dot">•</span>`,
    `<span><strong>${escapeHtml(point.label)}</strong><span>${escapeHtml(point.body)}</span></span>`,
    `</div>`,
  ].join("")).join("")}</div>`;
}

function renderCard(props: LiquidCardProps, options: LiquidBrowserStaticRenderOptions): string | null {
  if (!stringValue(props.title)) return null;
  const hero = image(props.image, props.title, options, "medousa-liquid__card-image");
  return [
    `<article class="medousa-liquid__surface medousa-liquid__card">`,
    hero,
    `<div class="medousa-liquid__card-heading">`,
    visual({ emoji: props.emoji, icon: props.icon }, options),
    `<div class="medousa-liquid__card-copy"><h3 class="medousa-liquid__title">${escapeHtml(props.title)}</h3>`,
    props.subtitle ? `<p class="medousa-liquid__subtitle">${escapeHtml(props.subtitle)}</p>` : "",
    `</div></div>`,
    markdown(props.body, "medousa-liquid__card-body"),
    props.meta ? `<div class="medousa-liquid__meta">${escapeHtml(props.meta)}</div>` : "",
    props.summary ? `<p class="medousa-liquid__summary">${escapeHtml(props.summary)}</p>` : "",
    pills(props.chips, "chip"),
    cardPoints(props.points, options),
    pills(props.badges),
    `</article>`,
  ].join("");
}

function renderCarousel(payload: unknown, options: LiquidBrowserStaticRenderOptions): string | null {
  const items = (payload as { items?: LiquidCardProps[] })?.items;
  if (!Array.isArray(items) || items.length === 0) return null;
  const cards = items.map((item) => renderCard(item, options)).filter(Boolean).join("");
  if (!cards) return null;
  return `<section class="medousa-liquid__carousel-shell" data-liquid-carousel><div class="medousa-liquid__carousel-controls"><button class="medousa-liquid__icon-button" type="button" data-liquid-carousel-nav="prev" aria-label="Previous card">‹</button><button class="medousa-liquid__icon-button" type="button" data-liquid-carousel-nav="next" aria-label="Next card">›</button></div><div class="medousa-liquid__carousel" data-liquid-carousel-track>${cards}</div></section>`;
}

function renderActions(payload: unknown, options: LiquidBrowserStaticRenderOptions): string | null {
  const actions = (payload as { actions?: LiquidActionProps[] })?.actions;
  if (!Array.isArray(actions) || actions.length === 0) return null;
  return `<section class="medousa-liquid__actions"><p class="medousa-liquid__kicker">Suggested</p>${actions.map((action) => {
    const label = stringValue(action.label);
    if (!label) return "";
    const intent = stringValue(action.intent) ?? label;
    return `<button class="medousa-liquid__action" type="button" data-liquid-action="${escapeAttr(intent)}" data-liquid-action-label="${escapeAttr(label)}">${visual(action, options) || "<span></span>"}<span>${escapeHtml(label)}</span><span class="medousa-liquid__action-arrow">›</span></button>`;
  }).join("")}</section>`;
}

function renderCallout(payload: unknown): string | null {
  const props = payload as { body?: string; tone?: string; title?: string };
  if (!stringValue(props.body)) return null;
  const tone = stringValue(props.tone)?.toLowerCase();
  const className = tone && /^[a-z][a-z0-9-]*$/.test(tone) ? ` medousa-liquid__callout--${tone}` : "";
  return `<aside class="medousa-liquid__surface medousa-liquid__callout${className}">${props.title ? `<p class="medousa-liquid__callout-title">${escapeHtml(props.title)}</p>` : ""}${markdown(props.body)}</aside>`;
}

function renderSection(props: LiquidSectionProps): string | null {
  if (!stringValue(props.title)) return null;
  return `<section class="medousa-liquid__surface medousa-liquid__section">${header(props.title, props.subtitle)}${markdown(props.body)}</section>`;
}

function renderBlock(props: LiquidBlockProps): string | null {
  if (!props?.body?.trim() && !props?.id && !props?.font) return null;
  const align = props.align && ["left", "center", "right", "justify"].includes(props.align)
    ? props.align
    : undefined;
  const vars = styledBlockCssVars({ ...props, align });
  const style = Object.entries(vars).map(([key, value]) => `${key}:${value}`).join(";");
  const id = props.id && /^[A-Za-z][\w:.-]*$/.test(props.id) ? ` id="${escapeAttr(props.id)}"` : "";
  return `<section${id} class="medousa-liquid__surface medousa-liquid__block"${style ? ` style="${escapeAttr(style)}"` : ""}>${markdown(props.body)}</section>`;
}

function renderChips(payload: unknown): string | null {
  const chips = (payload as { chips?: LiquidChipProps[] })?.chips;
  if (!Array.isArray(chips) || chips.length === 0) return null;
  return `<div class="medousa-liquid__chips">${chips.map((chip) => {
    const label = stringValue(chip.label);
    return label ? `<span class="medousa-liquid__chip${toneClass(chip.tone)}"${chip.value ? ` title="${escapeAttr(chip.value)}"` : ""}>${escapeHtml(label)}</span>` : "";
  }).join("")}</div>`;
}

function renderMedia(props: LiquidMediaProps, options: LiquidBrowserStaticRenderOptions): string | null {
  const media = image(props.src, props.alt, options);
  if (!media) return null;
  const ratio = stringValue(props.ratio);
  const style = ratio && /^\d+(?:\.\d+)?\s*[/ :]\s*\d+(?:\.\d+)?$/.test(ratio)
    ? ` style="aspect-ratio:${escapeAttr(ratio.replace(":", "/"))}"`
    : "";
  return `<figure class="medousa-liquid__surface medousa-liquid__media"${style}>${media}${props.caption ? `<figcaption>${escapeHtml(props.caption)}</figcaption>` : ""}</figure>`;
}

function link(title: string, url: string): string {
  const safe = safeLinkUrl(url);
  return safe
    ? `<a class="medousa-liquid__link" href="${escapeAttr(safe)}" data-liquid-link="${escapeAttr(safe)}">${escapeHtml(title)}</a>`
    : escapeHtml(title);
}

function renderCite(props: LiquidCiteProps): string | null {
  if (!props.quote && !props.title && !props.url) return null;
  const label = props.title || props.source || props.url || "Source";
  return `<figure class="medousa-liquid__surface medousa-liquid__cite">${props.quote ? `<blockquote class="medousa-liquid__quote">${escapeHtml(props.quote)}</blockquote>` : ""}<figcaption class="medousa-liquid__cite-footer"><span>${props.source ? escapeHtml(props.source) : ""}</span>${props.url ? link(label, props.url) : props.title ? `<span>${escapeHtml(props.title)}</span>` : ""}</figcaption></figure>`;
}

function renderCompare(props: LiquidCompareProps): string | null {
  if (!props?.axes?.length || !props?.entities || props.entities.length < 2) return null;
  let body: string;
  if (props.mode === "faceoff") {
    body = `<div class="medousa-liquid__grid medousa-liquid__grid--${Math.min(3, props.entities.length)}">${props.entities.map((entity) => `<section class="medousa-liquid__panel"><h4 class="medousa-liquid__panel-title">${escapeHtml(entity.label)}</h4>${props.axes.map((axis) => `<div class="medousa-liquid__point"><span></span><span><strong>${escapeHtml(axis.label)}</strong><span>${escapeHtml(entity.values[axis.id] ?? "—")}</span></span></div>`).join("")}</section>`).join("")}</div>`;
  } else {
    body = `<div class="medousa-liquid__table-wrap"><table class="medousa-liquid__table"><thead><tr><th>Criteria</th>${props.entities.map((entity) => `<th>${escapeHtml(entity.label)}</th>`).join("")}</tr></thead><tbody>${props.axes.map((axis) => `<tr><th>${escapeHtml(axis.label)}</th>${props.entities.map((entity) => `<td>${escapeHtml(entity.values[axis.id] ?? "—")}</td>`).join("")}</tr>`).join("")}</tbody></table></div>`;
  }
  return `<section class="medousa-liquid__surface">${header(props.title, props.subtitle, "Compare")}${body}${props.recommendation ? `<p class="medousa-liquid__recommendation"><strong>Recommendation:</strong> ${escapeHtml(props.recommendation)}</p>` : ""}</section>`;
}

function renderPlan(props: LiquidPlanProps, options: LiquidBrowserStaticRenderOptions): string | null {
  if (!props?.segments || props.segments.length < 2) return null;
  return `<section class="medousa-liquid__surface">${header(props.title, props.subtitle, props.grouping || "Plan")}<div class="medousa-liquid__rail">${props.segments.map((segment, index) => `<div class="medousa-liquid__rail-item">${visual(segment, options) || `<span class="medousa-liquid__rail-dot">${index + 1}</span>`}<div class="medousa-liquid__rail-copy"><strong>${escapeHtml(segment.label)}</strong>${segment.time || segment.badge ? `<small>${escapeHtml([segment.time, segment.badge].filter(Boolean).join(" · "))}</small>` : ""}${segment.subtitle ? `<p class="medousa-liquid__subtitle">${escapeHtml(segment.subtitle)}</p>` : ""}${markdown(segment.body)}</div></div>`).join("")}</div></section>`;
}

function renderTimeline(props: LiquidTimelineProps, options: LiquidBrowserStaticRenderOptions): string | null {
  if (!props?.events || props.events.length < 2) return null;
  return `<section class="medousa-liquid__surface">${header(props.title, props.subtitle, props.granularity || "Timeline")}<div class="medousa-liquid__rail">${props.events.map((event, index) => `<div class="medousa-liquid__rail-item">${visual(event, options) || `<span class="medousa-liquid__rail-dot">${index + 1}</span>`}<div class="medousa-liquid__rail-copy"><strong>${escapeHtml(event.label)}</strong>${event.ts || event.lane || event.meta ? `<small>${escapeHtml([event.ts, event.lane, event.meta].filter(Boolean).join(" · "))}</small>` : ""}${event.detail ? `<p class="medousa-liquid__subtitle">${escapeHtml(event.detail)}</p>` : ""}${markdown(event.body)}</div></div>`).join("")}</div></section>`;
}

function renderShortlist(props: LiquidShortlistProps, options: LiquidBrowserStaticRenderOptions): string | null {
  if (!props?.items || props.items.length < 2) return null;
  const gridClass = props.items.length > 2 ? " medousa-liquid__grid--2" : "";
  return `<section class="medousa-liquid__surface">${header(props.title, props.subtitle, props.criteria || "Shortlist")}<div class="medousa-liquid__grid${gridClass}">${props.items.map((item, index) => `<article class="medousa-liquid__panel"><div class="medousa-liquid__card-heading">${visual(item, options) || `<span class="medousa-liquid__rail-dot">${index + 1}</span>`}<div class="medousa-liquid__card-copy"><h4 class="medousa-liquid__panel-title">${escapeHtml(item.label)}</h4>${item.score ? `<span class="medousa-liquid__score">${escapeHtml(item.score)}</span>` : ""}</div></div>${item.summary ? `<p class="medousa-liquid__summary">${escapeHtml(item.summary)}</p>` : ""}${item.meta ? `<div class="medousa-liquid__meta">${escapeHtml(item.meta)}</div>` : ""}</article>`).join("")}</div></section>`;
}

function renderDecision(props: LiquidDecisionProps): string | null {
  if (!props?.options || props.options.length < 2) return null;
  return `<section class="medousa-liquid__surface">${header(props.title, props.subtitle, props.factors || "Decision")}<div class="medousa-liquid__grid medousa-liquid__grid--2">${props.options.map((option) => `<article class="medousa-liquid__panel"><h4 class="medousa-liquid__panel-title">${escapeHtml(option.label)}${option.score ? ` <span class="medousa-liquid__score">${escapeHtml(option.score)}</span>` : ""}</h4>${option.summary ? `<p class="medousa-liquid__summary">${escapeHtml(option.summary)}</p>` : ""}<div class="medousa-liquid__pros-cons"><div><strong class="medousa-liquid__pros">Pros</strong><ul>${option.pros.map((item) => `<li>${escapeHtml(item)}</li>`).join("")}</ul></div><div><strong class="medousa-liquid__cons">Cons</strong><ul>${option.cons.map((item) => `<li>${escapeHtml(item)}</li>`).join("")}</ul></div></div></article>`).join("")}</div>${props.recommendation ? `<p class="medousa-liquid__recommendation"><strong>Recommendation:</strong> ${escapeHtml(props.recommendation)}</p>` : ""}</section>`;
}

function renderBrief(props: LiquidBriefProps): string | null {
  if (!props?.sections?.length) return null;
  const sources = props.sources?.length
    ? `<ol class="medousa-liquid__sources">${props.sources.map((source) => `<li>${source.url ? link(source.title, source.url) : escapeHtml(source.title)}${source.quote ? ` — ${escapeHtml(source.quote)}` : ""}</li>`).join("")}</ol>`
    : "";
  return `<article class="medousa-liquid__surface">${header(props.title, props.subtitle, props.tone || "Brief")}${props.sections.map((section) => `<section class="medousa-liquid__brief-section"><h4>${escapeHtml(section.heading)}</h4>${markdown(section.body)}</section>`).join("")}${sources}</article>`;
}

function renderDashboard(props: LiquidDashboardProps, options: LiquidBrowserStaticRenderOptions): string | null {
  if (!props?.tiles || props.tiles.length < 2) return null;
  const requested = Number.parseInt(props.columns ?? "", 10);
  const columns = Number.isFinite(requested) ? Math.max(1, Math.min(4, requested)) : 2;
  return `<section class="medousa-liquid__surface">${header(props.title, props.subtitle, "Dashboard")}<div class="medousa-liquid__dashboard" style="--liquid-columns:${columns}">${props.tiles.map((tile) => `<article class="medousa-liquid__metric"><div class="medousa-liquid__card-heading">${visual(tile, options)}<div class="medousa-liquid__card-copy"><div class="medousa-liquid__metric-label">${escapeHtml(tile.label)}</div><div class="medousa-liquid__metric-value">${escapeHtml(`${tile.value}${tile.unit ? ` ${tile.unit}` : ""}`)}</div></div></div>${tile.delta ? `<div class="medousa-liquid__metric-delta">${escapeHtml(tile.delta)}</div>` : ""}${tile.hint ? `<div class="medousa-liquid__meta">${escapeHtml(tile.hint)}</div>` : ""}</article>`).join("")}</div></section>`;
}

function renderChart(props: LiquidChartProps): string | null {
  const chart = renderLiquidChart(props);
  if (!chart) return null;
  return `<section class="medousa-liquid__surface">${header(props.title, props.description, props.type)}${chart}</section>`;
}

function renderReport(props: LiquidReportProps): string | null {
  if (!props?.body && !props?.title) return null;
  return `<article class="medousa-liquid__surface">${header(props.title, props.subtitle, "Report")}<div class="medousa-liquid__report-body" data-liquid-report-columns="${escapeAttr(props.columns ?? "1")}">${markdown(props.body)}</div></article>`;
}

function renderSlides(props: LiquidSlidesProps, options: LiquidBrowserStaticRenderOptions): string | null {
  if (!props?.slides?.length) return null;
  const showAll = props.showAll === true;
  return `<section class="medousa-liquid__surface">${header(props.title, undefined, props.theme || "Slides")}<div class="medousa-liquid__slides${showAll ? " medousa-liquid__slides--all" : ""}" data-liquid-slides>${props.slides.map((slide, index) => {
    const background = safeMediaUrl(slide.bg, options);
    const backgroundImage = background
      ? `<img class="medousa-liquid__slide-bg" src="${escapeAttr(background)}" alt="" loading="lazy">`
      : "";
    return `<article class="medousa-liquid__slide" data-liquid-slide="${index}"${!showAll && index > 0 ? " hidden" : ""}>${backgroundImage}<h4 class="medousa-liquid__slide-label">${escapeHtml(slide.label)}</h4>${markdown(slide.body)}</article>`;
  }).join("")}${showAll ? "" : `<div class="medousa-liquid__slide-controls"><button class="medousa-liquid__icon-button" type="button" data-liquid-slide-nav="prev" aria-label="Previous slide">‹</button><span class="medousa-liquid__slide-count" data-liquid-slide-count>1 / ${props.slides.length}</span><button class="medousa-liquid__icon-button" type="button" data-liquid-slide-nav="next" aria-label="Next slide">›</button></div>`}</div></section>`;
}

function initialTab(props: LiquidTabsProps): number {
  if (!props.default) return 0;
  const numeric = Number.parseInt(props.default, 10);
  if (Number.isFinite(numeric) && numeric > 0 && numeric <= props.panels.length) return numeric - 1;
  const normalized = props.default.trim().toLowerCase();
  const match = props.panels.findIndex((panel) => panel.id.toLowerCase() === normalized || panel.label.toLowerCase() === normalized);
  return match >= 0 ? match : 0;
}

function renderTabs(props: LiquidTabsProps): string | null {
  if (!props?.panels || props.panels.length < 2) return null;
  const active = initialTab(props);
  return `<section class="medousa-liquid__surface">${header(props.title, props.subtitle)}<div class="medousa-liquid__tabs" data-liquid-tabs><div class="medousa-liquid__tab-list" role="tablist">${props.panels.map((panel, index) => `<button class="medousa-liquid__tab" type="button" role="tab" data-liquid-tab="${index}" aria-selected="${index === active}">${panel.emoji ? `${escapeHtml(panel.emoji)} ` : panel.icon ? `${renderLiquidIconHtml(panel.icon) ?? ""} ` : ""}${escapeHtml(panel.label)}</button>`).join("")}</div>${props.panels.map((panel, index) => `<section class="medousa-liquid__tab-panel" role="tabpanel" data-liquid-tab-panel="${index}"${index === active ? "" : " hidden"}>${markdown(panel.body)}</section>`).join("")}</div></section>`;
}

function renderSteps(props: LiquidStepsProps): string | null {
  if (!props?.steps || props.steps.length < 2) return null;
  return `<section class="medousa-liquid__surface">${header(props.title, props.subtitle)}<div class="medousa-liquid__steps">${props.steps.map((step, index) => {
    const status = step.status && ["done", "current", "pending"].includes(step.status)
      ? step.status
      : "pending";
    const marker = step.emoji ? escapeHtml(step.emoji) : step.icon ? renderLiquidIconHtml(step.icon) ?? String(index + 1) : status === "done" ? "✓" : String(index + 1);
    return `<div class="medousa-liquid__step medousa-liquid__step--${escapeAttr(status)}"><span class="medousa-liquid__step-marker">${marker}</span><div><strong>${escapeHtml(step.label)}</strong>${markdown(step.body)}</div></div>`;
  }).join("")}</div></section>`;
}

function renderAccordion(props: LiquidAccordionProps): string | null {
  if (!props?.items?.length) return null;
  return `<section class="medousa-liquid__surface">${header(props.title, props.subtitle)}<div class="medousa-liquid__accordion" data-liquid-accordion data-liquid-accordion-multiple="${props.multiple === true ? "true" : "false"}">${props.items.map((item) => `<details class="medousa-liquid__accordion-item" data-liquid-accordion-item${item.open ? " open" : ""}><summary>${item.emoji ? `${escapeHtml(item.emoji)} ` : item.icon ? `${renderLiquidIconHtml(item.icon) ?? ""} ` : ""}${escapeHtml(item.label)}</summary><div class="medousa-liquid__accordion-body">${markdown(item.body)}</div></details>`).join("")}</div></section>`;
}

function renderCode(props: LiquidCodeProps): string | null {
  if (!props?.source?.trim()) return null;
  const code = props.diff
    ? props.source.split("\n").map((line) => {
        const className = line.startsWith("+") ? "medousa-liquid__diff-add" : line.startsWith("-") ? "medousa-liquid__diff-remove" : "";
        return className ? `<span class="${className}">${escapeHtml(line)}</span>` : escapeHtml(line);
      }).join("\n")
    : escapeHtml(props.source);
  const label = props.title || props.lang || "Code";
  return `<section class="medousa-liquid__surface medousa-liquid__code"><div class="medousa-liquid__code-head"><span>${escapeHtml(label)}</span>${props.copy === false ? "" : `<button class="medousa-liquid__copy" type="button" data-liquid-copy>Copy</button>`}</div><pre><code data-liquid-code>${code}</code></pre></section>`;
}

function renderTreeNodes(nodes: LiquidTreeNode[], depth = 0): string {
  if (depth > 20) return "";
  return `<ul>${nodes.map((node) => {
    const name = escapeHtml(node.name);
    if (node.kind === "folder") {
      return `<li><details open><summary><span aria-hidden="true">▸</span>${name}</summary>${node.children?.length ? renderTreeNodes(node.children, depth + 1) : ""}</details></li>`;
    }
    return `<li><span class="medousa-liquid__tree-file"><span aria-hidden="true">·</span>${name}</span></li>`;
  }).join("")}</ul>`;
}

function renderTree(props: LiquidTreeProps): string | null {
  if (!props?.nodes?.length) return null;
  return `<section class="medousa-liquid__surface">${header(props.title, props.subtitle)}<div class="medousa-liquid__tree">${renderTreeNodes(props.nodes)}</div></section>`;
}

function renderFeed(props: LiquidFeedProps): string | null {
  if (!props?.feedId || !props.datatype) return null;
  return `<section class="medousa-liquid__surface">${header(props.title, undefined, "Live feed")}<div class="medousa-liquid__feed" data-liquid-feed data-liquid-feed-id="${escapeAttr(props.feedId)}" data-liquid-feed-type="${escapeAttr(props.datatype)}" data-liquid-feed-refresh="${escapeAttr(props.refresh ?? "manual")}" data-liquid-feed-empty="${escapeAttr(props.empty ?? "No feed data yet.")}"><div class="medousa-liquid__feed-state">${escapeHtml(props.refresh === "load" ? "Loading…" : props.empty ?? "Ready to load.")}</div>${props.refresh === "load" ? "" : `<button class="medousa-liquid__feed-refresh" type="button" data-liquid-feed-load>Refresh</button>`}</div></section>`;
}

function renderKind(
  kind: LiquidEmbedKind,
  payload: unknown,
  options: LiquidBrowserStaticRenderOptions,
): string | null {
  switch (kind) {
    case "card": return renderCard(payload as LiquidCardProps, options);
    case "carousel": return renderCarousel(payload, options);
    case "actions": return renderActions(payload, options);
    case "callout": return renderCallout(payload);
    case "section": return renderSection(payload as LiquidSectionProps);
    case "block": return renderBlock(payload as LiquidBlockProps);
    case "chips": return renderChips(payload);
    case "media": return renderMedia(payload as LiquidMediaProps, options);
    case "cite": return renderCite(payload as LiquidCiteProps);
    case "compare": return renderCompare(payload as LiquidCompareProps);
    case "plan": return renderPlan(payload as LiquidPlanProps, options);
    case "timeline": return renderTimeline(payload as LiquidTimelineProps, options);
    case "shortlist": return renderShortlist(payload as LiquidShortlistProps, options);
    case "decision": return renderDecision(payload as LiquidDecisionProps);
    case "brief": return renderBrief(payload as LiquidBriefProps);
    case "dashboard": return renderDashboard(payload as LiquidDashboardProps, options);
    case "chart": return renderChart(payload as LiquidChartProps);
    case "report": return renderReport(payload as LiquidReportProps);
    case "slides": return renderSlides(payload as LiquidSlidesProps, options);
    case "tabs": return renderTabs(payload as LiquidTabsProps);
    case "steps": return renderSteps(payload as LiquidStepsProps);
    case "accordion": return renderAccordion(payload as LiquidAccordionProps);
    case "code": return renderCode(payload as LiquidCodeProps);
    case "tree": return renderTree(payload as LiquidTreeProps);
    case "feed": return renderFeed(payload as LiquidFeedProps);
  }
}

/** Render a decoded Liquid payload into escaped, host-neutral HTML. */
export function renderLiquidEmbedHtml(
  kind: LiquidEmbedKind,
  payload: unknown,
  options: LiquidBrowserStaticRenderOptions = {},
): string | null {
  try {
    const body = renderKind(kind, payload, options);
    if (!body) return null;
    return `<div class="medousa-liquid medousa-liquid--${escapeAttr(kind)}" data-liquid-kind="${escapeAttr(kind)}" data-liquid-animate="${options.animate === false ? "false" : "true"}">${body}</div>`;
  } catch {
    return null;
  }
}
