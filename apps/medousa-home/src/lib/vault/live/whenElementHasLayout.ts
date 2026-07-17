/**
 * Run once an element is connected and has a non-zero layout width.
 * LayerCake charts hydrate empty when mounted at clientWidth === 0.
 */

export type LayoutWaitHandle = {
  cancel: () => void;
};

export function whenElementHasLayout(
  el: HTMLElement,
  run: () => void,
): LayoutWaitHandle {
  let done = false;
  let ro: ResizeObserver | null = null;
  let raf1 = 0;
  let raf2 = 0;
  let timer = 0;

  const finish = (force = false) => {
    if (done) return;
    if (!force && (!el.isConnected || el.clientWidth <= 0)) return;
    done = true;
    cleanup();
    run();
  };

  const cleanup = () => {
    if (ro) {
      ro.disconnect();
      ro = null;
    }
    if (raf1) cancelAnimationFrame(raf1);
    if (raf2) cancelAnimationFrame(raf2);
    if (timer) clearTimeout(timer);
    raf1 = 0;
    raf2 = 0;
    timer = 0;
  };

  finish();
  if (done) {
    return { cancel: () => {} };
  }

  if (typeof ResizeObserver !== "undefined") {
    ro = new ResizeObserver(() => finish());
    ro.observe(el);
  }

  raf1 = requestAnimationFrame(() => {
    raf2 = requestAnimationFrame(() => finish());
  });
  // Last resort so a forever-zero host still paints something.
  timer = window.setTimeout(() => finish(true), 400);

  return {
    cancel: () => {
      done = true;
      cleanup();
    },
  };
}
