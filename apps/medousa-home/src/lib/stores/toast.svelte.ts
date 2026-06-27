type ToastAction = {
  label: string;
  onAction: () => void;
};

class ToastStore {
  message = $state<string | null>(null);
  action = $state<ToastAction | null>(null);
  private timer: ReturnType<typeof setTimeout> | null = null;

  show(
    text: string,
    options?: {
      durationMs?: number;
      actionLabel?: string;
      onAction?: () => void;
    },
  ) {
    this.message = text;
    if (options?.actionLabel && options.onAction) {
      this.action = { label: options.actionLabel, onAction: options.onAction };
    } else {
      this.action = null;
    }
    if (this.timer) clearTimeout(this.timer);
    this.timer = setTimeout(() => {
      this.dismiss();
    }, options?.durationMs ?? 2800);
  }

  dismiss() {
    this.message = null;
    this.action = null;
    if (this.timer) clearTimeout(this.timer);
    this.timer = null;
  }
}

export const toast = new ToastStore();
