export type SpotlightPointer = { buttonsDown: boolean; inside: boolean };

/** Losing focus is not dismissal while the operator is holding a drag source. */
export function spotlightDismissal(options: {
  pointer: () => Promise<SpotlightPointer | null>;
  canDismiss: () => boolean;
  dismiss: () => void;
  releaseInside: () => void;
}) {
  let generation = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let dragging = false;

  function cancel() {
    generation++;
    clearTimeout(timer);
    timer = undefined;
  }
  async function check(owner: number, held: boolean) {
    if (owner !== generation || !options.canDismiss()) return;
    let pointer: SpotlightPointer | null;
    try { pointer = await options.pointer(); }
    catch { pointer = null; } // Unsupported/failed native probe retains ordinary blur dismissal.
    if (owner !== generation || !options.canDismiss()) return;
    if (pointer?.buttonsDown) {
      // This follows button release, not an arbitrary dismissal timeout. No polling while focused.
      timer = setTimeout(() => void check(owner, true), 32);
    } else if (!pointer || !(held || dragging) || !pointer.inside) {
      cancel();
      dragging = false;
      options.dismiss();
    } else {
      // A release over the launcher is an intentional return, even if the drag has no file.
      dragging = false;
      options.releaseInside();
    }
  }
  return {
    focusChanged(focused: boolean) {
      cancel();
      if (focused) dragging = false;
      else void check(generation, dragging);
    },
    dragChanged(type: "enter" | "over" | "drop" | "leave") {
      if (type === "enter" || type === "over") dragging = true;
      if (type === "drop") {
        cancel(); dragging = true;
        if (options.canDismiss()) options.releaseInside();
      }
    },
    cancel() { cancel(); dragging = false; },
  };
}
