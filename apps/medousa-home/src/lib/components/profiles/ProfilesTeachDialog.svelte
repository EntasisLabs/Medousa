<script lang="ts">
  import { rememberIdentityFact } from "$lib/daemon";
  import type { IdentityRememberResponse } from "$lib/types/identity";
  import type { IdentityRememberRequest } from "$lib/types/identity";
  import { parseIdentityTeachInput, withIdentityUserId } from "$lib/utils/identityTeach";
  import { X } from "@lucide/svelte";
  import { tick } from "svelte";

  interface Props {
    open: boolean;
    readOnly?: boolean;
    prefill?: string;
    onClose: () => void;
    onRemembered?: (
      parsed: IdentityRememberRequest,
      result: IdentityRememberResponse,
    ) => void | Promise<void>;
  }

  let {
    open,
    readOnly = false,
    prefill = "",
    onClose,
    onRemembered,
  }: Props = $props();

  let text = $state("");
  let busy = $state(false);
  let flash = $state<string | null>(null);
  let flashOk = $state(true);
  let inputEl: HTMLTextAreaElement | undefined = $state();
  let wasOpen = $state(false);

  $effect(() => {
    if (open && !wasOpen) {
      text = prefill;
      flash = null;
      flashOk = true;
      void tick().then(() => {
        inputEl?.focus();
        if (prefill) inputEl?.select();
      });
    }
    wasOpen = open;
  });

  async function submit(event?: SubmitEvent) {
    event?.preventDefault();
    const parsed = withIdentityUserId(parseIdentityTeachInput(text));
    if (!parsed.statement.trim() || readOnly) return;
    busy = true;
    flash = null;
    try {
      const result = await rememberIdentityFact(parsed);
      flashOk = result.committed || !result.requires_confirmation;
      if (result.committed) {
        text = "";
        flash = "Saved to your profile.";
        await onRemembered?.(parsed, result);
        onClose();
      } else {
        flash = result.message;
        await onRemembered?.(parsed, result);
      }
    } catch (err) {
      flashOk = false;
      flash = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
      return;
    }
    if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      void submit();
    }
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="vault-interact-backdrop"
    role="dialog"
    aria-modal="true"
    aria-labelledby="profiles-teach-title"
    tabindex="-1"
    onkeydown={onKeydown}
    onclick={(event) => {
      if (event.target === event.currentTarget) onClose();
    }}
  >
    <form class="vault-interact-sheet vault-compose-sheet profiles-create-sheet" onsubmit={submit}>
      <header class="vault-interact-header vault-compose-header">
        <div class="min-w-0">
          <p class="vault-interact-kicker">You</p>
          <h3 id="profiles-teach-title" class="vault-interact-title">Teach Medousa</h3>
        </div>
        <button
          type="button"
          class="vault-interact-dismiss"
          aria-label="Close"
          onclick={() => onClose()}
        >
          <X size={14} strokeWidth={2} />
        </button>
      </header>

      <textarea
        bind:this={inputEl}
        class="profiles-create-body"
        rows="4"
        placeholder="A fact about you, a preference, or how someone fits in…"
        bind:value={text}
        disabled={readOnly || busy}
      ></textarea>

      <p class="vault-interact-note">
        Timezone, answer style, people’s names and roles — say it in plain language.
      </p>

      {#if flash}
        <p class="text-xs {flashOk ? 'text-success-400' : 'text-warning-400'}" role="status">
          {flash}
        </p>
      {/if}

      <div class="vault-compose-footer">
        <button
          type="submit"
          class="vault-interact-commit"
          disabled={readOnly || busy || !text.trim()}
        >
          {busy ? "Saving…" : "Remember"}
        </button>
      </div>
    </form>
  </div>
{/if}
