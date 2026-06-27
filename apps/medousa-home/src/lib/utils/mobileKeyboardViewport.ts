/** Keeps floating mobile composers above the keyboard without shifting the whole shell. */
export function attachMobileKeyboardViewport(
  root: HTMLElement = document.documentElement,
): () => void {
  if (typeof window === "undefined") return () => {};

  let layoutHeight = window.innerHeight;

  const update = () => {
    const viewport = window.visualViewport;
    if (!viewport) {
      root.style.setProperty("--mobile-keyboard-inset", "0px");
      return;
    }

    const inset = Math.max(0, layoutHeight - viewport.height - viewport.offsetTop);
    root.style.setProperty("--mobile-keyboard-inset", `${inset}px`);

    if (
      inset < 8 &&
      root.dataset.mobileComposerActive !== "true" &&
      root.dataset.mobileBrowserUrlActive !== "true"
    ) {
      layoutHeight = window.innerHeight;
    }
  };

  update();
  window.visualViewport?.addEventListener("resize", update);
  window.visualViewport?.addEventListener("scroll", update);
  window.addEventListener("orientationchange", () => {
    layoutHeight = window.innerHeight;
    update();
  });
  window.addEventListener("resize", update);

  return () => {
    window.visualViewport?.removeEventListener("resize", update);
    window.visualViewport?.removeEventListener("scroll", update);
    window.removeEventListener("orientationchange", update);
    window.removeEventListener("resize", update);
    root.style.removeProperty("--mobile-keyboard-inset");
    root.dataset.mobileComposerActive = "false";
  };
}

/** Focus-driven dock mode — tabs hide while typing; keyboard inset lifts bottom chrome. */
export function setMobileComposerFocus(active: boolean) {
  if (typeof document === "undefined") return;
  document.documentElement.dataset.mobileComposerActive = active ? "true" : "false";
}

/** Web tab URL bar — float bottom chrome above keyboard without resizing native embed. */
export function setMobileBrowserUrlFocus(active: boolean) {
  if (typeof document === "undefined") return;
  document.documentElement.dataset.mobileBrowserUrlActive = active ? "true" : "false";
}

export function isMobileBrowserUrlFocused(): boolean {
  if (typeof document === "undefined") return false;
  return document.documentElement.dataset.mobileBrowserUrlActive === "true";
}

function isLayoutDebugEnabled(): boolean {
  if (typeof document === "undefined") return false;
  return document.documentElement.dataset.layoutDebug === "true";
}

function probeEnvSafeAreaBottom(): number {
  const probe = document.createElement("div");
  probe.style.cssText =
    "position:fixed;visibility:hidden;pointer-events:none;padding-bottom:env(safe-area-inset-bottom);";
  document.body.appendChild(probe);
  const value = parseFloat(getComputedStyle(probe).paddingBottom) || 0;
  probe.remove();
  return value;
}

function measureBottomChromeContentHeight(chromeEl: HTMLElement): number {
  let height = 0;
  for (const child of chromeEl.children) {
    if (!(child instanceof HTMLElement)) continue;
    if (getComputedStyle(child).display === "none") continue;
    height += child.getBoundingClientRect().height;
  }
  return Math.ceil(height);
}

function attachMobileLayoutDebugHud(chromeEl: HTMLElement): () => void {
  const hud = document.createElement("div");
  hud.className = "mobile-layout-debug-hud";
  hud.setAttribute("aria-hidden", "true");
  document.body.appendChild(hud);

  const update = () => {
    const viewport = window.visualViewport;
    const viewportHeight = viewport?.height ?? window.innerHeight;
    const chromeRect = chromeEl.getBoundingClientRect();
    const tabBar = chromeEl.querySelector<HTMLElement>(".mobile-tab-bar-inner");
    const tabRect = tabBar?.getBoundingClientRect();
    const chromeStyle = getComputedStyle(chromeEl);
    const contentHeight = measureBottomChromeContentHeight(chromeEl);
    const envBottom = probeEnvSafeAreaBottom();
    const reserved = getComputedStyle(document.documentElement).getPropertyValue(
      "--mobile-bottom-chrome-height",
    );

    hud.textContent = [
      "Layout debug (set data-layout-debug=true on html)",
      `innerHeight: ${window.innerHeight}px`,
      `visualViewport: ${viewportHeight.toFixed(1)}px`,
      `env(safe-area-inset-bottom): ${envBottom}px`,
      "",
      `chrome bottom: ${chromeRect.bottom.toFixed(1)}px`,
      `tab bar bottom: ${tabRect ? tabRect.bottom.toFixed(1) : "n/a"}px`,
      `gap chrome→viewport: ${(viewportHeight - chromeRect.bottom).toFixed(1)}px`,
      "",
      `chrome padding-bottom: ${chromeStyle.paddingBottom}`,
      `chrome offsetHeight: ${chromeEl.offsetHeight}px`,
      `content height (children): ${contentHeight}px`,
      `--mobile-bottom-chrome-height: ${reserved.trim() || "unset"}`,
    ].join("\n");
  };

  update();
  const observer = new ResizeObserver(update);
  observer.observe(chromeEl);
  window.visualViewport?.addEventListener("resize", update);
  window.visualViewport?.addEventListener("scroll", update);
  window.addEventListener("orientationchange", update);

  return () => {
    observer.disconnect();
    window.visualViewport?.removeEventListener("resize", update);
    window.visualViewport?.removeEventListener("scroll", update);
    window.removeEventListener("orientationchange", update);
    hud.remove();
  };
}

/** Sync `--mobile-bottom-chrome-height` with the fixed bottom chrome element. */
export function attachMobileBottomChromeLayout(
  chromeEl: HTMLElement | null | undefined,
): () => void {
  if (typeof window === "undefined" || !chromeEl) return () => {};

  const root = document.documentElement;
  let raf = 0;

  const update = () => {
    cancelAnimationFrame(raf);
    raf = requestAnimationFrame(() => {
      root.style.setProperty("--mobile-bottom-chrome-height", `${chromeEl.offsetHeight}px`);
    });
  };

  update();
  const observer = new ResizeObserver(update);
  observer.observe(chromeEl);
  for (const child of chromeEl.children) {
    if (child instanceof HTMLElement) observer.observe(child);
  }
  window.addEventListener("orientationchange", update);

  const stopDebug = isLayoutDebugEnabled() ? attachMobileLayoutDebugHud(chromeEl) : () => {};

  return () => {
    cancelAnimationFrame(raf);
    observer.disconnect();
    window.removeEventListener("orientationchange", update);
    root.style.removeProperty("--mobile-bottom-chrome-height");
    stopDebug();
  };
}
