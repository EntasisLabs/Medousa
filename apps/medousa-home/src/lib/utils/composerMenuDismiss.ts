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
    document.addEventListener("click", onDocClick);
    removeClick = () => document.removeEventListener("click", onDocClick);
  }, 0);

  document.addEventListener("keydown", onKey);

  return () => {
    window.clearTimeout(timer);
    removeClick?.();
    document.removeEventListener("keydown", onKey);
  };
}
