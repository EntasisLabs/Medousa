<script lang="ts">
  import GrowingTextarea from "$lib/components/ui/GrowingTextarea.svelte";
  import { haptic } from "$lib/haptics";

  interface Props {
    mobile?: boolean;
    prompt: string;
    cronExpr: string;
    timezone: string;
    manuscript?: string;
    registering?: boolean;
    onCancel: () => void;
    onSubmit: () => void | Promise<void>;
  }

  let {
    mobile = false,
    prompt = $bindable(""),
    cronExpr = $bindable(""),
    timezone = $bindable(""),
    manuscript,
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
    <span class="cron-field-label">Prompt</span>
    <div class="{barClass} cron-field-bar">
      <GrowingTextarea
        bind:value={prompt}
        placeholder="What should run on schedule?"
        minHeight={mobile ? 34 : 36}
        maxHeight={mobile ? 160 : 128}
        aria-label="Scheduled prompt"
      />
    </div>
  </label>

  <div class="cron-field-row">
    <label class="cron-field cron-field-grow">
      <span class="cron-field-label">Cron expression</span>
      <div class="{barClass} cron-field-bar cron-field-bar-compact">
        <input
          class="cron-field-input font-mono"
          bind:value={cronExpr}
          placeholder="0 9 * * *"
          autocapitalize="off"
          autocorrect="off"
          spellcheck="false"
          inputmode="text"
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

  {#if manuscript}
    <p class="cron-field-hint">
      Skill manuscript · <span class="font-mono">{manuscript}</span>
    </p>
  {/if}

  <div class="cron-form-actions">
    <button
      type="submit"
      class="cron-form-submit btn btn-sm variant-filled-primary"
      disabled={registering || !prompt.trim()}
    >
      {registering ? "Saving…" : "Create schedule"}
    </button>
    <button type="button" class="btn btn-sm variant-ghost-surface" onclick={onCancel}>
      Cancel
    </button>
  </div>
</form>
