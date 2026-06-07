<script lang="ts">
  import { catalog } from "$lib/stores/catalog.svelte";
  import { chat } from "$lib/stores/chat.svelte";

  interface Props {
    visible: boolean;
    onOpenChat: () => void;
  }

  let { visible, onOpenChat }: Props = $props();

  $effect(() => {
    if (visible) {
      void catalog.refresh();
    }
  });

  function runSkill(manuscriptId: string) {
    chat.draft = `/skill ${manuscriptId}`;
    onOpenChat();
  }
</script>

<section class="flex h-full min-w-0 flex-1 flex-col {visible ? '' : 'hidden'}">
  <header class="border-b border-surface-500/20 px-5 py-4">
    <h1 class="text-base font-semibold">Skills &amp; Tools</h1>
    <p class="text-xs text-surface-400">
      Specialties and tools available in the workshop
    </p>
  </header>

  <div class="flex-1 overflow-y-auto px-5 py-4">
    {#if catalog.loading}
      <p class="text-sm text-surface-400">Loading catalog…</p>
    {:else if catalog.error}
      <p class="text-sm text-error-400">{catalog.error}</p>
    {:else}
      <div class="mb-4 flex items-center gap-3">
        <label class="flex cursor-pointer items-center gap-2 text-sm text-surface-300">
          <input
            type="checkbox"
            class="checkbox"
            checked={catalog.skillsOnly}
            onchange={(event) => {
              catalog.skillsOnly = (event.currentTarget as HTMLInputElement).checked;
              void catalog.refresh();
            }}
          />
          Skills with runnable scripts only
        </label>
        <button
          type="button"
          class="btn btn-sm variant-ghost-surface"
          onclick={() => catalog.refresh()}
        >
          Refresh
        </button>
      </div>

      <section>
        <h2 class="text-xs font-semibold uppercase tracking-wide text-surface-400">
          Skills ({catalog.manuscripts.length})
        </h2>
        <ul class="mt-2 space-y-2">
          {#each catalog.manuscripts as entry (entry.id)}
            <li
              class="rounded-container-token border border-surface-500/20 bg-surface-900/50 p-3"
            >
              <div class="flex items-start justify-between gap-3">
                <div class="min-w-0">
                  <p class="font-medium text-surface-100">{entry.name}</p>
                  <p class="text-xs text-surface-500">
                    {entry.id} · {entry.scope}
                    {#if entry.openshell_enabled}
                      · openshell
                    {/if}
                  </p>
                  {#if entry.description}
                    <p class="mt-1 text-sm text-surface-300">{entry.description}</p>
                  {/if}
                  {#if entry.scripts.length > 0}
                    <p class="mt-2 text-xs text-surface-400">
                      Scripts:
                      {entry.scripts
                        .map((s) => `${s.relative_path} (${s.risk_class})`)
                        .join(", ")}
                    </p>
                  {/if}
                </div>
                {#if entry.has_scripts}
                  <button
                    type="button"
                    class="btn btn-sm variant-soft-primary shrink-0"
                    onclick={() => runSkill(entry.id)}
                  >
                    Run
                  </button>
                {/if}
              </div>
            </li>
          {:else}
            <li class="rounded-container-token bg-surface-900/40 p-4 text-sm text-surface-400">
              No skills found. Import with
              <code class="text-surface-300">medousa skill-import</code>.
            </li>
          {/each}
        </ul>
      </section>

      <section class="mt-8">
        <h2 class="text-xs font-semibold uppercase tracking-wide text-surface-400">
          Capabilities ({catalog.capabilities.length})
        </h2>
        <ul class="mt-2 space-y-1">
          {#each catalog.capabilities as capability (capability.id)}
            <li
              class="flex items-center justify-between rounded-container-token bg-surface-900/40 px-3 py-2 text-sm"
            >
              <span class="font-medium text-surface-200">{capability.title}</span>
              <span class="text-xs text-surface-500">
                {capability.id} · {capability.binding_count} binding{capability.binding_count ===
                1
                  ? ""
                  : "s"}
              </span>
            </li>
          {:else}
            <li class="px-3 py-4 text-sm text-surface-400">
              No capabilities registered.
            </li>
          {/each}
        </ul>
      </section>
    {/if}
  </div>
</section>
