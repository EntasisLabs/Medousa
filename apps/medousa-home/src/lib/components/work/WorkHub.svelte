<script lang="ts">
  import WorkHubStage from "$lib/components/work/WorkHubStage.svelte";
  import WorkHubTrays from "$lib/components/work/WorkHubTrays.svelte";

  interface Props {
    onSelectCard: (id: string) => void | Promise<void>;
    onOpenNote: (path: string) => void;
    onOpenChat: () => void;
  }

  let { onSelectCard, onOpenNote, onOpenChat }: Props = $props();
</script>

<div class="work-hub flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
  <WorkHubStage {onSelectCard} {onOpenNote} {onOpenChat} />
  <WorkHubTrays {onSelectCard} />
</div>
