const BOOTSTRAP_SPLASH_ID = "medousa-bootstrap-splash";
const BOOTSTRAP_SPLASH_EXIT_MS = 300;

/** Fade the static boot surface only after the destination behind it has mounted. */
export function dismissBootstrapSplash(): void {
  if (typeof document === "undefined") return;

  const splash = document.getElementById(BOOTSTRAP_SPLASH_ID);
  if (!splash || splash.dataset.exitScheduled === "true") return;

  splash.dataset.exitScheduled = "true";
  const beginExit = () => {
    splash.setAttribute("data-exiting", "true");
    window.setTimeout(() => splash.remove(), BOOTSTRAP_SPLASH_EXIT_MS);
  };

  if (typeof window.requestAnimationFrame === "function") {
    window.requestAnimationFrame(beginExit);
  } else {
    beginExit();
  }
}
