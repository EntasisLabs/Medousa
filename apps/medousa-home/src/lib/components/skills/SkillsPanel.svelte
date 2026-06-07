<script lang="ts">
  import { catalog } from "$lib/stores/catalog.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import type { ManuscriptCatalogEntry } from "$lib/types/catalog";

  interface Props {
    visible: boolean;
    onOpenChat: () => void;
    onScheduleSkill: (entry: ManuscriptCatalogEntry) => void;
  }

  let { visible, onOpenChat, onScheduleSkill }: Props = $props();

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
  <header class="workshop-header">
    <h1 class="text-base font-semibold text-surface-50">Skills &amp; Tools</h1>
    <p class="text-xs text-surface-300">
      Specialties and tools available in the workshop
    </p>
  </header>

  <div class="flex-1 overflow-y-auto px-5 py-4">
    {#if catalog.loading}
      <p class="workshop-muted">Loading catalog…</p>
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
        <h2 class="workshop-section-title">
          Skills · {catalog.manuscripts.length}
        </h2>
        <ul class="mt-3 space-y-3">
          {#each catalog.manuscripts as entry (entry.id)}
            <li class="workshop-inset p-4">
              <div class="flex items-start justify-between gap-4">
                <div class="min-w-0 flex-1">
                  <p class="font-medium text-surface-100">{entry.name}</p>
                  {#if entry.description}
                    <p class="mt-1 text-sm leading-relaxed text-surface-300">
                      {entry.description}
                    </p>
                  {/if}
                  {#if skillHint(entry)}
                    <p class="workshop-faint mt-2">{skillHint(entry)}</p>
                  {/if}
                </div>
                <div class="flex shrink-0 flex-col gap-2">
                  {#if entry.has_scripts}
                    <button
                      type="button"
                      class="btn btn-sm variant-filled-primary"
                      onclick={() => runSkill(entry.id)}
                    >
                      Run
                    </button>
                  {/if}
                  <button
                    type="button"
                    class="btn btn-sm variant-soft-primary"
                    onclick={() => onScheduleSkill(entry)}
                  >
                    Schedule…
                  </button>
                </div>
              </div>

              <details class="workshop-faint mt-3">
                <summary class="cursor-pointer select-none text-surface-300 hover:text-surface-100">
                  Technical details
                </summary>
                <dl class="workshop-faint mt-2 space-y-1 font-mono">
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
            <li class="workshop-inset p-4 text-sm text-surface-300">
              No skills yet. Import with
              <code class="text-surface-300">medousa skill-import</code>.
            </li>
          {/each}
        </ul>
      </section>

      <section class="mt-8">
        <h2 class="workshop-section-title">
          Tools · {catalog.capabilities.length}
        </h2>
        <ul class="mt-3 space-y-2">
          {#each catalog.capabilities as capability (capability.id)}
            <li class="workshop-inset px-4 py-3">
              <p class="text-sm font-medium text-surface-100">{capability.title}</p>
              <details class="mt-1 text-xs">
                <summary class="cursor-pointer select-none text-surface-300 hover:text-surface-100">
                  Registry entry
                </summary>
                <p class="workshop-faint mt-1 font-mono">
                  {capability.id} · {capability.binding_count} binding{capability.binding_count ===
                  1
                    ? ""
                    : "s"}
                </p>
              </details>
            </li>
          {:else}
            <li class="workshop-muted px-3 py-4">
              No tools registered yet.
            </li>
          {/each}
        </ul>
      </section>

    {/if}
  </div>
</section>
