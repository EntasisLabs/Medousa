<script lang="ts">
  import { catalog } from "$lib/stores/catalog.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import type { ManuscriptCatalogEntry } from "$lib/types/catalog";

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

  function skillHint(entry: ManuscriptCatalogEntry): string | null {
    if (entry.openshell_enabled) return "Runs in a sandbox";
    if (entry.has_scripts) return "Runnable skill";
    return null;
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
          Runnable skills only
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
        <h2 class="text-xs font-medium text-surface-500">
          Skills · {catalog.manuscripts.length}
        </h2>
        <ul class="mt-3 space-y-3">
          {#each catalog.manuscripts as entry (entry.id)}
            <li class="rounded-container-token border border-surface-500/20 bg-surface-900/50 p-4">
              <div class="flex items-start justify-between gap-4">
                <div class="min-w-0 flex-1">
                  <p class="font-medium text-surface-100">{entry.name}</p>
                  {#if entry.description}
                    <p class="mt-1 text-sm leading-relaxed text-surface-300">
                      {entry.description}
                    </p>
                  {/if}
                  {#if skillHint(entry)}
                    <p class="mt-2 text-xs text-surface-500">{skillHint(entry)}</p>
                  {/if}
                </div>
                {#if entry.has_scripts}
                  <button
                    type="button"
                    class="btn btn-sm variant-filled-primary shrink-0"
                    onclick={() => runSkill(entry.id)}
                  >
                    Run
                  </button>
                {/if}
              </div>

              <details class="mt-3 text-xs text-surface-500">
                <summary class="cursor-pointer select-none text-surface-400 hover:text-surface-300">
                  Technical details
                </summary>
                <dl class="mt-2 space-y-1 font-mono text-[11px] text-surface-500">
                  <div>id: {entry.id}</div>
                  <div>scope: {entry.scope}</div>
                  {#if entry.openshell_enabled}
                    <div>sandbox: openshell</div>
                  {/if}
                  {#if entry.scripts.length > 0}
                    <div>
                      scripts:
                      {entry.scripts
                        .map((script) => `${script.relative_path} (${script.risk_class})`)
                        .join(", ")}
                    </div>
                  {/if}
                </dl>
              </details>
            </li>
          {:else}
            <li class="rounded-container-token bg-surface-900/40 p-4 text-sm text-surface-400">
              No skills yet. Import with
              <code class="text-surface-300">medousa skill-import</code>.
            </li>
          {/each}
        </ul>
      </section>

      <section class="mt-8">
        <h2 class="text-xs font-medium text-surface-500">
          Tools · {catalog.capabilities.length}
        </h2>
        <ul class="mt-3 space-y-2">
          {#each catalog.capabilities as capability (capability.id)}
            <li class="rounded-container-token border border-surface-500/15 bg-surface-900/40 px-4 py-3">
              <p class="text-sm font-medium text-surface-100">{capability.title}</p>
              <details class="mt-1 text-xs">
                <summary class="cursor-pointer select-none text-surface-500 hover:text-surface-400">
                  Registry entry
                </summary>
                <p class="mt-1 font-mono text-[11px] text-surface-500">
                  {capability.id} · {capability.binding_count} binding{capability.binding_count ===
                  1
                    ? ""
                    : "s"}
                </p>
              </details>
            </li>
          {:else}
            <li class="px-3 py-4 text-sm text-surface-400">
              No tools registered yet.
            </li>
          {/each}
        </ul>
      </section>
    {/if}
  </div>
</section>
