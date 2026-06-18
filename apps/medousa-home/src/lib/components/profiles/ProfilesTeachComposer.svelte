<script lang="ts">
  import { ArrowUp } from "@lucide/svelte";
  import { rememberIdentityFact } from "$lib/daemon";
  import type { IdentityRememberResponse } from "$lib/types/identity";
  import type { IdentityRememberRequest } from "$lib/types/identity";
  import { parseIdentityTeachInput, withIdentityUserId } from "$lib/utils/identityTeach";

  interface Props {
    readOnly?: boolean;
    onRemembered?: (
      parsed: IdentityRememberRequest,
      result: IdentityRememberResponse,
    ) => void | Promise<void>;
  }

  let { readOnly = false, onRemembered }: Props = $props();

  let text = $state("");
  let busy = $state(false);
  let flash = $state<string | null>(null);
  let flashOk = $state(true);

  async function submit() {
    const parsed = withIdentityUserId(parseIdentityTeachInput(text));
    if (!parsed.statement.trim() || readOnly) return;
    busy = true;
    flash = null;
    try {
      const result = await rememberIdentityFact(parsed);
      flashOk = result.committed || !result.requires_confirmation;
      if (result.committed) {
        text = "";
        flash = "She'll remember that.";
      } else {
        flash = result.message;
      }
      await onRemembered?.(parsed, result);
    } catch (err) {
      flashOk = false;
      flash = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      void submit();
    }
  }
</script>

<div class="profiles-teach-dock">
  {#if flash}
    <p class="mb-2 text-xs {flashOk ? 'text-success-400' : 'text-warning-400'}" role="status">
      {flash}
    </p>
  {/if}
  <div class="composer-bar max-w-3xl">
    <textarea
      class="composer-bar-input min-h-[2.5rem] w-full flex-1 resize-none bg-transparent py-1 text-sm leading-relaxed text-surface-100 placeholder:text-surface-500 focus:outline-none"
      rows="1"
      placeholder="Tell her something she should remember…"
      bind:value={text}
      disabled={readOnly || busy}
      onkeydown={onKeydown}
    ></textarea>
    <button
      type="button"
      class="composer-bar-send shrink-0"
      disabled={readOnly || busy || !text.trim()}
      aria-label="Remember"
      onclick={() => void submit()}
    >
      <ArrowUp size={16} strokeWidth={2.5} aria-hidden="true" />
    </button>
  </div>
  <p class="workshop-faint mt-2 text-[11px]">
    Plain language works — “Mario is my partner”, “I prefer matcha”, your timezone.
  </p>
</div>
