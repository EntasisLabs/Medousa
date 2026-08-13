<script lang="ts">
  import MobileCodeProjectHeader from "$lib/components/mobile/code/MobileCodeProjectHeader.svelte";
  import MobileCodeSurfaceSwitcher from "$lib/components/mobile/code/MobileCodeSurfaceSwitcher.svelte";
  import MobileProjectFiles from "$lib/components/mobile/code/MobileProjectFiles.svelte";
  import MobileCodeEditor from "$lib/components/mobile/code/MobileCodeEditor.svelte";
  import MobileProjectTerminal from "$lib/components/mobile/code/MobileProjectTerminal.svelte";
  import MobileProjectChanges from "$lib/components/mobile/code/MobileProjectChanges.svelte";
  import { mobileCodeWorkspaceState } from "$lib/stores/mobileCodeWorkspaceState.svelte";

  interface Props {
    workId: string;
  }

  let { workId }: Props = $props();

  const surface = $derived(mobileCodeWorkspaceState.surface);
</script>

<section class="flex h-full min-h-0 flex-col bg-surface-950" aria-label="Code workspace">
  {#if surface === "files" || surface === "changes"}
    <MobileCodeProjectHeader />
  {/if}
  <div class="min-h-0 flex-1 overflow-hidden">
    {#if surface === "editor"}
      <MobileCodeEditor {workId} />
    {:else if surface === "terminal"}
      <MobileProjectTerminal {workId} />
    {:else if surface === "changes"}
      <MobileProjectChanges {workId} />
    {:else}
      <MobileProjectFiles {workId} />
    {/if}
  </div>
  {#if surface !== "terminal"}
    <MobileCodeSurfaceSwitcher />
  {/if}
</section>
