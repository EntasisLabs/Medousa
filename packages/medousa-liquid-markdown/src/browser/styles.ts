/** Shared, host-theme-aware styles for the framework-independent renderer. */
export const LIQUID_MARKDOWN_STYLES = String.raw`
.medousa-liquid,
.liquid-mini-kanban {
  --liquid-fg: var(--text-normal, var(--vscode-foreground, currentColor));
  --liquid-muted: var(--text-muted, var(--vscode-descriptionForeground, #707070));
  --liquid-bg: var(--background-secondary, var(--vscode-editor-background, rgba(127, 127, 127, .08)));
  --liquid-bg-raised: var(--background-primary, var(--vscode-sideBar-background, rgba(127, 127, 127, .045)));
  --liquid-border: var(--background-modifier-border, var(--vscode-widget-border, rgba(127, 127, 127, .3)));
  --liquid-hover: var(--background-modifier-hover, var(--vscode-toolbar-hoverBackground, rgba(127, 127, 127, .14)));
  --liquid-accent: var(--interactive-accent, var(--vscode-textLink-foreground, #6c78ff));
  --liquid-accent-fg: var(--text-on-accent, var(--vscode-button-foreground, #fff));
  --liquid-positive: var(--color-green, var(--vscode-testing-iconPassed, #42a572));
  --liquid-warning: var(--color-yellow, var(--vscode-charts-yellow, #c99428));
  --liquid-danger: var(--color-red, var(--vscode-errorForeground, #d05050));
  color: var(--liquid-fg);
  font: inherit;
}

.medousa-liquid {
  min-width: 0;
  margin: .7rem 0;
  line-height: 1.45;
}

.medousa-liquid *,
.liquid-mini-kanban * { box-sizing: border-box; }

.medousa-liquid__surface {
  min-width: 0;
  overflow: hidden;
  border: 1px solid var(--liquid-border);
  border-radius: 10px;
  background: var(--liquid-bg-raised);
}

.medousa-liquid__header { padding: 12px 13px 0; }
.medousa-liquid__title { margin: 0; color: var(--liquid-fg); font-size: 1em; font-weight: 700; line-height: 1.3; }
.medousa-liquid__subtitle { margin: 3px 0 0; color: var(--liquid-muted); font-size: .86em; }
.medousa-liquid__kicker { margin: 0 0 4px; color: var(--liquid-muted); font-size: .67em; font-weight: 700; letter-spacing: .08em; text-transform: uppercase; }
.medousa-liquid__markdown { min-width: 0; white-space: pre-wrap; overflow-wrap: anywhere; }
.medousa-liquid__markdown > :first-child { margin-top: 0; }
.medousa-liquid__markdown > :last-child { margin-bottom: 0; }
.medousa-liquid__markdown p { margin: 0 0 .55em; }
.medousa-liquid__markdown ul,
.medousa-liquid__markdown ol { margin: .35em 0 .6em; padding-left: 1.35em; }
.medousa-liquid__markdown pre { overflow: auto; }

.medousa-liquid__card { position: relative; padding: 13px; }
.medousa-liquid__card-image { display: block; width: 100%; max-height: 220px; margin: -13px -13px 12px; width: calc(100% + 26px); object-fit: cover; border-bottom: 1px solid var(--liquid-border); }
.medousa-liquid__card-heading { display: flex; align-items: flex-start; gap: 9px; }
.medousa-liquid__visual { display: inline-grid; width: 27px; height: 27px; flex: 0 0 auto; place-items: center; overflow: hidden; border-radius: 8px; color: var(--liquid-accent); background: color-mix(in srgb, var(--liquid-accent) 13%, transparent); font-size: 15px; }
.medousa-liquid__visual img { width: 100%; height: 100%; object-fit: cover; }
.medousa-liquid__card-copy { min-width: 0; flex: 1; }
.medousa-liquid__card-body { margin-top: 9px; color: var(--liquid-fg); }
.medousa-liquid__meta { margin-top: 8px; color: var(--liquid-muted); font-size: .78em; }
.medousa-liquid__summary { margin: 8px 0 0; color: var(--liquid-muted); font-size: .9em; }
.medousa-liquid__badges,
.medousa-liquid__chips { display: flex; flex-wrap: wrap; gap: 5px; margin-top: 9px; }
.medousa-liquid__badge,
.medousa-liquid__chip { display: inline-flex; align-items: center; min-height: 22px; padding: 2px 7px; border: 1px solid var(--liquid-border); border-radius: 999px; color: var(--liquid-muted); background: var(--liquid-bg); font-size: .74em; line-height: 1.2; }
.medousa-liquid__chip--accent { border-color: color-mix(in srgb, var(--liquid-accent) 55%, var(--liquid-border)); color: var(--liquid-accent); }
.medousa-liquid__chip--success { color: var(--liquid-positive); }
.medousa-liquid__chip--warn { color: var(--liquid-warning); }
.medousa-liquid__points { display: grid; gap: 7px; margin-top: 10px; }
.medousa-liquid__point { display: grid; grid-template-columns: auto minmax(0, 1fr); gap: 8px; padding-top: 7px; border-top: 1px solid var(--liquid-border); }
.medousa-liquid__point strong { display: block; font-size: .84em; }
.medousa-liquid__point span:last-child { color: var(--liquid-muted); font-size: .82em; }

.medousa-liquid__carousel-shell { position: relative; }
.medousa-liquid__carousel { display: grid; grid-auto-columns: minmax(min(85%, 260px), 1fr); grid-auto-flow: column; gap: 9px; overflow-x: auto; padding: 1px 1px 8px; scroll-snap-type: x mandatory; scrollbar-width: thin; }
.medousa-liquid__carousel > .medousa-liquid__surface { scroll-snap-align: start; }
.medousa-liquid__carousel-controls { display: flex; justify-content: flex-end; gap: 4px; margin-bottom: 5px; }
.medousa-liquid__icon-button { display: inline-grid; width: 27px; height: 27px; place-items: center; border: 1px solid var(--liquid-border); border-radius: 7px; color: var(--liquid-fg); background: var(--liquid-bg-raised); cursor: pointer; }
.medousa-liquid__icon-button:hover { background: var(--liquid-hover); }

.medousa-liquid__actions { display: grid; gap: 6px; }
.medousa-liquid__action { display: grid; grid-template-columns: auto minmax(0, 1fr) auto; align-items: center; gap: 8px; width: 100%; padding: 9px 10px; border: 1px solid var(--liquid-border); border-radius: 8px; color: var(--liquid-fg); background: var(--liquid-bg-raised); font: inherit; text-align: left; cursor: pointer; }
.medousa-liquid__action:hover { border-color: var(--liquid-accent); background: var(--liquid-hover); }
.medousa-liquid__action-arrow { color: var(--liquid-muted); }

.medousa-liquid__callout { padding: 11px 12px; border-left: 3px solid var(--liquid-accent); }
.medousa-liquid__callout--warn { border-left-color: var(--liquid-warning); }
.medousa-liquid__callout--error { border-left-color: var(--liquid-danger); }
.medousa-liquid__callout--success,
.medousa-liquid__callout--tip { border-left-color: var(--liquid-positive); }
.medousa-liquid__callout-title { margin: 0 0 4px; font-weight: 700; }

.medousa-liquid__section { padding: 12px 13px; }
.medousa-liquid__section .medousa-liquid__header { padding: 0 0 9px; }
.medousa-liquid__block { padding: 11px 12px; font-family: var(--block-font, inherit); font-size: var(--block-size, inherit); line-height: var(--block-spacing, inherit); text-align: var(--block-align, inherit); }
.medousa-liquid__media { margin: 0; }
.medousa-liquid__media img { display: block; width: 100%; max-height: 440px; object-fit: cover; background: var(--liquid-bg); }
.medousa-liquid__media figcaption { padding: 7px 10px; color: var(--liquid-muted); font-size: .78em; }
.medousa-liquid__cite { padding: 12px 13px; }
.medousa-liquid__quote { margin: 0; font-size: .96em; font-style: italic; }
.medousa-liquid__cite-footer { display: flex; flex-wrap: wrap; gap: 4px 8px; margin-top: 8px; color: var(--liquid-muted); font-size: .78em; }
.medousa-liquid__link { color: var(--liquid-accent); text-decoration: none; }
.medousa-liquid__link:hover { text-decoration: underline; }

.medousa-liquid__grid { display: grid; gap: 9px; padding: 12px 13px 13px; }
.medousa-liquid__grid--2 { grid-template-columns: repeat(2, minmax(0, 1fr)); }
.medousa-liquid__grid--3 { grid-template-columns: repeat(3, minmax(0, 1fr)); }
.medousa-liquid__panel { min-width: 0; padding: 10px; border: 1px solid var(--liquid-border); border-radius: 8px; background: var(--liquid-bg); }
.medousa-liquid__panel-title { margin: 0 0 7px; font-size: .86em; font-weight: 700; }
.medousa-liquid__recommendation { margin: 0 13px 13px; padding: 8px 9px; border-radius: 7px; color: var(--liquid-accent); background: color-mix(in srgb, var(--liquid-accent) 10%, transparent); font-size: .84em; }

.medousa-liquid__table-wrap { overflow-x: auto; padding: 11px 13px 13px; }
.medousa-liquid__table { width: 100%; border-collapse: collapse; font-size: .8em; }
.medousa-liquid__table th,
.medousa-liquid__table td { padding: 7px 8px; border-bottom: 1px solid var(--liquid-border); text-align: left; vertical-align: top; }
.medousa-liquid__table th { color: var(--liquid-muted); font-size: .9em; font-weight: 700; }

.medousa-liquid__rail { position: relative; display: grid; gap: 0; padding: 11px 13px 13px; }
.medousa-liquid__rail-item { position: relative; display: grid; grid-template-columns: 30px minmax(0, 1fr); gap: 8px; padding: 0 0 12px; }
.medousa-liquid__rail-item:last-child { padding-bottom: 0; }
.medousa-liquid__rail-item::before { content: ""; position: absolute; top: 25px; bottom: 0; left: 13px; width: 1px; background: var(--liquid-border); }
.medousa-liquid__rail-item:last-child::before { display: none; }
.medousa-liquid__rail-dot { z-index: 1; display: grid; width: 27px; height: 27px; place-items: center; border: 1px solid var(--liquid-border); border-radius: 50%; color: var(--liquid-accent); background: var(--liquid-bg-raised); font-size: .75em; }
.medousa-liquid__rail-copy { min-width: 0; padding-top: 3px; }
.medousa-liquid__rail-copy strong { display: block; font-size: .84em; }
.medousa-liquid__rail-copy small { color: var(--liquid-muted); }

.medousa-liquid__score { color: var(--liquid-accent); font-size: .78em; font-weight: 700; }
.medousa-liquid__pros-cons { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 8px; }
.medousa-liquid__pros-cons ul { margin: 5px 0 0; padding-left: 1.2em; color: var(--liquid-muted); font-size: .8em; }
.medousa-liquid__pros { color: var(--liquid-positive); }
.medousa-liquid__cons { color: var(--liquid-danger); }

.medousa-liquid__brief-section { padding: 11px 13px; border-top: 1px solid var(--liquid-border); }
.medousa-liquid__brief-section h4 { margin: 0 0 6px; font-size: .86em; }
.medousa-liquid__sources { margin: 0; padding: 10px 13px 12px 30px; border-top: 1px solid var(--liquid-border); color: var(--liquid-muted); font-size: .78em; }

.medousa-liquid__dashboard { display: grid; grid-template-columns: repeat(var(--liquid-columns, 2), minmax(0, 1fr)); gap: 8px; padding: 11px 13px 13px; }
.medousa-liquid__metric { min-width: 0; padding: 10px; border: 1px solid var(--liquid-border); border-radius: 8px; background: var(--liquid-bg); }
.medousa-liquid__metric-label { color: var(--liquid-muted); font-size: .72em; }
.medousa-liquid__metric-value { margin-top: 3px; overflow: hidden; font-size: 1.25em; font-weight: 750; text-overflow: ellipsis; }
.medousa-liquid__metric-delta { margin-top: 3px; color: var(--liquid-positive); font-size: .72em; }

.medousa-liquid__chart { padding: 11px 12px 9px; }
.medousa-liquid__chart-svg { display: block; width: 100%; min-height: 150px; overflow: visible; }
.medousa-liquid__chart-svg text { fill: var(--liquid-muted); font: 10px ui-sans-serif, system-ui, sans-serif; }
.medousa-liquid__chart-grid { stroke: var(--liquid-border); stroke-width: 1; }
.medousa-liquid__legend { display: flex; flex-wrap: wrap; gap: 5px 11px; margin-top: 7px; color: var(--liquid-muted); font-size: .72em; }
.medousa-liquid__legend-item { display: inline-flex; align-items: center; gap: 5px; }
.medousa-liquid__legend-swatch { width: 8px; height: 8px; border-radius: 2px; background: var(--swatch); }
.medousa-liquid__caption { margin: 7px 0 0; color: var(--liquid-muted); font-size: .75em; }
.medousa-liquid__trend { color: var(--liquid-positive); font-size: .78em; font-weight: 700; }
.medousa-liquid__trend--down { color: var(--liquid-danger); }
.medousa-liquid__trend--flat { color: var(--liquid-muted); }

.medousa-liquid__report-body { padding: 11px 13px 13px; }
.medousa-liquid__slides { padding: 11px 12px 12px; }
.medousa-liquid__slide { position: relative; min-height: 190px; overflow: hidden; padding: 17px; border: 1px solid var(--liquid-border); border-radius: 9px; background: linear-gradient(145deg, color-mix(in srgb, var(--liquid-accent) 13%, var(--liquid-bg)), var(--liquid-bg-raised)); }
.medousa-liquid__slide[hidden] { display: none; }
.medousa-liquid__slide > :not(.medousa-liquid__slide-bg) { position: relative; z-index: 1; }
.medousa-liquid__slide-bg { position: absolute; z-index: 0; inset: 0; width: 100%; height: 100%; object-fit: cover; opacity: .42; }
.medousa-liquid__slide-label { margin: 0 0 11px; font-size: 1.12em; font-weight: 750; }
.medousa-liquid__slide-controls { display: grid; grid-template-columns: auto 1fr auto; align-items: center; gap: 8px; margin-top: 8px; }
.medousa-liquid__slide-count { color: var(--liquid-muted); font-size: .72em; text-align: center; }
.medousa-liquid__slides--all .medousa-liquid__slide { display: block; margin-bottom: 9px; }
.medousa-liquid__slides--all .medousa-liquid__slide-controls { display: none; }

.medousa-liquid__tabs { padding: 10px 12px 12px; }
.medousa-liquid__tab-list { display: flex; gap: 4px; overflow-x: auto; margin-bottom: 9px; padding-bottom: 1px; }
.medousa-liquid__tab { flex: 0 0 auto; padding: 5px 8px; border: 1px solid transparent; border-radius: 7px; color: var(--liquid-muted); background: transparent; font: inherit; font-size: .78em; cursor: pointer; }
.medousa-liquid__tab:hover { background: var(--liquid-hover); }
.medousa-liquid__tab[aria-selected="true"] { border-color: var(--liquid-border); color: var(--liquid-fg); background: var(--liquid-bg); }
.medousa-liquid__tab-panel { min-height: 45px; }
.medousa-liquid__tab-panel[hidden] { display: none; }

.medousa-liquid__steps { display: grid; gap: 0; padding: 11px 13px 13px; counter-reset: liquid-step; }
.medousa-liquid__step { position: relative; display: grid; grid-template-columns: 27px minmax(0, 1fr); gap: 8px; padding-bottom: 12px; counter-increment: liquid-step; }
.medousa-liquid__step:last-child { padding-bottom: 0; }
.medousa-liquid__step-marker { display: grid; width: 25px; height: 25px; place-items: center; border: 1px solid var(--liquid-border); border-radius: 50%; color: var(--liquid-muted); background: var(--liquid-bg-raised); font-size: .7em; }
.medousa-liquid__step--done .medousa-liquid__step-marker { border-color: var(--liquid-positive); color: var(--liquid-positive); }
.medousa-liquid__step--current .medousa-liquid__step-marker { border-color: var(--liquid-accent); color: var(--liquid-accent-fg); background: var(--liquid-accent); }
.medousa-liquid__step strong { display: block; padding-top: 3px; font-size: .84em; }
.medousa-liquid__step .medousa-liquid__markdown { margin-top: 4px; color: var(--liquid-muted); font-size: .82em; }

.medousa-liquid__accordion { padding: 8px 11px 11px; }
.medousa-liquid__accordion-item { border-bottom: 1px solid var(--liquid-border); }
.medousa-liquid__accordion-item:last-child { border-bottom: 0; }
.medousa-liquid__accordion-item summary { display: flex; align-items: center; gap: 7px; padding: 8px 2px; font-size: .84em; font-weight: 650; cursor: pointer; }
.medousa-liquid__accordion-body { padding: 0 2px 10px; color: var(--liquid-muted); font-size: .86em; }

.medousa-liquid__code { overflow: hidden; }
.medousa-liquid__code-head { display: flex; align-items: center; justify-content: space-between; gap: 8px; padding: 6px 9px; border-bottom: 1px solid var(--liquid-border); color: var(--liquid-muted); font-size: .72em; }
.medousa-liquid__copy { border: 0; border-radius: 5px; padding: 3px 6px; color: var(--liquid-fg); background: transparent; font: inherit; cursor: pointer; }
.medousa-liquid__copy:hover { background: var(--liquid-hover); }
.medousa-liquid__code pre { margin: 0; padding: 10px; overflow: auto; background: var(--liquid-bg); white-space: pre; }
.medousa-liquid__code code { font-family: var(--font-monospace, var(--vscode-editor-font-family, ui-monospace, monospace)); font-size: .82em; }
.medousa-liquid__diff-add { color: var(--liquid-positive); }
.medousa-liquid__diff-remove { color: var(--liquid-danger); }

.medousa-liquid__tree { padding: 9px 12px 12px; }
.medousa-liquid__tree ul { margin: 0; padding-left: 17px; list-style: none; }
.medousa-liquid__tree > ul { padding-left: 0; }
.medousa-liquid__tree li { margin: 2px 0; }
.medousa-liquid__tree summary,
.medousa-liquid__tree-file { display: flex; align-items: center; gap: 6px; min-height: 23px; font-size: .8em; }
.medousa-liquid__tree summary { cursor: pointer; }

.medousa-liquid__feed { padding: 11px 12px; }
.medousa-liquid__feed-state { color: var(--liquid-muted); font-size: .82em; }
.medousa-liquid__feed-content { color: var(--liquid-fg); }
.medousa-liquid__feed-refresh { margin-top: 8px; padding: 4px 8px; border: 1px solid var(--liquid-border); border-radius: 6px; color: var(--liquid-fg); background: var(--liquid-bg); font: inherit; font-size: .76em; cursor: pointer; }
.medousa-liquid__feed-table { margin-top: 7px; overflow-x: auto; }

.medousa-liquid-icon { display: inline-flex; align-items: center; justify-content: center; min-width: 1em; color: var(--liquid-accent, currentColor); vertical-align: -.08em; }
.liquid-md-embed--error { padding: 7px 9px; border-left: 2px solid var(--liquid-danger, #d05050); color: var(--liquid-danger, #d05050); }

.liquid-mini-kanban { margin: .7rem 0; padding: 10px; overflow-x: auto; border: 1px solid var(--liquid-border); border-radius: 10px; background: var(--liquid-bg-raised); }
.liquid-mini-kanban__label { margin: 0 0 7px; color: var(--liquid-muted); font-size: .67em; font-weight: 700; letter-spacing: .08em; text-transform: uppercase; }
.liquid-mini-kanban__board { display: grid; grid-auto-columns: minmax(150px, 1fr); grid-auto-flow: column; gap: 8px; }
.liquid-mini-kanban__column { min-width: 0; padding: 7px; border-radius: 7px; background: var(--liquid-bg); }
.liquid-mini-kanban__column-title { margin: 0 0 6px; font-size: .78em; font-weight: 700; }
.liquid-mini-kanban__cards { display: grid; gap: 5px; }
.liquid-mini-kanban__card { padding: 6px 7px; border: 1px solid var(--liquid-border); border-radius: 6px; background: var(--liquid-bg-raised); font-size: .76em; }

@media (max-width: 420px) {
  .medousa-liquid__grid--2,
  .medousa-liquid__grid--3,
  .medousa-liquid__pros-cons { grid-template-columns: 1fr; }
  .medousa-liquid__dashboard { grid-template-columns: repeat(min(var(--liquid-columns, 2), 2), minmax(0, 1fr)); }
}

@media (prefers-reduced-motion: no-preference) {
  .medousa-liquid[data-liquid-animate="true"] { animation: medousa-liquid-enter .28s ease-out both; }
  @keyframes medousa-liquid-enter {
    from { opacity: 0; transform: translateY(4px); }
    to { opacity: 1; transform: translateY(0); }
  }
}
`;

const installedStyles = new WeakMap<Document | ShadowRoot, HTMLStyleElement>();

/** Install the shared stylesheet once in a document or shadow root. */
export function installLiquidMarkdownStyles(
  target: Document | ShadowRoot = document,
): HTMLStyleElement {
  const existing = installedStyles.get(target);
  if (existing?.isConnected) return existing;

  // nodeType works across iframe/DOM-library realms; instanceof Document does not.
  const isDocument = target.nodeType === 9;
  const doc = isDocument ? target as Document : target.ownerDocument;
  if (!doc) throw new Error("Liquid Markdown styles require an attached document");
  const style = doc.createElement("style");
  style.dataset.medousaLiquidMarkdown = "1";
  style.textContent = LIQUID_MARKDOWN_STYLES;
  if (isDocument) {
    const documentTarget = target as Document;
    (documentTarget.head ?? documentTarget.documentElement).appendChild(style);
  } else {
    target.appendChild(style);
  }
  installedStyles.set(target, style);
  return style;
}
