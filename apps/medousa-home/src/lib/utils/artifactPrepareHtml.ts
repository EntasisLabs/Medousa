export type ArtifactEmbedMode = "inline" | "panel" | "fullscreen";

const THEME_STYLE_ID = "medousa-artifact-theme";
const MODE_STYLE_ID = "medousa-artifact-mode";

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
  if (feedState && Object.keys(feedState).length > 0) {
    const feedScript = `<script>window.__MEDOUSA_FEED__=${JSON.stringify(feedState)};</script>`;
    html = injectBeforeHeadClose(html, feedScript);
  }
  return html;
}

export function measureIframeContentHeight(frame: HTMLIFrameElement): number {
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
