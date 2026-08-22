const STATUS_POPOVER_OPEN_EVENT = "medousa-status-popover-open";

export function announceStatusPopoverOpen(id: string): void {
  if (typeof window === "undefined") return;
  window.dispatchEvent(
    new CustomEvent<{ id: string }>(STATUS_POPOVER_OPEN_EVENT, {
      detail: { id },
    }),
  );
}

export function closeOnOtherStatusPopover(
  id: string,
  close: () => void,
): () => void {
  if (typeof window === "undefined") return () => {};
  const onOpen = (event: Event) => {
    const next = (event as CustomEvent<{ id?: string }>).detail?.id;
    if (next && next !== id) close();
  };
  window.addEventListener(STATUS_POPOVER_OPEN_EVENT, onOpen);
  return () => window.removeEventListener(STATUS_POPOVER_OPEN_EVENT, onOpen);
}
