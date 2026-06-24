class ToastStore {
  message = $state<string | null>(null);
  private timer: ReturnType<typeof setTimeout> | null = null;

  show(text: string, durationMs = 2800) {
    this.message = text;
    if (this.timer) clearTimeout(this.timer);
    this.timer = setTimeout(() => {
      this.message = null;
      this.timer = null;
    }, durationMs);
  }

  dismiss() {
    this.message = null;
    if (this.timer) clearTimeout(this.timer);
    this.timer = null;
  }
}

export const toast = new ToastStore();
