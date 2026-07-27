/** Desktop dock popover for New ask (Work rail). */

class WorkAskDockStore {
  open = $state(false);
  /** Anchor for placeDockPopover (New ask button). */
  anchorEl = $state<HTMLElement | null>(null);

  openDock(anchor?: HTMLElement | null) {
    if (anchor) this.anchorEl = anchor;
    this.open = true;
  }

  closeDock() {
    this.open = false;
  }

  toggleDock(anchor?: HTMLElement | null) {
    if (this.open) {
      this.closeDock();
      return;
    }
    this.openDock(anchor);
  }
}

export const workAskDock = new WorkAskDockStore();
