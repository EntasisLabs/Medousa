<script lang="ts">
  import { Plus, Trash2, X } from "@lucide/svelte";
  import {
    compareEntityLabels,
    summarizeAccordionItems,
    summarizeCodeSource,
    summarizeCompareTable,
    summarizeDashboardTiles,
    summarizeSteps,
    summarizeTabsPanels,
    summarizeTreeText,
    type LiquidAccordionDraft,
    type LiquidCalloutDraft,
    type LiquidCardDraft,
    type LiquidCodeDraft,
    type LiquidCompareDraft,
    type LiquidDashboardDraft,
    type LiquidFenceDraft,
    type LiquidFenceLang,
    type LiquidStepsDraft,
    type LiquidTabsDraft,
    type LiquidTreeDraft,
  } from "$lib/utils/vaultLiquidFence";
  import ChartPipeTableEditor from "./ChartPipeTableEditor.svelte";
  import VaultLiquidQuietItemEdit from "./VaultLiquidQuietItemEdit.svelte";
  import VaultLiquidQuietList from "./VaultLiquidQuietList.svelte";

  interface Props {
    open: boolean;
    lang: LiquidFenceLang | null;
    initial: LiquidFenceDraft | null;
    onSave: (next: LiquidFenceDraft) => void;
    onClose: () => void;
  }

  let { open, lang = null, initial = null, onSave, onClose }: Props = $props();

  const DASHBOARD_TONES = [
    { id: "default", label: "Default" },
    { id: "accent", label: "Accent" },
    { id: "success", label: "Success" },
    { id: "warn", label: "Warn" },
    { id: "error", label: "Error" },
  ] as const;

  const CALLOUT_TONES = [
    { id: "note", label: "Note" },
    { id: "warn", label: "Warn" },
    { id: "error", label: "Error" },
    { id: "success", label: "Success" },
  ] as const;

  const STEP_STATUSES = [
    { id: "done", label: "Done" },
    { id: "current", label: "Current" },
    { id: "pending", label: "Pending" },
  ] as const;

  let card = $state<LiquidCardDraft>({
    title: "",
    subtitle: "",
    emoji: "",
    body: "",
    meta: "",
    points: [],
  });
  let callout = $state<LiquidCalloutDraft>({
    tone: "note",
    title: "",
    body: "",
  });
  let dashboard = $state<LiquidDashboardDraft>({
    title: "",
    columns: "2",
    tiles: [],
  });
  let tabs = $state<LiquidTabsDraft>({
    title: "",
    defaultLabel: "",
    panels: [],
  });
  let steps = $state<LiquidStepsDraft>({ title: "", steps: [] });
  let accordion = $state<LiquidAccordionDraft>({
    title: "",
    multiple: false,
    items: [],
  });
  let code = $state<LiquidCodeDraft>({ lang: "typescript", title: "", source: "" });
  let tree = $state<LiquidTreeDraft>({ title: "", treeText: "" });
  let compare = $state<LiquidCompareDraft>({
    title: "",
    subtitle: "",
    recommendation: "",
    mode: "matrix",
    tableMarkdown: "| | Option A | Option B |\n| --- | --- | --- |\n| Axis | … | … |",
  });

  let editing = $state<
    "emoji" | "meta" | "points" | "tone" | "default" | "lang" | "recommendation" | null
  >(null);
  /** List / source swap inside the sheet (tiles, panels, steps, items, source, tree). */
  let listView = $state(false);
  /** Nested quiet-row editor index (accordion / tabs / steps). */
  let itemEditIndex = $state<number | null>(null);

  const sheetTitle = $derived(
    lang
      ? lang.charAt(0).toUpperCase() + lang.slice(1)
      : "Configure",
  );

  $effect(() => {
    if (!open || !initial) return;
    switch (initial.lang) {
      case "card":
        card = {
          ...initial.draft,
          points: initial.draft.points.map((p) => ({ ...p })),
        };
        break;
      case "callout":
        callout = { ...initial.draft };
        break;
      case "dashboard":
        dashboard = {
          ...initial.draft,
          tiles: initial.draft.tiles.map((t) => ({ ...t })),
        };
        break;
      case "tabs":
        tabs = {
          ...initial.draft,
          panels: initial.draft.panels.map((p) => ({ ...p })),
        };
        break;
      case "steps":
        steps = {
          ...initial.draft,
          steps: initial.draft.steps.map((s) => ({ ...s })),
        };
        break;
      case "accordion":
        accordion = {
          ...initial.draft,
          items: initial.draft.items.map((i) => ({ ...i })),
        };
        break;
      case "code":
        code = { ...initial.draft };
        break;
      case "tree":
        tree = { ...initial.draft };
        break;
      case "compare":
        compare = { ...initial.draft };
        break;
    }
    editing = null;
    listView = false;
    itemEditIndex = null;
  });

  function commit() {
    if (lang === "card") {
      onSave({ lang: "card", draft: card });
      return;
    }
    if (lang === "callout") {
      onSave({ lang: "callout", draft: callout });
      return;
    }
    if (lang === "dashboard") {
      const tiles = dashboard.tiles.filter(
        (t) => t.label.trim() || t.value.trim(),
      );
      while (tiles.length < 2) {
        tiles.push({
          label: tiles.length === 0 ? "Metric" : "Status",
          value: "—",
          tone: tiles.length === 0 ? "default" : "accent",
          delta: "",
        });
      }
      onSave({ lang: "dashboard", draft: { ...dashboard, tiles } });
      return;
    }
    if (lang === "tabs") {
      const panels = tabs.panels.filter(
        (p) => p.label.trim() || p.body.trim(),
      );
      while (panels.length < 2) {
        panels.push({
          label: panels.length === 0 ? "Install" : "Run",
          body: "…",
        });
      }
      onSave({ lang: "tabs", draft: { ...tabs, panels } });
      return;
    }
    if (lang === "steps") {
      const next = steps.steps.filter((s) => s.label.trim());
      while (next.length < 2) {
        next.push({
          label: next.length === 0 ? "Build" : "Ship",
          body: "…",
          status: next.length === 0 ? "done" : "current",
        });
      }
      onSave({ lang: "steps", draft: { ...steps, steps: next } });
      return;
    }
    if (lang === "accordion") {
      const items = accordion.items.filter(
        (i) => i.label.trim() || i.body.trim(),
      );
      if (items.length < 1) {
        items.push({ label: "Item", body: "…", open: true });
      }
      onSave({ lang: "accordion", draft: { ...accordion, items } });
      return;
    }
    if (lang === "code") {
      onSave({
        lang: "code",
        draft: {
          ...code,
          source: code.source.trimEnd() || "// …",
          lang: code.lang.trim() || "text",
        },
      });
      return;
    }
    if (lang === "tree") {
      onSave({
        lang: "tree",
        draft: {
          ...tree,
          treeText: tree.treeText.trimEnd() || "src/",
        },
      });
      return;
    }
    if (lang === "compare") {
      onSave({ lang: "compare", draft: { ...compare } });
    }
  }

  function leaveListOrItem() {
    if (itemEditIndex != null) {
      itemEditIndex = null;
      return;
    }
    listView = false;
  }

  function onSheetKeydown(event: KeyboardEvent) {
    if (event.key !== "Escape") return;
    event.preventDefault();
    if (itemEditIndex != null) {
      itemEditIndex = null;
      return;
    }
    if (listView) {
      listView = false;
      return;
    }
    if (editing) {
      editing = null;
      return;
    }
    onClose();
  }

  const quietListLang = $derived(
    lang === "accordion" || lang === "tabs" || lang === "steps",
  );

  const quietRows = $derived.by(() => {
    if (lang === "accordion") {
      return accordion.items.map((i) => ({ title: i.label, body: i.body }));
    }
    if (lang === "tabs") {
      return tabs.panels.map((p) => ({ title: p.label, body: p.body }));
    }
    if (lang === "steps") {
      return steps.steps.map((s) => ({ title: s.label, body: s.body }));
    }
    return [];
  });

  const quietMinRows = $derived(lang === "accordion" ? 1 : 2);

  const quietAddLabel = $derived(
    lang === "tabs" ? "Add panel" : lang === "steps" ? "Add step" : "Add item",
  );

  function openQuietEdit(index: number) {
    itemEditIndex = index;
  }

  function quietRemove(index: number) {
    if (lang === "accordion") removeItem(index);
    else if (lang === "tabs") removePanel(index);
    else if (lang === "steps") removeStep(index);
    if (itemEditIndex === index) itemEditIndex = null;
    else if (itemEditIndex != null && itemEditIndex > index) {
      itemEditIndex -= 1;
    }
  }

  function quietAdd() {
    if (lang === "accordion") addItem();
    else if (lang === "tabs") addPanel();
    else if (lang === "steps") addStep();
  }

  function setQuietTitle(value: string) {
    if (itemEditIndex == null) return;
    const i = itemEditIndex;
    if (lang === "accordion") {
      accordion = {
        ...accordion,
        items: accordion.items.map((item, idx) =>
          idx === i ? { ...item, label: value } : item,
        ),
      };
    } else if (lang === "tabs") {
      tabs = {
        ...tabs,
        panels: tabs.panels.map((panel, idx) =>
          idx === i ? { ...panel, label: value } : panel,
        ),
      };
    } else if (lang === "steps") {
      steps = {
        ...steps,
        steps: steps.steps.map((step, idx) =>
          idx === i ? { ...step, label: value } : step,
        ),
      };
    }
  }

  function setQuietBody(value: string) {
    if (itemEditIndex == null) return;
    const i = itemEditIndex;
    if (lang === "accordion") {
      accordion = {
        ...accordion,
        items: accordion.items.map((item, idx) =>
          idx === i ? { ...item, body: value } : item,
        ),
      };
    } else if (lang === "tabs") {
      tabs = {
        ...tabs,
        panels: tabs.panels.map((panel, idx) =>
          idx === i ? { ...panel, body: value } : panel,
        ),
      };
    } else if (lang === "steps") {
      steps = {
        ...steps,
        steps: steps.steps.map((step, idx) =>
          idx === i ? { ...step, body: value } : step,
        ),
      };
    }
  }

  function toggleAccordionOpen(index: number) {
    accordion = {
      ...accordion,
      items: accordion.items.map((item, idx) =>
        idx === index ? { ...item, open: !item.open } : item,
      ),
    };
  }

  function setStepStatus(index: number, status: string) {
    steps = {
      ...steps,
      steps: steps.steps.map((step, idx) =>
        idx === index ? { ...step, status } : step,
      ),
    };
  }

  function addPoint() {
    card = { ...card, points: [...card.points, { label: "", value: "" }] };
  }
  function removePoint(index: number) {
    card = { ...card, points: card.points.filter((_, i) => i !== index) };
  }
  function addTile() {
    dashboard = {
      ...dashboard,
      tiles: [
        ...dashboard.tiles,
        { label: "", value: "", tone: "default", delta: "" },
      ],
    };
  }
  function removeTile(index: number) {
    if (dashboard.tiles.length <= 2) return;
    dashboard = {
      ...dashboard,
      tiles: dashboard.tiles.filter((_, i) => i !== index),
    };
  }
  function addPanel() {
    tabs = { ...tabs, panels: [...tabs.panels, { label: "", body: "" }] };
  }
  function removePanel(index: number) {
    if (tabs.panels.length <= 2) return;
    tabs = { ...tabs, panels: tabs.panels.filter((_, i) => i !== index) };
  }
  function addStep() {
    steps = {
      ...steps,
      steps: [...steps.steps, { label: "", body: "", status: "pending" }],
    };
  }
  function removeStep(index: number) {
    if (steps.steps.length <= 2) return;
    steps = { ...steps, steps: steps.steps.filter((_, i) => i !== index) };
  }
  function addItem() {
    accordion = {
      ...accordion,
      items: [...accordion.items, { label: "", body: "", open: false }],
    };
  }
  function removeItem(index: number) {
    if (accordion.items.length <= 1) return;
    accordion = {
      ...accordion,
      items: accordion.items.filter((_, i) => i !== index),
    };
  }

  const listLabel = $derived(
    lang === "dashboard"
      ? "Tiles"
      : lang === "tabs"
        ? "Panels"
        : lang === "steps"
          ? "Steps"
          : lang === "accordion"
            ? "Items"
            : lang === "code"
              ? "Source"
              : lang === "tree"
                ? "Tree"
                : lang === "compare"
                  ? "Table"
                  : "Edit",
  );

  const compareEntities = $derived(compareEntityLabels(compare));

  const listHeaderTitle = $derived(
    itemEditIndex != null
      ? lang === "tabs"
        ? "Edit panel"
        : lang === "steps"
          ? "Edit step"
          : "Edit item"
      : listLabel,
  );
</script>

{#if open && lang}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="vault-interact-backdrop"
    role="dialog"
    aria-modal="true"
    aria-labelledby="liquid-builder-title"
    tabindex="-1"
    onkeydown={onSheetKeydown}
    onclick={(event) => {
      if (event.target === event.currentTarget) onClose();
    }}
  >
    <form
      class="vault-interact-sheet vault-chart-builder-sheet vault-liquid-builder-sheet"
      class:vault-liquid-builder-sheet--list={listView}
      onsubmit={(event) => {
        event.preventDefault();
        if (itemEditIndex != null) {
          itemEditIndex = null;
          return;
        }
        if (listView) {
          listView = false;
          return;
        }
        commit();
      }}
    >
      {#if listView}
        <header class="vault-chart-builder-header">
          <h3 id="liquid-builder-title" class="vault-chart-data-title">{listHeaderTitle}</h3>
          <button
            type="button"
            class="vault-interact-dismiss shrink-0"
            aria-label="Back"
            onclick={leaveListOrItem}
          >
            <X size={14} strokeWidth={2} />
          </button>
        </header>

        {#if quietListLang && itemEditIndex != null && quietRows[itemEditIndex]}
          <VaultLiquidQuietItemEdit
            titleLabel={lang === "accordion" ? "Question" : "Title"}
            bodyLabel={lang === "accordion" ? "Answer" : "Body"}
            titlePlaceholder={lang === "accordion" ? "Question" : "Label"}
            bodyPlaceholder={lang === "accordion" ? "Answer" : "Body"}
            title={quietRows[itemEditIndex].title}
            body={quietRows[itemEditIndex].body}
            onTitleChange={setQuietTitle}
            onBodyChange={setQuietBody}
          />
        {:else if quietListLang && lang === "tabs"}
          <VaultLiquidQuietList
            rows={quietRows}
            addLabel={quietAddLabel}
            minRows={quietMinRows}
            onEdit={openQuietEdit}
            onRemove={quietRemove}
            onAdd={quietAdd}
          />
        {:else if quietListLang && lang === "accordion"}
          <VaultLiquidQuietList
            rows={quietRows}
            addLabel={quietAddLabel}
            minRows={quietMinRows}
            onEdit={openQuietEdit}
            onRemove={quietRemove}
            onAdd={quietAdd}
          >
            {#snippet leading(index)}
              <button
                type="button"
                class="vault-liquid-quiet-toggle"
                class:vault-liquid-quiet-toggle--on={accordion.items[index]?.open}
                aria-pressed={accordion.items[index]?.open ?? false}
                title={accordion.items[index]?.open ? "Open by default" : "Closed by default"}
                aria-label={accordion.items[index]?.open ? "Open by default" : "Closed by default"}
                onclick={() => toggleAccordionOpen(index)}
              >
                <span class="vault-liquid-quiet-toggle__knob"></span>
              </button>
            {/snippet}
          </VaultLiquidQuietList>
        {:else if quietListLang && lang === "steps"}
          <VaultLiquidQuietList
            rows={quietRows}
            addLabel={quietAddLabel}
            minRows={quietMinRows}
            onEdit={openQuietEdit}
            onRemove={quietRemove}
            onAdd={quietAdd}
          >
            {#snippet leading(index)}
              <select
                class="vault-liquid-quiet-status"
                aria-label="Step status"
                value={steps.steps[index]?.status ?? "pending"}
                onchange={(event) =>
                  setStepStatus(
                    index,
                    (event.currentTarget as HTMLSelectElement).value,
                  )}
              >
                {#each STEP_STATUSES as status (status.id)}
                  <option value={status.id}>{status.label}</option>
                {/each}
              </select>
            {/snippet}
          </VaultLiquidQuietList>
        {:else if lang === "dashboard"}
          <div class="vault-liquid-list">
            {#each dashboard.tiles as tile, index (index)}
              <div class="vault-liquid-list__item">
                <input class="vault-liquid-list__field" type="text" placeholder="Label" aria-label="Tile label" bind:value={tile.label} />
                <input class="vault-liquid-list__field" type="text" placeholder="Value" aria-label="Tile value" bind:value={tile.value} />
                <select class="vault-liquid-list__select" aria-label="Tone" bind:value={tile.tone}>
                  {#each DASHBOARD_TONES as tone (tone.id)}
                    <option value={tone.id}>{tone.label}</option>
                  {/each}
                </select>
                <input class="vault-liquid-list__field vault-liquid-list__field--delta" type="text" placeholder="Delta" aria-label="Delta" bind:value={tile.delta} />
                <button type="button" class="vault-liquid-list__remove" aria-label="Remove tile" disabled={dashboard.tiles.length <= 2} onclick={() => removeTile(index)}>
                  <Trash2 size={14} strokeWidth={2} />
                </button>
              </div>
            {/each}
            <button type="button" class="vault-liquid-list__add" onclick={addTile}>
              <Plus size={14} strokeWidth={2} /> Add tile
            </button>
          </div>
        {:else if lang === "code"}
          <div class="vault-liquid-source">
            <textarea
              class="vault-liquid-source__area"
              rows="14"
              spellcheck="false"
              aria-label="Source"
              bind:value={code.source}
            ></textarea>
          </div>
        {:else if lang === "tree"}
          <div class="vault-liquid-source">
            <textarea
              class="vault-liquid-source__area"
              rows="14"
              spellcheck="false"
              aria-label="Tree"
              placeholder={"src/\n  lib/\n    index.ts"}
              bind:value={tree.treeText}
            ></textarea>
          </div>
        {:else if lang === "compare"}
          <div class="vault-liquid-compare-table">
            <ChartPipeTableEditor
              content={compare.tableMarkdown}
              onchange={(next) => {
                compare = { ...compare, tableMarkdown: next };
              }}
            />
          </div>
        {/if}

        <footer class="vault-chart-builder-footer">
          <button type="submit" class="vault-chart-builder-done">Done</button>
        </footer>
      {:else}
        <header class="vault-chart-builder-header">
          <div class="vault-chart-builder-identity min-w-0">
            {#if lang === "card"}
              <input id="liquid-builder-title" class="vault-chart-builder-title-input" type="text" placeholder="Untitled card" aria-label="Card title" bind:value={card.title} />
              <input class="vault-chart-builder-desc-input" type="text" placeholder="Subtitle" aria-label="Subtitle" bind:value={card.subtitle} />
            {:else if lang === "callout"}
              <input id="liquid-builder-title" class="vault-chart-builder-title-input" type="text" placeholder="Untitled callout" aria-label="Callout title" bind:value={callout.title} />
            {:else if lang === "dashboard"}
              <input id="liquid-builder-title" class="vault-chart-builder-title-input" type="text" placeholder="Untitled dashboard" aria-label="Dashboard title" bind:value={dashboard.title} />
            {:else if lang === "tabs"}
              <input id="liquid-builder-title" class="vault-chart-builder-title-input" type="text" placeholder="Untitled tabs" aria-label="Tabs title" bind:value={tabs.title} />
            {:else if lang === "steps"}
              <input id="liquid-builder-title" class="vault-chart-builder-title-input" type="text" placeholder="Untitled steps" aria-label="Steps title" bind:value={steps.title} />
            {:else if lang === "accordion"}
              <input id="liquid-builder-title" class="vault-chart-builder-title-input" type="text" placeholder="Untitled accordion" aria-label="Accordion title" bind:value={accordion.title} />
            {:else if lang === "code"}
              <input id="liquid-builder-title" class="vault-chart-builder-title-input" type="text" placeholder="filename.ts" aria-label="Code title" bind:value={code.title} />
            {:else if lang === "compare"}
              <input id="liquid-builder-title" class="vault-chart-builder-title-input" type="text" placeholder="Untitled compare" aria-label="Compare title" bind:value={compare.title} />
              <input class="vault-chart-builder-desc-input" type="text" placeholder="Subtitle" aria-label="Subtitle" bind:value={compare.subtitle} />
            {:else}
              <input id="liquid-builder-title" class="vault-chart-builder-title-input" type="text" placeholder="Untitled tree" aria-label="Tree title" bind:value={tree.title} />
            {/if}
          </div>
          <button type="button" class="vault-interact-dismiss shrink-0" aria-label="Close" onclick={onClose}>
            <X size={14} strokeWidth={2} />
          </button>
        </header>

        <div class="vault-chart-facts">
          {#if lang === "card"}
            <div class="vault-chart-fact" class:vault-chart-fact--open={editing === "emoji"} data-chart-fact-row>
              <div class="vault-chart-fact__row">
                <span class="vault-chart-fact__label">Emoji</span>
                {#if editing !== "emoji"}
                  <button type="button" class="vault-chart-fact__value" onclick={() => (editing = "emoji")}>{card.emoji.trim() || "None"}</button>
                {/if}
              </div>
              {#if editing === "emoji"}
                <div class="vault-chart-fact__editor">
                  <input class="vault-chart-fact__free" type="text" maxlength="8" placeholder="📋" aria-label="Emoji" bind:value={card.emoji} onkeydown={(e) => { if (e.key === "Enter") { e.preventDefault(); editing = null; } }} />
                </div>
              {/if}
            </div>
            <div class="vault-chart-fact" class:vault-chart-fact--open={editing === "meta"} data-chart-fact-row>
              <div class="vault-chart-fact__row">
                <span class="vault-chart-fact__label">Meta</span>
                {#if editing !== "meta"}
                  <button type="button" class="vault-chart-fact__value" onclick={() => (editing = "meta")}>{card.meta.trim() || "None"}</button>
                {/if}
              </div>
              {#if editing === "meta"}
                <div class="vault-chart-fact__editor">
                  <input class="vault-chart-fact__free" type="text" placeholder="Optional meta line" aria-label="Meta" bind:value={card.meta} onkeydown={(e) => { if (e.key === "Enter") { e.preventDefault(); editing = null; } }} />
                </div>
              {/if}
            </div>
            <div class="vault-chart-fact" class:vault-chart-fact--open={editing === "points"} data-chart-fact-row>
              <div class="vault-chart-fact__row">
                <span class="vault-chart-fact__label">Points</span>
                <button type="button" class="vault-chart-fact__value" onclick={() => (editing = editing === "points" ? null : "points")}>
                  {card.points.length ? `${card.points.length} point${card.points.length === 1 ? "" : "s"}` : "None"}
                </button>
              </div>
              {#if editing === "points"}
                <div class="vault-chart-fact__editor vault-liquid-points">
                  {#each card.points as point, index (index)}
                    <div class="vault-liquid-points__row">
                      <input class="vault-chart-fact__free" type="text" placeholder="Label" aria-label="Point label" bind:value={point.label} />
                      <input class="vault-chart-fact__free" type="text" placeholder="Value" aria-label="Point value" bind:value={point.value} />
                      <button type="button" class="vault-liquid-list__remove" aria-label="Remove point" onclick={() => removePoint(index)}>
                        <Trash2 size={14} strokeWidth={2} />
                      </button>
                    </div>
                  {/each}
                  <button type="button" class="vault-liquid-list__add" onclick={addPoint}>
                    <Plus size={14} strokeWidth={2} /> Add point
                  </button>
                </div>
              {/if}
            </div>
            <div class="vault-chart-fact" data-chart-fact-row>
              <div class="vault-chart-fact__row">
                <span class="vault-chart-fact__label">Body</span>
                <span class="vault-chart-fact__value vault-chart-fact__value--static">{card.body.trim() ? "On page" : "Empty"}</span>
              </div>
            </div>
          {:else if lang === "callout"}
            <div class="vault-chart-fact" class:vault-chart-fact--open={editing === "tone"} data-chart-fact-row>
              <div class="vault-chart-fact__row">
                <span class="vault-chart-fact__label">Tone</span>
                {#if editing !== "tone"}
                  <button type="button" class="vault-chart-fact__value" onclick={() => (editing = "tone")}>{callout.tone}</button>
                {/if}
              </div>
              {#if editing === "tone"}
                <div class="vault-chart-fact__editor">
                  <div class="vault-chart-builder-seg" role="listbox" aria-label="Tone">
                    {#each CALLOUT_TONES as tone (tone.id)}
                      <button
                        type="button"
                        class="vault-chart-builder-seg__btn"
                        class:vault-chart-builder-seg__btn--on={callout.tone === tone.id}
                        role="option"
                        aria-selected={callout.tone === tone.id}
                        onclick={() => {
                          callout = { ...callout, tone: tone.id };
                          editing = null;
                        }}
                      >
                        {tone.label}
                      </button>
                    {/each}
                  </div>
                </div>
              {/if}
            </div>
            <div class="vault-chart-fact" data-chart-fact-row>
              <div class="vault-chart-fact__row">
                <span class="vault-chart-fact__label">Body</span>
                <span class="vault-chart-fact__value vault-chart-fact__value--static">{callout.body.trim() ? "On page" : "Empty"}</span>
              </div>
            </div>
          {:else if lang === "dashboard"}
            <div class="vault-chart-fact" data-chart-fact-row>
              <div class="vault-chart-fact__row">
                <span class="vault-chart-fact__label">Tiles</span>
                <button type="button" class="vault-chart-fact__value" onclick={() => (listView = true)}>{summarizeDashboardTiles(dashboard)}</button>
              </div>
            </div>
            <div class="vault-chart-fact" data-chart-fact-row>
              <div class="vault-chart-fact__row">
                <span class="vault-chart-fact__label">Columns</span>
                <span class="vault-chart-fact__value vault-chart-fact__value--static">{dashboard.columns}</span>
              </div>
            </div>
          {:else if lang === "tabs"}
            <div class="vault-chart-fact" data-chart-fact-row>
              <div class="vault-chart-fact__row">
                <span class="vault-chart-fact__label">Panels</span>
                <button type="button" class="vault-chart-fact__value" onclick={() => (listView = true)}>{summarizeTabsPanels(tabs)}</button>
              </div>
            </div>
            <div class="vault-chart-fact" class:vault-chart-fact--open={editing === "default"} data-chart-fact-row>
              <div class="vault-chart-fact__row">
                <span class="vault-chart-fact__label">Default</span>
                {#if editing !== "default"}
                  <button type="button" class="vault-chart-fact__value" onclick={() => (editing = "default")}>
                    {tabs.defaultLabel.trim() || tabs.panels[0]?.label || "First"}
                  </button>
                {/if}
              </div>
              {#if editing === "default"}
                <div class="vault-chart-fact__editor">
                  <div class="vault-chart-builder-seg" role="listbox" aria-label="Default tab">
                    {#each tabs.panels as panel, index (index)}
                      {@const label = panel.label.trim() || `Tab ${index + 1}`}
                      <button
                        type="button"
                        class="vault-chart-builder-seg__btn"
                        class:vault-chart-builder-seg__btn--on={tabs.defaultLabel === panel.label.trim()}
                        role="option"
                        aria-selected={tabs.defaultLabel === panel.label.trim()}
                        onclick={() => {
                          tabs = { ...tabs, defaultLabel: panel.label.trim() };
                          editing = null;
                        }}
                      >
                        {label}
                      </button>
                    {/each}
                  </div>
                </div>
              {/if}
            </div>
          {:else if lang === "steps"}
            <div class="vault-chart-fact" data-chart-fact-row>
              <div class="vault-chart-fact__row">
                <span class="vault-chart-fact__label">Steps</span>
                <button type="button" class="vault-chart-fact__value" onclick={() => (listView = true)}>{summarizeSteps(steps)}</button>
              </div>
            </div>
          {:else if lang === "accordion"}
            <div class="vault-chart-fact" data-chart-fact-row>
              <div class="vault-chart-fact__row">
                <span class="vault-chart-fact__label">Items</span>
                <button type="button" class="vault-chart-fact__value" onclick={() => (listView = true)}>{summarizeAccordionItems(accordion)}</button>
              </div>
            </div>
            <div class="vault-chart-fact" data-chart-fact-row>
              <div class="vault-chart-fact__row">
                <span class="vault-chart-fact__label">Multiple</span>
                <button
                  type="button"
                  class="vault-chart-fact__value"
                  onclick={() => (accordion = { ...accordion, multiple: !accordion.multiple })}
                >
                  {accordion.multiple ? "On" : "Off"}
                </button>
              </div>
            </div>
          {:else if lang === "code"}
            <div class="vault-chart-fact" data-chart-fact-row>
              <div class="vault-chart-fact__row">
                <span class="vault-chart-fact__label">Source</span>
                <button type="button" class="vault-chart-fact__value" onclick={() => (listView = true)}>{summarizeCodeSource(code)}</button>
              </div>
            </div>
            <div class="vault-chart-fact" class:vault-chart-fact--open={editing === "lang"} data-chart-fact-row>
              <div class="vault-chart-fact__row">
                <span class="vault-chart-fact__label">Lang</span>
                {#if editing !== "lang"}
                  <button type="button" class="vault-chart-fact__value" onclick={() => (editing = "lang")}>{code.lang || "text"}</button>
                {/if}
              </div>
              {#if editing === "lang"}
                <div class="vault-chart-fact__editor">
                  <input
                    class="vault-chart-fact__free"
                    type="text"
                    spellcheck="false"
                    placeholder="typescript"
                    aria-label="Language"
                    bind:value={code.lang}
                    onkeydown={(e) => {
                      if (e.key === "Enter") {
                        e.preventDefault();
                        editing = null;
                      }
                    }}
                  />
                </div>
              {/if}
            </div>
          {:else if lang === "tree"}
            <div class="vault-chart-fact" data-chart-fact-row>
              <div class="vault-chart-fact__row">
                <span class="vault-chart-fact__label">Tree</span>
                <button type="button" class="vault-chart-fact__value" onclick={() => (listView = true)}>{summarizeTreeText(tree)}</button>
              </div>
            </div>
          {:else if lang === "compare"}
            <div class="vault-chart-fact" data-chart-fact-row>
              <div class="vault-chart-fact__row">
                <span class="vault-chart-fact__label">Table</span>
                <button type="button" class="vault-chart-fact__value" onclick={() => (listView = true)}>{summarizeCompareTable(compare)}</button>
              </div>
            </div>
            <div class="vault-chart-fact" data-chart-fact-row>
              <div class="vault-chart-fact__row">
                <span class="vault-chart-fact__label">Mode</span>
                <button
                  type="button"
                  class="vault-chart-fact__value"
                  onclick={() =>
                    (compare = {
                      ...compare,
                      mode: compare.mode === "faceoff" ? "matrix" : "faceoff",
                    })}
                >
                  {compare.mode === "faceoff" ? "Face-off" : "Matrix"}
                </button>
              </div>
            </div>
            <div class="vault-chart-fact" class:vault-chart-fact--open={editing === "recommendation"} data-chart-fact-row>
              <div class="vault-chart-fact__row">
                <span class="vault-chart-fact__label">Pick</span>
                {#if editing !== "recommendation"}
                  <button type="button" class="vault-chart-fact__value" onclick={() => (editing = "recommendation")}>
                    {compare.recommendation.trim() || "None"}
                  </button>
                {/if}
              </div>
              {#if editing === "recommendation"}
                <div class="vault-chart-fact__editor">
                  <div class="vault-chart-builder-seg" role="listbox" aria-label="Recommended entity">
                    <button
                      type="button"
                      class="vault-chart-builder-seg__btn"
                      class:vault-chart-builder-seg__btn--on={!compare.recommendation.trim()}
                      role="option"
                      aria-selected={!compare.recommendation.trim()}
                      onclick={() => {
                        compare = { ...compare, recommendation: "" };
                        editing = null;
                      }}
                    >
                      None
                    </button>
                    {#each compareEntities as entity, entityIndex (entityIndex)}
                      <button
                        type="button"
                        class="vault-chart-builder-seg__btn"
                        class:vault-chart-builder-seg__btn--on={compare.recommendation.trim() === entity}
                        role="option"
                        aria-selected={compare.recommendation.trim() === entity}
                        onclick={() => {
                          compare = { ...compare, recommendation: entity };
                          editing = null;
                        }}
                      >
                        {entity}
                      </button>
                    {/each}
                  </div>
                </div>
              {/if}
            </div>
          {/if}
        </div>

        <footer class="vault-chart-builder-footer">
          <span class="vault-liquid-builder-kind">{sheetTitle}</span>
          <button type="submit" class="vault-chart-builder-done">Done</button>
        </footer>
      {/if}
    </form>
  </div>
{/if}
