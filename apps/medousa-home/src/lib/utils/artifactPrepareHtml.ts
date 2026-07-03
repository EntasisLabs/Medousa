import {
  buildMedousaArtifactRuntimeClientScript,
  MEDOUSA_ARTIFACT_RUNTIME_SCRIPT_ID,
} from "$lib/utils/medousaArtifactRuntimeClient";
import {
  buildMedousaFeedClientScript,
  MEDOUSA_FEED_CLIENT_SCRIPT_ID,
} from "$lib/utils/medousaFeedClient";
import {
  buildMedousaStoreBootstrapScript,
  buildMedousaStoreClientScript,
  MEDOUSA_STORE_BOOTSTRAP_SCRIPT_ID,
  MEDOUSA_STORE_CLIENT_SCRIPT_ID,
} from "$lib/utils/medousaStoreClient";

export type ArtifactEmbedMode = "inline" | "panel" | "fullscreen";

const THEME_STYLE_ID = "medousa-artifact-theme";
const MODE_STYLE_ID = "medousa-artifact-mode";
const FEED_CLIENT_STYLE_ID = "medousa-feed-client-style";
const ARTIFACT_METRICS_SCRIPT_ID = "medousa-artifact-metrics-script";

function buildArtifactMetricsScript(): string {
  const source = `(function(){
if(window.__MEDOUSA_ARTIFACT_METRICS__)return;
window.__MEDOUSA_ARTIFACT_METRICS__=true;
function report(){
  var h=Math.max(
    document.documentElement.scrollHeight||0,
    document.documentElement.offsetHeight||0,
    document.body.scrollHeight||0,
    document.body.offsetHeight||0,
    120
  );
  parent.postMessage({type:"medousa:artifact:height",height:h},"*");
}
if(document.readyState==="loading")document.addEventListener("DOMContentLoaded",report);
else report();
if(typeof ResizeObserver!=="undefined"){
  try{new ResizeObserver(report).observe(document.documentElement);}catch(e){}
}
})();`;
  return `<script id="${ARTIFACT_METRICS_SCRIPT_ID}">${source}</script>`;
}

function stripInjectionById(html: string, scriptId: string): string {
  const pattern = new RegExp(
    `<script[^>]*id=["']${scriptId}["'][^>]*>[\\s\\S]*?</script>`,
    "i",
  );
  return html.replace(pattern, "");
}

function buildMedousaFeedClientStyle(): string {
  return `<style id="${FEED_CLIENT_STYLE_ID}">medousa-feed,.medousa-feed-card{display:block;font:13px/1.45 system-ui,sans-serif;color:var(--medousa-host-fg,#f4f4f5)}.medousa-feed-phase{font-weight:600;text-transform:capitalize}.medousa-feed-status,.medousa-feed-time{font-size:12px;color:var(--medousa-host-muted,#a1a1aa)}.medousa-feed-excerpt{margin-top:6px;white-space:pre-wrap;word-break:break-word}</style>`;
}

function injectBeforeHeadClose(html: string, injection: string): string {
  if (html.includes(injection)) return html;
  const lower = html.toLowerCase();
  if (lower.includes("</head>")) {
    return html.replace(/<\/head>/i, `${injection}</head>`);
  }
  if (lower.includes("<head>")) {
    return html.replace(/<head>/i, `<head>${injection}`);
  }
  if (lower.includes("<body")) {
    return html.replace(/<body/i, `<head>${injection}</head><body`);
  }
  return `${injection}${html}`;
}

/** Optional host tokens — artifacts opt in via var(--medousa-host-*); no element overrides. */
export function buildArtifactThemeStyle(isDark: boolean): string {
  const fg = isDark ? "#f4f4f5" : "#18181b";
  const muted = isDark ? "#a1a1aa" : "#52525b";
  return `<style id="${THEME_STYLE_ID}">:root{--medousa-host-bg:transparent;--medousa-host-fg:${fg};--medousa-host-muted:${muted}}</style>`;
}

/** Root-only embed chrome — never styles artifact markup. */
export function buildArtifactModeStyle(mode: ArtifactEmbedMode): string {
  if (mode === "inline") {
    return `<style id="${MODE_STYLE_ID}">html,body{margin:0;padding:0;background:transparent;overflow:hidden}</style>`;
  }
  return `<style id="${MODE_STYLE_ID}">html,body{margin:0;padding:0;background:transparent;overflow:auto;scrollbar-width:thin;-ms-overflow-style:auto}html::-webkit-scrollbar,body::-webkit-scrollbar{width:8px;height:8px}</style>`;
}

export function prepareArtifactHtml(
  raw: string,
  mode: ArtifactEmbedMode,
  isDark: boolean,
  feedState?: Record<string, unknown> | null,
  componentId?: string | null,
): string {
  const themeStyle = buildArtifactThemeStyle(isDark);
  const modeStyle = buildArtifactModeStyle(mode);
  let html = raw;
  if (!html.includes(THEME_STYLE_ID)) {
    html = injectBeforeHeadClose(html, themeStyle);
  }
  if (!html.includes(MODE_STYLE_ID)) {
    html = injectBeforeHeadClose(html, modeStyle);
  }
  if (!html.includes(FEED_CLIENT_STYLE_ID)) {
    html = injectBeforeHeadClose(html, buildMedousaFeedClientStyle());
  }
  if (!html.includes(MEDOUSA_FEED_CLIENT_SCRIPT_ID)) {
    html = injectBeforeHeadClose(html, buildMedousaFeedClientScript());
  }
  if (!html.includes(ARTIFACT_METRICS_SCRIPT_ID)) {
    html = injectBeforeHeadClose(html, buildArtifactMetricsScript());
  }
  if (!html.includes(MEDOUSA_STORE_CLIENT_SCRIPT_ID)) {
    html = injectBeforeHeadClose(html, buildMedousaStoreClientScript());
  }
  if (!html.includes(MEDOUSA_ARTIFACT_RUNTIME_SCRIPT_ID)) {
    html = injectBeforeHeadClose(html, buildMedousaArtifactRuntimeClientScript());
  }
  if (componentId?.trim()) {
    html = stripInjectionById(html, MEDOUSA_STORE_BOOTSTRAP_SCRIPT_ID);
    html = injectBeforeHeadClose(html, buildMedousaStoreBootstrapScript(componentId.trim()));
  }
  if (feedState && Object.keys(feedState).length > 0) {
    const feedScript = `<script>window.__MEDOUSA_FEED__=${JSON.stringify(feedState)};</script>`;
    html = injectBeforeHeadClose(html, feedScript);
  }
  return html;
}

export function measureIframeContentHeight(frame: HTMLIFrameElement): number {
  // Sandboxed srcdoc iframes (no allow-same-origin) block contentDocument — callers
  // should prefer medousa:artifact:height postMessage from the injected metrics script.
  try {
    const doc = frame.contentDocument;
    if (!doc) return 0;
    const docEl = doc.documentElement;
    const body = doc.body;
    return Math.max(
      docEl?.scrollHeight ?? 0,
      docEl?.offsetHeight ?? 0,
      body?.scrollHeight ?? 0,
      body?.offsetHeight ?? 0,
    );
  } catch {
    return 0;
  }
}

export const DEFAULT_INLINE_ARTIFACT_CAP_PX = 480;
export const ARTIFACT_CHROME_BAR_HEIGHT_PX = 48;
