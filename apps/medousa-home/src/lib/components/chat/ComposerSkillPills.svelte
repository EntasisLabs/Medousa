<script lang="ts">
  import { X } from "@lucide/svelte";
  import { catalog } from "$lib/stores/catalog.svelte";
  import {
    composerAttachments,
    type ComposerAttachmentHost,
  } from "$lib/stores/composerAttachments.svelte";

  interface Props {
    host: ComposerAttachmentHost;
    disabled?: boolean;
    class?: string;
  }

  let { host, disabled = false, class: className = "" }: Props = $props();

  const attachments = $derived(composerAttachments.forHost(host));

  const skills = $derived(
    attachments.skillIds
      .map((id) => catalog.manuscripts.find((entry) => entry.id === id))
      .filter((entry): entry is NonNullable<typeof entry> => Boolean(entry)),
  );
  const tools = $derived(
    attachments.toolIds
      .map((id) => catalog.capabilities.find((entry) => entry.id === id))
      .filter((entry): entry is NonNullable<typeof entry> => Boolean(entry)),
  );

  const visible = $derived(skills.length > 0 || tools.length > 0);
</script>

{#if visible}
  <div class="composer-skill-pills {className}" role="list" aria-label="Attached skills and tools">
    {#each skills as entry (entry.id)}
      <span class="composer-skill-pill composer-skill-pill-skill" role="listitem">
        <span class="truncate" title={entry.description ?? entry.id}>{entry.name}</span>
        <button
          type="button"
          class="composer-skill-pill-remove"
          aria-label={`Remove skill ${entry.name}`}
          {disabled}
          onclick={() => attachments.detachSkill(entry.id)}
        >
          <X size={12} strokeWidth={2} />
        </button>
      </span>
    {/each}
    {#each tools as entry (entry.id)}
      <span class="composer-skill-pill composer-skill-pill-tool" role="listitem">
        <span class="truncate" title={entry.description ?? entry.id}>{entry.title}</span>
        <button
          type="button"
          class="composer-skill-pill-remove"
          aria-label={`Remove tool ${entry.title}`}
          {disabled}
          onclick={() => attachments.detachTool(entry.id)}
        >
          <X size={12} strokeWidth={2} />
        </button>
      </span>
    {/each}
  </div>
{/if}
