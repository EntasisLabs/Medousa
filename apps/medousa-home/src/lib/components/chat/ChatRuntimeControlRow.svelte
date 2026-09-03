<script lang="ts">
  import AgentSessionControls from "$lib/components/chat/AgentSessionControls.svelte";
  import ChatAgentModePicker from "$lib/components/chat/ChatAgentModePicker.svelte";
  import ChatNarrationToggle from "$lib/components/chat/ChatNarrationToggle.svelte";
  import ChatRuntimePicker from "$lib/components/chat/ChatRuntimePicker.svelte";
  import ComposerDraftsControl from "$lib/components/chat/ComposerDraftsControl.svelte";
  import ComposerTurnControls from "$lib/components/chat/ComposerTurnControls.svelte";
  import type { AgentSessionConfigOption } from "$lib/daemon";
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

<div class="chat-runtime-under">
  <ChatRuntimePicker {value} disabled={switchingDisabled} {onChange} />
  {#if value === "medousa"}
    <ChatAgentModePicker {sessionId} disabled={switchingDisabled} />
  {/if}
  <ChatNarrationToggle />
  <ComposerTurnControls {disabled} showNativeControls={value === "medousa"} />
  {#if value !== "medousa"}
    <AgentSessionControls
      options={configOptions}
      includeModel={false}
      disabled={switchingDisabled}
      onChange={onConfigChange}
    />
  {/if}
  <ComposerDraftsControl {disabled} mode={value} {model} />
</div>
