<script lang="ts">
  import GrowingTextarea from "$lib/components/ui/GrowingTextarea.svelte";
  import FriendlySchedulePicker from "$lib/components/automations/FriendlySchedulePicker.svelte";
  import { haptic } from "$lib/haptics";
  import { catalog } from "$lib/stores/catalog.svelte";
  import type { AutomationDeliveryMode } from "$lib/types/recurring";
  import "$lib/components/skills/agentEditor.css";

  interface Props {
    mobile?: boolean;
    /** Liquid popover layout — no boxed 2016 form. */
    compact?: boolean;
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
    /** Re-place host popover when create layout grows/shrinks. */
    onLayoutChange?: () => void;
  }

  let {
    mobile = false,
    compact = false,
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
    onLayoutChange,
  }: Props = $props();

  const barClass = $derived(mobile ? "composer-bar composer-bar-mobile" : "composer-bar");

  const manuscriptLabel = $derived.by(() => {
    const id = manuscript?.trim();
    if (!id) return null;
    const name = catalog.manuscripts.find((entry) => entry.id === id)?.name;
    return name ? { name, id } : { name: id, id };
  });

  $effect(() => {
    if (manuscript?.trim() && catalog.manuscripts.length === 0 && !catalog.loading) {
      void catalog.refresh();
    }
  });

  function handleSubmit(event: Event) {
    event.preventDefault();
    if (mobile) haptic("medium");
    void onSubmit();
  }
</script>

{#if compact}
  <form class="agent-liquid schedule-create-liquid" onsubmit={handleSubmit}>
    <label class="block">
      <span class="sr-only">Title</span>
      <input
        class="agent-liquid-name"
        bind:value={title}
        placeholder="Morning brief"
        autocapitalize="sentences"
        spellcheck="false"
        aria-label="Schedule title"
      />
    </label>

    <label class="mt-2 block">
      <span class="sr-only">What runs</span>
      <textarea
        class="agent-liquid-textarea"
        bind:value={prompt}
        placeholder="Describe the scheduled work…"
        rows="2"
        aria-label="Automation prompt"
      ></textarea>
    </label>

    <div class="mt-3">
      <FriendlySchedulePicker
        {mobile}
        liquid
        bind:cronExpr
        bind:timezone
        {onLayoutChange}
      />
    </div>

    <div class="schedule-deliver mt-3">
      <span class="schedule-deliver-label">Deliver to</span>
      <select
        class="schedule-deliver-select"
        bind:value={deliveryMode}
        aria-label="Delivery destination"
      >
        <option value="in_app">Stay in Medousa</option>
        <option value="telegram">Telegram</option>
        <option value="quiet">Quietly</option>
      </select>
    </div>

    {#if deliveryMode === "telegram"}
      <label class="mt-1.5 block">
        <span class="sr-only">Telegram chat</span>
        <input
          class="agent-liquid-input font-mono"
          bind:value={telegramChatId}
          placeholder="Chat id"
          autocapitalize="off"
          autocorrect="off"
          spellcheck="false"
          aria-label="Telegram chat id"
        />
      </label>
    {/if}

    {#if manuscriptLabel}
      <p class="mt-2 text-[11px] text-surface-500">
        Then <span class="text-surface-400">{manuscriptLabel.name}</span>
      </p>
    {/if}

    <div class="schedule-create-foot flex items-center gap-4">
      <button
        type="submit"
        class="schedule-create-act schedule-create-act-primary"
        disabled={registering || !prompt.trim()}
      >
        {registering ? "Saving…" : "Create"}
      </button>
      <button type="button" class="schedule-create-act" onclick={onCancel}>Cancel</button>
    </div>
  </form>
{:else}
  <form
    class="cron-create-form {mobile ? 'cron-create-form-mobile' : ''}"
    onsubmit={handleSubmit}
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

    <FriendlySchedulePicker {mobile} bind:cronExpr bind:timezone label="When should it run?" />

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

    {#if manuscriptLabel}
      <p class="cron-field-hint">
        Then: <span class="font-medium text-surface-200">{manuscriptLabel.name}</span>
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
{/if}
