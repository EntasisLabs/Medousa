<script lang="ts">
  import { Check, ListTodo, X } from "@lucide/svelte";
  import { registerMobileBackHandler } from "$lib/mobileNavigation";
  import { calendar, calendarDateUtils } from "$lib/stores/calendar.svelte";
  import { attachMobileSheetGestures } from "$lib/utils/mobileSheetGestures";

  interface Props {
    mobile?: boolean;
    onClose: () => void;
    onSwitchKind?: (kind: "event" | "reminder") => void;
  }

  let { mobile = false, onClose, onSwitchKind }: Props = $props();

  const { isoDay } = calendarDateUtils;

  let title = $state("");
  let dueDay = $state(isoDay(calendar.selectedDay));
  let saving = $state(false);
  let error = $state<string | null>(null);
  let sheetEl = $state<HTMLDivElement | null>(null);
  let headerEl = $state<HTMLElement | null>(null);

  $effect(() => {
    dueDay = isoDay(calendar.selectedDay);
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

  $effect(() => {
    if (!mobile || !sheetEl || !headerEl) return;
    return attachMobileSheetGestures(sheetEl, headerEl, {
      onDismiss: onClose,
    });
  });
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="cal-pop-backdrop"
  class:cal-pop-backdrop-mobile={mobile}
  class:mobile-sheet-backdrop={mobile}
  role="presentation"
  onclick={onClose}
  onkeydown={onKeydown}
>
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    bind:this={sheetEl}
    class="cal-pop"
    class:cal-pop-mobile={mobile}
    class:mobile-sheet={mobile}
    class:calendar-editor-sheet={mobile}
    role="dialog"
    tabindex="-1"
    aria-modal="true"
    aria-label="New reminder"
    onclick={(e) => e.stopPropagation()}
    onkeydown={onKeydown}
  >
    {#if mobile}
      <div bind:this={headerEl} class="cal-pop-mobile-sheet-head">
        <div class="mobile-turn-sheet-grabber" aria-hidden="true"></div>
        <header class="cal-pop-mobile-head">
          <button type="button" class="cal-pop-x" aria-label="Close" onclick={onClose}>
            <X size={21} strokeWidth={1.75} />
          </button>
          <h2>New</h2>
          <button
            type="button"
            class="cal-pop-mobile-save"
            aria-label="Add reminder"
            disabled={saving}
            onclick={() => void submit()}
          >
            <Check size={22} strokeWidth={2} />
          </button>
        </header>
        <div class="cal-pop-kind" role="tablist" aria-label="Create type">
          <button
            type="button"
            role="tab"
            aria-selected="false"
            onclick={() => onSwitchKind?.("event")}
          >
            Event
          </button>
          <button type="button" role="tab" aria-selected="true" class="cal-pop-kind-active">
            Reminder
          </button>
        </div>
      </div>
    {:else}
      <div class="cal-pop-grab" aria-hidden="true"></div>
      <header class="cal-pop-head">
        <div class="cal-pop-mode">
          <span class="cal-pop-mode-active">New Reminder</span>
        </div>
        <button type="button" class="cal-pop-x" aria-label="Close" onclick={onClose}>
          <X size={15} strokeWidth={1.75} />
        </button>
      </header>
    {/if}

    <div class="cal-pop-scroll">
      <div class="cal-pop-card cal-pop-card-title">
        {#if !mobile}
          <ListTodo size={16} strokeWidth={1.75} class="cal-pop-row-icon" />
        {/if}
        <input
          class="cal-pop-title"
          bind:value={title}
          placeholder="Reminder"
          maxlength={200}
        />
      </div>

      <div class="cal-pop-card">
        {#if mobile}
          <label class="cal-pop-mobile-field-row" for="cal-reminder-due">
            <span>Due</span>
            <input
              id="cal-reminder-due"
              class="cal-pop-field"
              type="date"
              bind:value={dueDay}
            />
          </label>
        {:else}
          <label class="cal-pop-hint" for="cal-reminder-due">Due</label>
          <input
            id="cal-reminder-due"
            class="cal-pop-field"
            type="date"
            bind:value={dueDay}
          />
        {/if}
        <p class="cal-pop-hint">Stored as a vault checkbox in calendar/reminders.md</p>
      </div>

      {#if error}
        <p class="cal-pop-error">{error}</p>
      {/if}
    </div>

    {#if !mobile}
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
    {/if}
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
    inset: 0;
    bottom: auto;
    z-index: 70;
    height: calc(
      var(--mobile-layout-height, 100dvh) - var(--mobile-keyboard-inset, 0px)
    );
    align-items: flex-end;
    justify-content: center;
    padding: 0;
    background: rgb(var(--color-surface-950) / 0.7);
    backdrop-filter: none;
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
    display: flex;
    height: min(52dvh, 28rem);
    width: 100%;
    max-height: calc(
      var(--mobile-layout-height, 100dvh) - var(--mobile-keyboard-inset, 0px) -
        max(1rem, env(safe-area-inset-top, 0px))
    );
    flex-direction: column;
    overflow: hidden;
    border: 1px solid rgb(var(--shell-border) / 0.62);
    border-bottom: 0;
    border-radius: 1.5rem 1.5rem 0 0;
    background: rgb(var(--shell-pane-bg));
    padding: 0;
    box-shadow: 0 -18px 52px rgb(var(--theme-shadow) / 0.32);
    backdrop-filter: none;
  }

  .cal-pop-mobile-sheet-head {
    flex-shrink: 0;
  }

  .cal-pop-scroll {
    display: contents;
  }

  .cal-pop-mobile .cal-pop-scroll {
    display: block;
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
    padding: 0.75rem 1rem calc(2rem + env(safe-area-inset-bottom, 0px));
    -webkit-overflow-scrolling: touch;
  }

  .cal-pop-mobile-head {
    display: grid;
    flex-shrink: 0;
    grid-template-columns: 2.75rem minmax(0, 1fr) 2.75rem;
    align-items: center;
    gap: 0.75rem;
    padding: 0.1rem 1rem 0.45rem;
  }

  .cal-pop-mobile-head h2 {
    margin: 0;
    text-align: center;
    font-size: 1.0625rem;
    font-weight: 650;
    letter-spacing: -0.018em;
    color: rgb(var(--theme-text));
  }

  .cal-pop-mobile .cal-pop-x,
  .cal-pop-mobile-save {
    display: inline-flex;
    width: 2.75rem;
    height: 2.75rem;
    align-items: center;
    justify-content: center;
    border: 1px solid rgb(var(--shell-border) / 0.72);
    border-radius: 9999px;
    background: rgb(var(--color-surface-800) / 0.62);
    color: rgb(var(--theme-text));
  }

  .cal-pop-mobile-save {
    background: rgb(var(--color-secondary-500));
    color: rgb(var(--color-surface-50));
  }

  .cal-pop-mobile-save:disabled {
    opacity: 0.45;
  }

  .cal-pop-kind {
    display: grid;
    flex-shrink: 0;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.15rem;
    margin: 0.15rem 1rem 0.55rem;
    border-radius: 0.8rem;
    background: rgb(var(--color-surface-800) / 0.72);
    padding: 0.16rem;
  }

  .cal-pop-kind button {
    min-height: 2.6rem;
    border-radius: 0.65rem;
    font-size: 0.875rem;
    font-weight: 600;
    color: rgb(var(--theme-text-secondary));
  }

  .cal-pop-kind button.cal-pop-kind-active {
    background: rgb(var(--color-surface-600) / 0.82);
    color: rgb(var(--theme-text));
    box-shadow: 0 1px 2px rgb(var(--theme-shadow) / 0.18);
  }

  .cal-pop-mobile .cal-pop-card {
    margin-bottom: 0.75rem;
    border: 0;
    border-radius: 1rem;
    background: rgb(var(--color-surface-800) / 0.58);
    padding: 0.8rem 0.9rem;
  }

  .cal-pop-mobile .cal-pop-card-title {
    min-height: 4.25rem;
    padding: 0.55rem 0.9rem;
  }

  .cal-pop-mobile .cal-pop-title {
    padding: 0.65rem 0.15rem !important;
    font-size: 1.1875rem;
    line-height: 1.35;
  }

  .cal-pop-mobile-field-row {
    display: flex;
    min-height: 3.35rem;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    font-size: 0.9375rem;
    color: rgb(var(--theme-text));
  }

  .cal-pop-mobile .cal-pop-field {
    width: auto;
    min-width: 9.5rem;
    min-height: 2.5rem;
    margin-top: 0;
    border-radius: 9999px;
    background: rgb(var(--color-surface-700) / 0.72);
    padding: 0.35rem 0.7rem;
    font-size: 1rem;
    text-align: right;
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

  :global(.cal-pop-row-icon) {
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
    color: rgb(var(--theme-error));
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
