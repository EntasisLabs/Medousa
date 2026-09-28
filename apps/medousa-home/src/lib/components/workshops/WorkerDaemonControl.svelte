<script lang="ts">
  import { Cpu } from "@lucide/svelte";
  import WorkerDaemonJoinSheet from "./WorkerDaemonJoinSheet.svelte";
  let open = $state(false);
</script>

<button
  type="button"
  class="ws-icon-btn"
  title="Add worker daemon"
  aria-label="Add worker daemon"
  onclick={() => (open = true)}
>
  <Cpu size={15} strokeWidth={1.75} />
</button>
<WorkerDaemonJoinSheet {open} onClose={() => (open = false)} />

<style>
  .ws-icon-btn {
    display: inline-flex;
    height: 1.85rem;
    width: 1.85rem;
    align-items: center;
    justify-content: center;
    border: 1px solid rgb(var(--color-surface-500) / 0.32);
    border-radius: 0.5rem;
    background: rgb(var(--color-surface-900) / 0.28);
    color: rgb(var(--theme-text-secondary));
    cursor: pointer;
  }
  .ws-icon-btn:hover {
    border-color: rgb(var(--color-surface-500) / 0.5);
    background: rgb(var(--color-surface-800) / 0.35);
    color: rgb(var(--color-surface-100));
  }
</style>
