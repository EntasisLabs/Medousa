<script lang="ts">
  import AgentSessionControls from "$lib/components/chat/AgentSessionControls.svelte";
  import ChatAgentModePicker from "$lib/components/chat/ChatAgentModePicker.svelte";
  import ChatExecutionTargetPicker from "$lib/components/chat/ChatExecutionTargetPicker.svelte";
  import UndertakingContextChip from "$lib/components/work/UndertakingContextChip.svelte";
  import ChatRuntimePicker from "$lib/components/chat/ChatRuntimePicker.svelte";
  import ComposerDraftsControl from "$lib/components/chat/ComposerDraftsControl.svelte";
  import ComposerTurnControls from "$lib/components/chat/ComposerTurnControls.svelte";
  import type { AgentSessionConfigOption } from "$lib/daemon";
  import { agentRuntimeLabel } from "$lib/utils/sessionAgentRuntime";
  import type { ChatAgentRuntime } from "$lib/utils/sessionAgentRuntime";

  interface Props {
    sessionId: string;
    value: ChatAgentRuntime;
    configOptions: AgentSessionConfigOption[];
    pending: boolean;
    disabled: boolean;
    model: string;
    onChange: (runtime: ChatAgentRuntime) => void;
    onConfigChange: (configId: string, value: unknown) => void | Promise<void>;
  }

  let {
    sessionId,
    value,
    configOptions,
    pending,
    disabled,
    model,
    onChange,
    onConfigChange,
  }: Props = $props();

  const switchingDisabled = $derived(disabled || pending);
</script>

<div class="composer-context-row">
  <UndertakingContextChip chatOnly composer />
  {#if value === "medousa"}<ChatExecutionTargetPicker {sessionId} disabled={switchingDisabled} />{/if}
</div>
<div class="chat-runtime-under">
  {#if value === "medousa"}
    <ChatAgentModePicker {sessionId} disabled={switchingDisabled} />
  {:else}
    <ChatRuntimePicker {value} disabled={switchingDisabled} {onChange} />
  {/if}
  <ComposerTurnControls {sessionId} disabled={switchingDisabled} showNativeControls={value === "medousa"} runtimeLabel={agentRuntimeLabel(value)}>
    {#snippet agentSettings()}
      <ChatRuntimePicker inline {value} disabled={switchingDisabled} {onChange} />
      {#if value !== "medousa"}
        <AgentSessionControls inline options={configOptions} includeModel={false} disabled={switchingDisabled} onChange={onConfigChange} />
      {/if}
    {/snippet}
    {#snippet drafts(close)}
      <ComposerDraftsControl inline {disabled} mode={value} {model} onRestore={close} />
    {/snippet}
  </ComposerTurnControls>
</div>
