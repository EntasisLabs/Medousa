<script lang="ts">
  import "$lib/styles/lme.postcss";
  import "$lib/styles/vault-browse.postcss";
  import "$lib/styles/vault-workshop.postcss";
  import LmeCodeExplorer from "$lib/components/lme/explorers/LmeCodeExplorer.svelte";
  import MobileCodeWorkspace from "$lib/components/mobile/code/MobileCodeWorkspace.svelte";
  import { Code2 } from "@lucide/svelte";
  import { registerMobileBackHandler } from "$lib/mobileNavigation";
  import { mobileCodeWorkspaceState } from "$lib/stores/mobileCodeWorkspaceState.svelte";
  import { undertakings } from "$lib/stores/undertakings.svelte";
  import { enterMobileCodeProject } from "$lib/utils/mobileCodeOpen";

  const workId = $derived(mobileCodeWorkspaceState.selectedWorkId);

  $effect(() => {
    if (!workId) return;
    return registerMobileBackHandler(() => mobileCodeWorkspaceState.handleBack());
  });
</script>

{#if workId && undertakings.detail?.id === workId}
  <MobileCodeWorkspace {workId} />
{:else if workId}
  <section class="flex h-full min-h-0 flex-col bg-surface-950">
    <p class="px-4 py-6 text-sm text-content-quiet">Opening project…</p>
  </section>
{:else}
  <section class="flex h-full min-h-0 flex-col bg-surface-950" aria-label="Code projects">
    <header class="flex shrink-0 items-center gap-2 border-b border-surface-500/30 px-4 py-3">
      <Code2 size={17} class="text-content-link" />
      <div>
        <h1 class="text-base font-semibold text-surface-50">Code</h1>
        <p class="text-content-quiet text-[10px]">Make changes without losing your place</p>
      </div>
    </header>
    <div class="min-h-0 flex-1 overflow-hidden">
      <LmeCodeExplorer
        onOpenProject={async (id) => {
          await enterMobileCodeProject(id);
        }}
      />
    </div>
  </section>
{/if}
