<script lang="ts">
  import AgentSessionControls from "$lib/components/chat/AgentSessionControls.svelte";
  import ChatAgentModePicker from "$lib/components/chat/ChatAgentModePicker.svelte";
  import ChatRuntimePicker from "$lib/components/chat/ChatRuntimePicker.svelte";
  import ComposerTurnControls from "$lib/components/chat/ComposerTurnControls.svelte";
  import type { AgentSessionConfigOption } from "$lib/daemon";
  import type { ChatAgentRuntime } from "$lib/utils/sessionAgentRuntime";

  interface Props {
    sessionId: string;
    value: ChatAgentRuntime;
    configOptions: AgentSessionConfigOption[];
    pending: boolean;
    disabled: boolean;
    onChange: (runtime: ChatAgentRuntime) => void;
    onConfigChange: (configId: string, value: unknown) => void | Promise<void>;
  }

  let {
    sessionId,
    value,
    configOptions,
    pending,
    disabled,
    onChange,
    onConfigChange,
  }: Props = $props();

  const switchingDisabled = $derived(disabled || pending);
</script>

<div class="chat-runtime-under" aria-label="Composer controls">
  <div class="chat-runtime-primary">
    <ChatRuntimePicker {value} disabled={switchingDisabled} {onChange} />
    {#if value === "medousa"}
      <ChatAgentModePicker {sessionId} disabled={switchingDisabled} />
    {/if}
  </div>
  <ComposerTurnControls {sessionId} disabled={switchingDisabled} showNativeControls={value === "medousa"}>
    {#snippet agentSettings()}
      {#if value !== "medousa"}
        <AgentSessionControls inline options={configOptions} includeModel={false} disabled={switchingDisabled} onChange={onConfigChange} />
      {/if}
    {/snippet}
  </ComposerTurnControls>
</div>
