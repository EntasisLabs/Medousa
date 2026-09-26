<script lang="ts">
  interface Props {
    value?: string;
    configured?: boolean;
    clearRequested?: boolean;
    busy?: boolean;
  }

  let {
    value = $bindable(""),
    configured = $bindable(false),
    clearRequested = $bindable(false),
    busy = false,
  }: Props = $props();

  function removeSavedToken() {
    value = "";
    configured = false;
    clearRequested = true;
  }
</script>

<label class="block">
  <span class="workshop-label">Bearer token (optional)</span>
  <input
    class="input mt-1 w-full font-mono text-sm"
    type="password"
    bind:value
    disabled={busy}
    placeholder={configured ? "Saved securely" : "sk-…"}
    autocomplete="off"
  />
  {#if configured}
    <span class="mt-1 flex items-center justify-between gap-2 text-xs">
      <span class="workshop-faint">A token is saved in Medousa’s secure store.</span>
      <button
        type="button"
        class="text-error-300 hover:text-error-200"
        disabled={busy}
        onclick={removeSavedToken}
      >
        Remove
      </button>
    </span>
  {/if}
</label>
