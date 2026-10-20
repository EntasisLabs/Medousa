/** Stop WebKit's image gesture before handing a real drag to the native window. */
export function createPetGesture(startDragging: () => Promise<void>, onError: (cause: unknown) => void) {
  let suppressClick = false;
  let cleanup: (() => void) | null = null;
  return {
    pointerdown(event: PointerEvent) {
      if (event.button !== 0 || !event.isPrimary) return;
      cleanup?.();
      suppressClick = false;
      event.preventDefault();
      const startX = event.clientX;
      const startY = event.clientY;
      const pointerId = event.pointerId;
      const stop = () => {
        window.removeEventListener("pointermove", move);
        window.removeEventListener("pointerup", end);
        window.removeEventListener("pointercancel", end);
        cleanup = null;
      };
      const end = (next: PointerEvent) => { if (next.pointerId === pointerId) stop(); };
      const move = (next: PointerEvent) => {
        if (next.pointerId !== pointerId || Math.hypot(next.clientX - startX, next.clientY - startY) < 3) return;
        next.preventDefault();
        suppressClick = true;
        stop();
        // The window handle is already loaded: don't import a module during the gesture.
        void startDragging().catch(onError);
      };
      cleanup = stop;
      window.addEventListener("pointermove", move, { passive: false });
      window.addEventListener("pointerup", end);
      window.addEventListener("pointercancel", end);
    },
    click(event: MouseEvent): boolean {
      // Keyboard activation must work even if native dragging consumed mouseup/click.
      if (event.detail === 0) return true;
      const allow = !suppressClick;
      suppressClick = false;
      return allow;
    },
    destroy() { cleanup?.(); },
  };
}
