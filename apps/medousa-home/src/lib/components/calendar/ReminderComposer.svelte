<script lang="ts">
  import { ListTodo, X } from "@lucide/svelte";
  import { registerMobileBackHandler } from "$lib/mobileNavigation";
  import { calendar, calendarDateUtils } from "$lib/stores/calendar.svelte";

  interface Props {
    mobile?: boolean;
    onClose: () => void;
  }

  let { mobile = false, onClose }: Props = $props();

  const { isoDay } = calendarDateUtils;

  let title = $state("");
  let dueDay = $state(isoDay(calendar.selectedDay));
  let saving = $state(false);
  let error = $state<string | null>(null);
  let titleEl: HTMLInputElement | undefined = $state();

  $effect(() => {
    dueDay = isoDay(calendar.selectedDay);
  });

  $effect(() => {
    queueMicrotask(() => titleEl?.focus());
  });

  async function submit() {
    saving = true;
    error = null;
    try {
      await calendar.createReminder(title.trim() || "Reminder", dueDay);
      onClose();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      saving = false;
    }
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
      return;
    }
    if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
      event.preventDefault();
      void submit();
    }
  }

  $effect(() => {
    if (!mobile) return;
    return registerMobileBackHandler(() => {
      onClose();
      return true;
    });
  });
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="cal-pop-backdrop"
  class:cal-pop-backdrop-mobile={mobile}
  role="presentation"
  onclick={onClose}
  onkeydown={onKeydown}
>
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="cal-pop"
    class:cal-pop-mobile={mobile}
    role="dialog"
    aria-modal="true"
    aria-label="New reminder"
    onclick={(e) => e.stopPropagation()}
    onkeydown={onKeydown}
  >
    <div class="cal-pop-grab" aria-hidden="true"></div>
    <header class="cal-pop-head">
      <div class="cal-pop-mode">
        <span class="cal-pop-mode-active">New Reminder</span>
      </div>
      <button type="button" class="cal-pop-x" aria-label="Close" onclick={onClose}>
        <X size={15} strokeWidth={1.75} />
      </button>
    </header>

    <div class="cal-pop-card cal-pop-card-title">
      <ListTodo size={16} strokeWidth={1.75} class="cal-pop-row-icon" />
      <input
        bind:this={titleEl}
        class="cal-pop-title"
        bind:value={title}
        placeholder="Reminder"
        maxlength={200}
      />
    </div>

    <div class="cal-pop-card">
      <label class="cal-pop-hint" for="cal-reminder-due">Due</label>
      <input
        id="cal-reminder-due"
        class="cal-pop-field"
        type="date"
        bind:value={dueDay}
      />
      <p class="cal-pop-hint">Stored as a vault checkbox in calendar/reminders.md</p>
    </div>

    {#if error}
      <p class="cal-pop-error">{error}</p>
    {/if}

    <footer class="cal-pop-foot">
      <span></span>
      <div class="cal-pop-foot-right">
        <button type="button" class="cal-pop-text" onclick={onClose}>Cancel</button>
        <button
          type="button"
          class="cal-pop-save"
          disabled={saving}
          onclick={() => void submit()}
        >
          {saving ? "Saving…" : "Add"}
        </button>
      </div>
    </footer>
  </div>
</div>

<style>
  .cal-pop-backdrop {
    position: absolute;
    inset: 0;
    z-index: 40;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 1rem;
    background: rgb(var(--color-surface-950) / 0.28);
    backdrop-filter: blur(10px) saturate(1.15);
  }

  .cal-pop-backdrop-mobile {
    position: fixed;
    z-index: 50;
    align-items: flex-end;
    justify-content: stretch;
    padding: 0;
    padding-bottom: env(safe-area-inset-bottom, 0px);
    background: rgb(var(--color-surface-950) / 0.45);
  }

  .cal-pop {
    width: min(22.5rem, 100%);
    border-radius: 0.95rem;
    border: 1px solid rgb(255 255 255 / 0.08);
    background: color-mix(in srgb, rgb(var(--shell-pane-bg)) 78%, transparent);
    box-shadow:
      0 1px 0 rgb(255 255 255 / 0.06) inset,
      0 24px 64px rgb(0 0 0 / 0.45);
    backdrop-filter: blur(28px) saturate(1.35);
    padding: 0.55rem 0.7rem 0.7rem;
  }

  .cal-pop-mobile {
    width: 100%;
    border-radius: 1rem 1rem 0 0;
  }

  .cal-pop-grab {
    width: 2.25rem;
    height: 0.22rem;
    margin: 0.1rem auto 0.45rem;
    border-radius: 9999px;
    background: rgb(var(--shell-border));
    opacity: 0.7;
  }

  .cal-pop-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 0.55rem;
  }

  .cal-pop-mode {
    display: inline-flex;
    padding: 0.12rem;
    border-radius: 0.5rem;
    background: rgb(var(--color-surface-900) / 0.45);
  }

  .cal-pop-mode-active {
    display: inline-flex;
    min-height: 1.45rem;
    align-items: center;
    border-radius: 0.4rem;
    background: rgb(var(--color-secondary-500) / 0.88);
    padding: 0.15rem 0.65rem;
    font-size: 0.6875rem;
    font-weight: 650;
    color: rgb(var(--color-surface-50));
  }

  .cal-pop-x {
    display: inline-flex;
    height: 1.6rem;
    width: 1.6rem;
    align-items: center;
    justify-content: center;
    border-radius: 9999px;
    color: rgb(var(--shell-icon));
    background: rgb(var(--color-surface-800) / 0.55);
  }

  .cal-pop-card {
    border-radius: 0.7rem;
    border: 1px solid rgb(var(--shell-border) / 0.55);
    background: rgb(var(--color-surface-900) / 0.42);
    padding: 0.55rem 0.7rem;
    margin-bottom: 0.45rem;
  }

  .cal-pop-card-title {
    display: flex;
    align-items: center;
    gap: 0.55rem;
  }

  .cal-pop-title {
    flex: 1;
    min-width: 0;
    border: 0;
    background: transparent;
    font-size: 1.05rem;
    font-weight: 560;
    color: rgb(var(--color-surface-50));
    outline: none;
  }

  .cal-pop-row-icon {
    color: rgb(var(--shell-muted));
  }

  .cal-pop-field {
    display: block;
    width: 100%;
    margin-top: 0.35rem;
    min-height: 1.7rem;
    border: 0;
    border-radius: 0.4rem;
    background: rgb(var(--color-surface-800) / 0.55);
    padding: 0.2rem 0.45rem;
    font-size: 0.75rem;
    color: rgb(var(--color-surface-100));
  }

  .cal-pop-hint {
    margin: 0.4rem 0 0;
    font-size: 0.6875rem;
    color: rgb(var(--shell-muted));
  }

  .cal-pop-error {
    margin: 0 0.2rem 0.45rem;
    font-size: 0.75rem;
    color: rgb(var(--color-error-400));
  }

  .cal-pop-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    padding: 0.35rem 0.15rem 0.1rem;
  }

  .cal-pop-foot-right {
    display: flex;
    align-items: center;
    gap: 0.55rem;
  }

  .cal-pop-text {
    border: 0;
    background: transparent;
    font-size: 0.8125rem;
    font-weight: 550;
    color: rgb(var(--shell-label));
  }

  .cal-pop-save {
    min-height: 1.7rem;
    border: 0;
    border-radius: 0.5rem;
    background: rgb(var(--color-secondary-500));
    padding: 0.3rem 0.85rem;
    font-size: 0.8125rem;
    font-weight: 650;
    color: rgb(var(--color-surface-50));
  }

  .cal-pop-save:disabled {
    opacity: 0.55;
  }
</style>
