<script lang="ts">
  /**
   * Chart data body — no second dialog. Hosted inside Configure as a view swap.
   */
  import ChartPipeTableEditor from "$lib/components/vault/ChartPipeTableEditor.svelte";
  import type { ChartFenceType } from "$lib/utils/liquidFenceTemplates";

  interface Props {
    tableMarkdown: string;
    chartType: ChartFenceType;
    onChange: (tableMarkdown: string) => void;
  }

  let { tableMarkdown, chartType, onChange }: Props = $props();
</script>

<div class="vault-chart-data-body">
  <ChartPipeTableEditor
    content={tableMarkdown}
    {chartType}
    onchange={onChange}
  />
</div>
