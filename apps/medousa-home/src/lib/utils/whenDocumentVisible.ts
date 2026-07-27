/**
 * Run work only while the document (WebView) is visible.
 * Hidden pre-created popouts on desktop must not connect to the daemon / mount
 * heavy UI until the window is actually shown.
 */
export function whenDocumentVisible(start: () => () => void): () => void {
  let stopActive: (() => void) | null = null;

  const ensure = () => {
    if (typeof document === "undefined") return;
    if (document.visibilityState === "visible") {
      if (!stopActive) stopActive = start();
      return;
    }
    if (stopActive) {
      stopActive();
      stopActive = null;
    }
  };

  ensure();
  if (typeof document === "undefined") return () => {};

  document.addEventListener("visibilitychange", ensure);
  return () => {
    document.removeEventListener("visibilitychange", ensure);
    if (stopActive) {
      stopActive();
      stopActive = null;
    }
  };
}
