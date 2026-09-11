<script lang="ts">
  import { onMount } from "svelte";
  import { Check, Pause, Play, RotateCcw } from "@lucide/svelte";
  import { getLiquidContext } from "$lib/liquid/render/context";
  import type { ArchetypeProps } from "$lib/liquid/render/types";
  import { renderInlineMarkdown } from "$lib/markdown/render";
  import type { LiquidActionProps, LiquidRecipeStep } from "$lib/markdown/liquidEmbeds";
  import {
    createTimerState,
    formatTimerMs,
    migrateTimerState,
    reconcileTimerState,
    transitionTimer,
    type TimerAction,
    type TimerStateV1,
  } from "$lib/liquid/timer/timerState";
  import {
    cancelTimerNotification,
    scheduleTimerNotification,
  } from "$lib/liquid/timer/timerNotifications";

  let { node }: ArchetypeProps = $props();
  const ctx = getLiquidContext();
  let timers = $state<Record<string, TimerStateV1>>({});
  let revisions = $state<Record<string, number>>({});
  let announcement = $state("");

  const title = $derived(typeof node.props.title === "string" ? node.props.title : "");
  const subtitle = $derived(typeof node.props.subtitle === "string" ? node.props.subtitle : "");
  const recipeYield = $derived(typeof node.props.yield === "string" ? node.props.yield : "");
  const notes = $derived(typeof node.props.notes === "string" ? node.props.notes : "");
  const ingredients = $derived(strings(node.props.ingredients));
  const resources = $derived(strings(node.props.resources));
  const actions = $derived.by((): LiquidActionProps[] => {
    if (!Array.isArray(node.props.actions)) return [];
    return node.props.actions.flatMap((raw) => {
      if (!raw || typeof raw !== "object") return [];
      const row = raw as Record<string, unknown>;
      const label = typeof row.label === "string" ? row.label.trim() : "";
      if (!label) return [];
      return [{ label, intent: typeof row.intent === "string" && row.intent.trim() ? row.intent.trim() : label }];
    });
  });
  const steps = $derived.by((): LiquidRecipeStep[] => {
    if (!Array.isArray(node.props.steps)) return [];
    return node.props.steps.flatMap((raw, index) => {
      if (!raw || typeof raw !== "object") return [];
      const row = raw as Record<string, unknown>;
      const label = typeof row.label === "string" ? row.label.trim() : "";
      if (!label) return [];
      const step: LiquidRecipeStep = {
        id: typeof row.id === "string" && row.id.trim() ? row.id.trim() : `step-${index + 1}`,
        label,
      };
      if (typeof row.body === "string" && row.body.trim()) step.body = row.body.trim();
      const duration = Number(row.durationMs ?? row.duration_ms);
      if (Number.isFinite(duration) && duration >= 1_000 && duration <= 86_400_000) step.durationMs = duration;
      if (typeof row.durationLabel === "string") step.durationLabel = row.durationLabel;
      return [step];
    });
  });

  function strings(value: unknown): string[] {
    return Array.isArray(value)
      ? value.filter((item): item is string => typeof item === "string" && Boolean(item.trim())).map((item) => item.trim())
      : [];
  }

  function scope(step: LiquidRecipeStep) {
    if (!ctx.sessionId || !ctx.messageId) return null;
    return { sessionId: ctx.sessionId, messageId: ctx.messageId, nodeId: node.id, instanceId: step.id };
  }

  function timer(step: LiquidRecipeStep): TimerStateV1 {
    return timers[step.id] ?? createTimerState(step.durationMs ?? 60_000);
  }

  async function loadTimers() {
    for (const step of steps.filter((step) => step.durationMs)) {
      const coordinate = scope(step);
      let state = createTimerState(step.durationMs!);
      if (coordinate && ctx.componentState) {
        try {
          const record = await ctx.componentState.get<TimerStateV1>(coordinate);
          if (record) {
            state = migrateTimerState(record.state, step.durationMs!);
            revisions = { ...revisions, [step.id]: record.revision };
          }
        } catch {
          // Static/local fallback remains fully usable for the current mount.
        }
      }
      const reconciled = reconcileTimerState(state);
      timers = { ...timers, [step.id]: reconciled };
      if (state.status === "running" && reconciled.status === "complete") {
        announcement = `${step.label} timer complete`;
        void persist(step, reconciled);
      }
    }
  }

  async function persist(step: LiquidRecipeStep, state: TimerStateV1) {
    const coordinate = scope(step);
    if (!coordinate || !ctx.componentState) return;
    try {
      const record = await ctx.componentState.put(coordinate, {
        schemaVersion: 1,
        componentType: "timer",
        expectedRevision: revisions[step.id] ?? 0,
        state,
      });
      revisions = { ...revisions, [step.id]: record.revision };
    } catch (error) {
      if (!/revision conflict/i.test(String(error))) return;
      const latest = await ctx.componentState.get<TimerStateV1>(coordinate).catch(() => null);
      if (latest) {
        timers = { ...timers, [step.id]: reconcileTimerState(migrateTimerState(latest.state, step.durationMs!)) };
        revisions = { ...revisions, [step.id]: latest.revision };
      }
    }
  }

  async function apply(step: LiquidRecipeStep, action: TimerAction) {
    const prior = timer(step);
    let next = transitionTimer(prior, action);
    await cancelTimerNotification(prior.notificationId);
    if (next.status === "running" && next.deadlineAt) {
      try {
        const notificationId = await scheduleTimerNotification({
          seed: `${ctx.sessionId ?? "local"}:${ctx.messageId ?? "message"}:${node.id}:${step.id}`,
          title: step.label,
          deadlineAt: next.deadlineAt,
        });
        if (notificationId) next = { ...next, notificationId };
      } catch {
        // Native scheduling is an enhancement; durable deadlines remain authoritative.
      }
    }
    timers = { ...timers, [step.id]: next };
    announcement = next.status === "complete" ? `${step.label} timer complete` : `${step.label} timer ${next.status}`;
    ctx.sink?.emit({
      nodeId: node.id,
      instanceId: step.id,
      type: "edit",
      disposition: "local_state",
      expectedStateRevision: revisions[step.id] ?? 0,
      payload: { action: `timer_${action}`, status: next.status, deadlineAt: next.deadlineAt },
      ts: Date.now(),
    });
    await persist(step, next);
  }

  function submitAction(action: LiquidActionProps) {
    ctx.sink?.emit({
      nodeId: node.id,
      instanceId: "recipe",
      type: "submit",
      disposition: "submit_turn",
      payload: { intent: action.intent || action.label },
      ts: Date.now(),
    });
  }

  onMount(() => {
    void loadTimers();
    const interval = window.setInterval(() => {
      for (const step of steps.filter((item) => item.durationMs)) {
        const prior = timer(step);
        const next = reconcileTimerState(prior);
        if (next.remainingMs !== prior.remainingMs || next.status !== prior.status) {
          timers = { ...timers, [step.id]: next };
        }
        if (prior.status === "running" && next.status === "complete") {
          announcement = `${step.label} timer complete`;
          void persist(step, next);
        }
      }
    }, 500);
    return () => window.clearInterval(interval);
  });
</script>

{#if title && steps.length}
  <article class="liquid-recipe" aria-labelledby={`${node.id}-title`}>
    <header>
      <div>
        <p class="eyebrow">Guided recipe</p>
        <h3 id={`${node.id}-title`}>{@html renderInlineMarkdown(title)}</h3>
        {#if subtitle}<p class="subtitle">{@html renderInlineMarkdown(subtitle)}</p>{/if}
      </div>
      {#if recipeYield}<span class="yield">{recipeYield}</span>{/if}
    </header>

    {#if ingredients.length || resources.length}
      <section class="resources">
        <h4>{ingredients.length ? "Ingredients" : "What you need"}</h4>
        <ul>{#each (ingredients.length ? ingredients : resources) as item}<li>{item}</li>{/each}</ul>
      </section>
    {/if}

    <ol class="steps">
      {#each steps as step, index (step.id)}
        <li>
          <span class="number">{index + 1}</span>
          <div class="step-copy">
            <div class="step-head"><strong>{@html renderInlineMarkdown(step.label)}</strong>{#if step.durationLabel}<span class="duration">{step.durationLabel}</span>{/if}</div>
            {#if step.body}<p>{@html renderInlineMarkdown(step.body)}</p>{/if}
            {#if step.durationMs}
              {@const current = timer(step)}
              <div class="timer" class:complete={current.status === "complete"}>
                <span class="clock">{current.status === "complete" ? "Done" : formatTimerMs(current.remainingMs)}</span>
                {#if current.status === "idle"}
                  <button type="button" onclick={() => apply(step, "start")}><Play size={14} /> Start</button>
                {:else if current.status === "running"}
                  <button type="button" onclick={() => apply(step, "pause")}><Pause size={14} /> Pause</button>
                {:else if current.status === "paused"}
                  <button type="button" onclick={() => apply(step, "resume")}><Play size={14} /> Resume</button>
                {:else}
                  <span class="done"><Check size={14} /> Complete</span>
                {/if}
                {#if current.status !== "idle"}
                  <button class="icon" type="button" aria-label={`Reset ${step.label} timer`} onclick={() => apply(step, "reset")}><RotateCcw size={14} /></button>
                {/if}
              </div>
            {/if}
          </div>
        </li>
      {/each}
    </ol>

    {#if notes}<aside class="notes"><strong>Notes</strong><p>{@html renderInlineMarkdown(notes)}</p></aside>{/if}
    {#if actions.length}<div class="actions">{#each actions as action}<button type="button" onclick={() => submitAction(action)}>{action.label}<span aria-hidden="true">›</span></button>{/each}</div>{/if}
    <p class="sr-only" aria-live="polite">{announcement}</p>
  </article>
{/if}

<style>
  .liquid-recipe { border: 1px solid color-mix(in srgb, var(--color-surface-500) 26%, transparent); border-radius: 1rem; background: color-mix(in srgb, var(--color-surface-900) 58%, transparent); overflow: hidden; color: rgb(var(--color-surface-100)); }
  header { display: flex; align-items: flex-start; justify-content: space-between; gap: 1rem; padding: 1rem; border-bottom: 1px solid color-mix(in srgb, var(--color-surface-500) 18%, transparent); }
  .eyebrow { margin: 0 0 .25rem; color: rgb(var(--color-primary-300)); font-size: .66rem; font-weight: 750; letter-spacing: .09em; text-transform: uppercase; }
  h3, h4, p { margin: 0; } h3 { font-size: 1.05rem; } h4 { font-size: .73rem; text-transform: uppercase; letter-spacing: .06em; color: rgb(var(--theme-text-tertiary)); }
  .subtitle { margin-top: .25rem; font-size: .8rem; color: rgb(var(--theme-text-tertiary)); }
  .yield, .duration { white-space: nowrap; border-radius: 999px; padding: .25rem .5rem; background: color-mix(in srgb, var(--color-primary-500) 16%, transparent); color: rgb(var(--color-primary-200)); font-size: .7rem; }
  .resources { padding: .8rem 1rem; background: color-mix(in srgb, var(--color-surface-800) 35%, transparent); }
  .resources ul { columns: 2; column-gap: 1.5rem; margin: .5rem 0 0; padding-left: 1.1rem; font-size: .8rem; } .resources li { break-inside: avoid; margin: .22rem 0; }
  .steps { list-style: none; margin: 0; padding: .3rem 1rem; }
  .steps > li { display: grid; grid-template-columns: 1.65rem 1fr; gap: .65rem; padding: .8rem 0; border-bottom: 1px solid color-mix(in srgb, var(--color-surface-500) 15%, transparent); }
  .steps > li:last-child { border-bottom: 0; } .number { display: grid; place-items: center; width: 1.55rem; height: 1.55rem; border-radius: 999px; background: color-mix(in srgb, var(--color-surface-600) 55%, transparent); font-size: .7rem; font-weight: 750; }
  .step-copy { min-width: 0; } .step-head { display: flex; justify-content: space-between; gap: .7rem; align-items: center; font-size: .86rem; } .step-copy > p { margin-top: .28rem; color: rgb(var(--theme-text-secondary)); font-size: .79rem; line-height: 1.45; }
  .timer { display: flex; align-items: center; gap: .45rem; margin-top: .6rem; padding: .42rem .5rem; border-radius: .65rem; background: color-mix(in srgb, var(--color-surface-700) 40%, transparent); } .timer.complete { background: color-mix(in srgb, var(--color-success-500) 13%, transparent); }
  .clock { min-width: 3.7rem; font: 700 .82rem/1 ui-monospace, monospace; } .timer button, .done { display: inline-flex; align-items: center; gap: .3rem; border: 0; border-radius: .5rem; padding: .38rem .55rem; background: color-mix(in srgb, var(--color-primary-500) 24%, transparent); color: inherit; font-size: .72rem; cursor: pointer; } .timer .icon { margin-left: auto; padding: .38rem; background: transparent; color: rgb(var(--theme-text-tertiary)); } .done { background: transparent; color: rgb(var(--color-success-300)); }
  .notes { margin: 0 1rem 1rem; padding: .7rem .8rem; border-radius: .7rem; background: color-mix(in srgb, var(--color-primary-500) 9%, transparent); font-size: .77rem; } .notes p { margin-top: .2rem; color: rgb(var(--theme-text-secondary)); }
  .actions { display: flex; gap: .5rem; padding: 0 1rem 1rem; flex-wrap: wrap; } .actions button { display: flex; justify-content: space-between; gap: .8rem; flex: 1 1 9rem; border: 1px solid color-mix(in srgb, var(--color-surface-500) 25%, transparent); border-radius: .65rem; padding: .6rem .7rem; color: inherit; background: color-mix(in srgb, var(--color-surface-800) 42%, transparent); cursor: pointer; }
  .sr-only { position: absolute; width: 1px; height: 1px; padding: 0; margin: -1px; overflow: hidden; clip: rect(0,0,0,0); white-space: nowrap; border: 0; }
  @media (max-width: 520px) { .resources ul { columns: 1; } header { padding: .85rem; } .steps { padding-inline: .85rem; } }
</style>
