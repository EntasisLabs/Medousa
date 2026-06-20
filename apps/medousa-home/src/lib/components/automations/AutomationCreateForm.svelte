<script lang="ts">
  import GrowingTextarea from "$lib/components/ui/GrowingTextarea.svelte";
  import { haptic } from "$lib/haptics";
  import type { AutomationDeliveryMode } from "$lib/types/recurring";

  interface Props {
    mobile?: boolean;
    title: string;
    prompt: string;
    cronExpr: string;
    timezone: string;
    displayName?: string;
    manuscript?: string;
    deliveryMode: AutomationDeliveryMode;
    telegramChatId: string;
    registering?: boolean;
    onCancel: () => void;
    onSubmit: () => void | Promise<void>;
  }

  let {
    mobile = false,
    title = $bindable(""),
    prompt = $bindable(""),
    cronExpr = $bindable(""),
    timezone = $bindable(""),
    displayName = $bindable(""),
    manuscript,
    deliveryMode = $bindable("in_app"),
    telegramChatId = $bindable(""),
    registering = false,
    onCancel,
    onSubmit,
  }: Props = $props();

  const barClass = $derived(mobile ? "composer-bar composer-bar-mobile" : "composer-bar");
</script>

<form
  class="cron-create-form {mobile ? 'cron-create-form-mobile' : ''}"
  onsubmit={(event) => {
    event.preventDefault();
    if (mobile) haptic("medium");
    void onSubmit();
  }}
>
  <label class="cron-field">
    <span class="cron-field-label">Title</span>
    <div class="{barClass} cron-field-bar cron-field-bar-compact">
      <input
        class="cron-field-input"
        bind:value={title}
        placeholder="Morning brief"
        autocapitalize="sentences"
        spellcheck="false"
        aria-label="Automation title"
      />
    </div>
  </label>

  <label class="cron-field">
    <span class="cron-field-label">What should run</span>
    <div class="{barClass} cron-field-bar">
      <GrowingTextarea
        bind:value={prompt}
        placeholder="Describe the scheduled work…"
        minHeight={mobile ? 34 : 36}
        maxHeight={mobile ? 160 : 128}
        aria-label="Automation prompt"
      />
    </div>
  </label>

  <div class="cron-field-row">
    <label class="cron-field cron-field-grow">
      <span class="cron-field-label">Schedule</span>
      <div class="{barClass} cron-field-bar cron-field-bar-compact">
        <input
          class="cron-field-input font-mono"
          bind:value={cronExpr}
          placeholder="0 9 * * *"
          autocapitalize="off"
          autocorrect="off"
          spellcheck="false"
          aria-label="Cron expression"
        />
      </div>
    </label>

    <label class="cron-field cron-field-timezone">
      <span class="cron-field-label">Timezone</span>
      <div class="{barClass} cron-field-bar cron-field-bar-compact">
        <input
          class="cron-field-input font-mono"
          bind:value={timezone}
          placeholder="UTC"
          autocapitalize="characters"
          autocorrect="off"
          spellcheck="false"
          aria-label="Timezone"
        />
      </div>
    </label>
  </div>

  <label class="cron-field">
    <span class="cron-field-label">Where results go</span>
    <div class="{barClass} cron-field-bar cron-field-bar-compact">
      <select class="cron-field-input" bind:value={deliveryMode} aria-label="Delivery destination">
        <option value="in_app">In Medousa (run history)</option>
        <option value="telegram">Telegram message</option>
        <option value="quiet">Run quietly</option>
      </select>
    </div>
  </label>

  {#if deliveryMode === "telegram"}
    <label class="cron-field">
      <span class="cron-field-label">Telegram chat id</span>
      <div class="{barClass} cron-field-bar cron-field-bar-compact">
        <input
          class="cron-field-input font-mono"
          bind:value={telegramChatId}
          placeholder="123456789"
          autocapitalize="off"
          autocorrect="off"
          spellcheck="false"
          aria-label="Telegram chat id"
        />
      </div>
    </label>
  {/if}

  {#if manuscript}
    <p class="cron-field-hint">
      Specialist · <span class="font-mono">{manuscript}</span>
    </p>
  {/if}

  <p class="cron-field-hint">
    Runs as a full agent turn with tools (not a single prompt-only reply).
  </p>

  <div class="cron-form-actions">
    <button
      type="submit"
      class="cron-form-submit btn btn-sm variant-filled-primary"
      disabled={registering || !prompt.trim()}
    >
      {registering ? "Saving…" : "Create automation"}
    </button>
    <button type="button" class="btn btn-sm variant-ghost-surface" onclick={onCancel}>
      Cancel
    </button>
  </div>
</form>
