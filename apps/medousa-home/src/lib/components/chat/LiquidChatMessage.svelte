<script lang="ts">
  /**
   * Renders one chat message through the Liquid scene renderer (PR3, no daemon).
   * The message is adapted to a `document` scene; interaction flows back through
   * an event sink. Gated by the `liquidChat` flag in `ChatMessageList`.
   */
  import "$lib/liquid/archetypes";
  import { SceneRenderer } from "$lib/liquid/render";
  import type { LiquidRenderContext } from "$lib/liquid/render";
  import type { SceneEvent } from "$lib/liquid/core";
  import type { EventSink } from "$lib/liquid/ports";
  import { chatMessageToScene } from "$lib/liquid/surfaces/chat/messageToScene";
  import { settings } from "$lib/stores/settings.svelte";
  import { visibleChatStatusLine } from "$lib/utils/chatStreamDisplay";
  import type { ChatMessage } from "$lib/types/chat";
  import type { ToolHistorySliceRef } from "$lib/types/toolHistory";

  interface Props {
    message: ChatMessage;
    sessionId: string;
    mobile?: boolean;
    compact?: boolean;
    onPromoteToFlow?: (ref: ToolHistorySliceRef) => void | Promise<void>;
    onRetryWorker?: (workId: string) => void;
  }

  let {
    message,
    sessionId,
    mobile = false,
    compact = false,
    onPromoteToFlow,
    onRetryWorker,
  }: Props = $props();

  const statusLine = $derived(
    visibleChatStatusLine(message.statusLine, settings.showEngineDetailsInChat),
  );
  const statusWarn = $derived(
    message.phase === "worker_ack" || message.phase === "awaiting_operator",
  );

  const scene = $derived(chatMessageToScene(message, { statusLine, statusWarn }));

  const sink: EventSink = {
    emit(event: SceneEvent) {
      if (event.type !== "run") return;
      const payload = event.payload as { action?: string; workId?: string } | undefined;
      if (payload?.action === "retry_worker" && payload.workId) {
        onRetryWorker?.(payload.workId);
      }
    },
  };

  const context = $derived<LiquidRenderContext>({
    sink,
    sessionId,
    mobile,
    compact,
    openLinksInWeb: true,
    onPromoteToFlow,
  });
</script>

<SceneRenderer node={scene} {context} />
