/**
 * Attach outside-click / Escape dismiss for composer popovers.
 * Defers the click listener so the opening click cannot immediately close the menu.
 */
export function attachComposerMenuDismiss(options: {
  isInside: (target: Node | null) => boolean;
  onDismiss: () => void;
}): () => void {
  const onKey = (event: KeyboardEvent) => {
    if (event.key === "Escape") options.onDismiss();
  };

  let removeClick: (() => void) | null = null;
  const timer = window.setTimeout(() => {
    const onDocClick = (event: MouseEvent) => {
      if (options.isInside(event.target as Node | null)) return;
      options.onDismiss();
    };
    // Inspect the original DOM boundary before an action handler can replace or
    // remove its clicked element. Bubble-phase inspection can mistake those
    // internal transitions for outside clicks and dismiss the entire popover.
    document.addEventListener("click", onDocClick, true);
    removeClick = () => document.removeEventListener("click", onDocClick, true);
  }, 0);

  document.addEventListener("keydown", onKey);

  return () => {
    window.clearTimeout(timer);
    removeClick?.();
    document.removeEventListener("keydown", onKey);
  };
}
