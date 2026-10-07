/** Follows rendered geometry, while keeping user reading intent separate. */
export class ChatScrollFollower {
  private following = true;
  private frame = 0;
  private lastTop: number;
  private lastHeight: number;
  private smoothTarget: number | null = null;
  private touchY: number | null = null;
  private forcePending = false;
  private behavior: ScrollBehavior = "auto";
  private readingAnchor: HTMLElement | null = null;
  private readingOffset = 0;

  constructor(
    private readonly root: HTMLElement,
    private readonly onFollowingChange: (following: boolean) => void,
    private readonly afterLayout: () => void,
  ) {
    this.lastTop = root.scrollTop;
    this.lastHeight = root.scrollHeight;
    root.addEventListener("wheel", this.onWheel, { passive: true });
    root.addEventListener("touchstart", this.onTouchStart, { passive: true });
    root.addEventListener("touchmove", this.onTouchMove, { passive: true });
    root.addEventListener("touchend", this.onTouchEnd, { passive: true });
    root.addEventListener("touchcancel", this.onTouchEnd, { passive: true });
    root.addEventListener("keydown", this.onKeyDown);
    this.setFollowing(true);
  }

  private setFollowing(value: boolean) {
    this.following = value;
    // Native anchoring helps readers; explicit bottom following owns pinning.
    this.root.style.overflowAnchor = value ? "none" : "";
    this.onFollowingChange(value);
  }

  private pause() {
    this.forcePending = false;
    if (this.smoothTarget !== null) {
      this.root.scrollTo({ top: this.root.scrollTop, behavior: "auto" });
      this.smoothTarget = null;
    }
    this.lastTop = this.root.scrollTop;
    this.setFollowing(false);
    this.captureReadingAnchor();
  }

  private onWheel = (event: WheelEvent) => {
    if (event.deltaY < 0) this.pause();
  };
  private onTouchStart = (event: TouchEvent) => {
    this.touchY = event.touches[0]?.clientY ?? null;
  };
  private onTouchMove = (event: TouchEvent) => {
    const y = event.touches[0]?.clientY ?? null;
    if (y !== null && this.touchY !== null && y > this.touchY + 1) this.pause();
    this.touchY = y;
  };
  private onTouchEnd = () => { this.touchY = null; };
  private onKeyDown = (event: KeyboardEvent) => {
    const target = event.target as HTMLElement | null;
    if (target?.closest("input, textarea, [contenteditable=true]")) return;
    if (["ArrowUp", "PageUp", "Home"].includes(event.key) || (event.key === " " && event.shiftKey)) {
      this.pause();
    }
  };

  onScroll() {
    const top = this.root.scrollTop;
    const height = this.root.scrollHeight;
    const bottom = Math.max(0, height - this.root.clientHeight);
    if (this.smoothTarget !== null) {
      if (Math.abs(top - Math.min(bottom, this.smoothTarget)) <= 1) {
        this.smoothTarget = null;
        this.schedule();
      }
    } else if (this.following && top < this.lastTop - 1 && height >= this.lastHeight) {
      // Scrollbar dragging or accessibility scrolling also disengages follow.
      this.pause();
    } else if (!this.following && top > this.lastTop && bottom - top <= 2) {
      this.setFollowing(true);
    }
    this.lastTop = top;
    this.lastHeight = height;
    if (!this.following) this.captureReadingAnchor();
  }

  private captureReadingAnchor() {
    const viewport = this.root.getBoundingClientRect();
    // Prefer a retained Markdown block to a whole turn whose contents may grow.
    let candidates = this.root.querySelectorAll<HTMLElement>(
      "[data-stable-markdown-block] > *, [data-streaming-markdown-tail] > *",
    );
    if (!candidates.length) candidates = this.root.querySelectorAll("[data-chat-history-anchor]");
    this.readingAnchor = null;
    for (const element of candidates) {
      const rect = element.getBoundingClientRect();
      if (rect.bottom > viewport.top && rect.top < viewport.bottom) {
        this.readingAnchor = element;
        this.readingOffset = rect.top - viewport.top;
        break;
      }
    }
  }

  layoutChanged() {
    this.schedule();
  }

  request(force = false, behavior: ScrollBehavior = "auto") {
    if (!force && !this.following) return;
    if (force) {
      this.setFollowing(true);
      this.forcePending = true;
      this.behavior = behavior;
    }
    this.schedule();
  }

  private schedule() {
    if (this.frame) return;
    this.frame = requestAnimationFrame(() => {
      this.frame = 0;
      if (this.following && this.smoothTarget === null) {
        const target = Math.max(0, this.root.scrollHeight - this.root.clientHeight);
        const behavior = this.forcePending ? this.behavior : "auto";
        if (Math.abs(this.root.scrollTop - target) > 1) {
          if (behavior === "smooth") this.smoothTarget = target;
          this.root.scrollTo({ top: target, behavior });
        }
        this.lastTop = this.root.scrollTop;
        this.lastHeight = this.root.scrollHeight;
      } else if (!this.following) {
        const anchor = this.readingAnchor;
        if (anchor?.isConnected && this.root.contains(anchor)) {
          const offset = anchor.getBoundingClientRect().top - this.root.getBoundingClientRect().top;
          const delta = offset - this.readingOffset;
          // If native anchoring already restored the view, delta is zero.
          if (Math.abs(delta) > 1) this.root.scrollTop += delta;
        }
        this.lastTop = this.root.scrollTop;
        this.lastHeight = this.root.scrollHeight;
        this.captureReadingAnchor();
      }
      this.forcePending = false;
      this.afterLayout();
    });
  }

  reset() {
    if (this.frame) cancelAnimationFrame(this.frame);
    if (this.smoothTarget !== null) this.root.scrollTo({ top: this.root.scrollTop, behavior: "auto" });
    this.frame = 0;
    this.forcePending = false;
    this.smoothTarget = null;
    this.readingAnchor = null;
    this.lastTop = this.root.scrollTop;
    this.lastHeight = this.root.scrollHeight;
    this.setFollowing(true);
    // Switching chats can replace content without changing its total height.
    this.schedule();
  }

  stopFollowing() { this.pause(); }

  destroy() {
    if (this.frame) cancelAnimationFrame(this.frame);
    this.frame = 0;
    if (this.smoothTarget !== null) this.root.scrollTo({ top: this.root.scrollTop, behavior: "auto" });
    this.smoothTarget = null;
    this.readingAnchor = null;
    this.root.removeEventListener("wheel", this.onWheel);
    this.root.removeEventListener("touchstart", this.onTouchStart);
    this.root.removeEventListener("touchmove", this.onTouchMove);
    this.root.removeEventListener("touchend", this.onTouchEnd);
    this.root.removeEventListener("touchcancel", this.onTouchEnd);
    this.root.removeEventListener("keydown", this.onKeyDown);
    this.root.style.overflowAnchor = "";
  }
}
