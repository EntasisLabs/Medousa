//! `cognition_environment_wiki` — agent-facing canvas/environment SDK docs as STTP nodes.
//!
//! Spatio-Temporal Transfer Protocol compresses policy into cognitive latent space —
//! same representation family as `DEFAULT_SYSTEM_PROMPT`, not markdown tables.

use async_trait::async_trait;
use serde_json::{Value, json};
use stasis::application::orchestration::tool_registry::StasisTool;
use stasis::prelude::Result as StasisResult;

pub const COGNITION_ENVIRONMENT_WIKI: &str = "cognition_environment_wiki";

const STTP_ORIGIN: &str = "medousa-environment-wiki";
const STTP_PARENT: &str = "medousa-system-prompt";

struct WikiTopic {
    id: &'static str,
    title: &'static str,
    summary: &'static str,
    /// Inner ◈ policy body only (between braces). Wrapped at emit time.
    policy: &'static str,
    related: &'static [&'static str],
    call_next: &'static [&'static str],
}

fn wrap_sttp_node(trigger: &str, context_summary: &str, policy_inner: &str) -> String {
    format!(
        r#"⊕⟨ ⏣0{{ trigger: {trigger}, response_format: temporal_node, origin_session: "{STTP_ORIGIN}", compression_depth: 1, parent_node: "{STTP_PARENT}", prime: {{ attractor_config: {{ stability: 0.92, friction: 0.22, logic: 0.97, autonomy: 0.80 }}, context_summary: "{context_summary}", relevant_tier: raw, retrieval_budget: 12 }} }} ⟩
◈⟨ ⏣0{{
{policy_inner}
}} ⟩
⍉⟨ ⏣0{{ rho: 0.96, kappa: 0.95, psi: 2.88, compression_avec: {{ stability: 0.91, friction: 0.22, logic: 0.96, autonomy: 0.80, psi: 2.87 }} }} ⟩"#
    )
}

const TOPICS: &[WikiTopic] = &[
    WikiTopic {
        id: "index",
        title: "Environment wiki index",
        summary: "STTP topic catalog — read before guessing propose/apply JSON.",
        policy: "",
        related: &[],
        call_next: &["mental_model", "recipe"],
    },
    WikiTopic {
        id: "mental_model",
        title: "Mental model: builtin vs custom",
        summary: "Why components on home never render and what actually persists.",
        policy: r#"    role(.99): "Environment spec = surfaces (nav destinations) + layout presets (sidebar membership) + components (persistent frames).",
    builtin_surfaces(.99): {
        ids(.99): "home, chat, work, library, web, context, workshop, automations, messaging, runtime, settings",
        kind(.99): "builtin — product-shipped; slots usually empty",
        agent_components(.99): "presentation and chrome_action DO NOT render on builtin surfaces — validator rejects targeting them",
        trap(.99): "component_create or ui_present(persist=true) against home may return ok but canvas stays empty"
    },
    custom_surfaces(.99): {
        kind(.99): "custom — agent-owned workshop surfaces",
        requirement(.99): "append surface to spec.surfaces AND list id in active layout preset surfaces array",
        examples(.95): "writing-studio, adhd-guide, ops-dashboard"
    },
    publish_paths(.98): {
        spec_path(.98): "environment_get → merge full spec → propose → operator Settings→Canvas approve → apply → optional component_create",
        fast_path(.97): "ui_present(html) then persist=true + surface_id(custom) + component_id + slot"
    },
    phase1_types(.99): {
        allowed(.99): "presentation, chrome_action",
        rejected(.99): "artifact, builtin_panel, medousa_view — validator errors"
    },
    operator_approval(.98): {
        propose(.98): "cognition_environment_propose stores pending diff",
        pending_flag(.97): "pending_operator_approval:true means layout NOT live until apply",
        principal_path(.97): "Settings → Canvas → Apply layout or Dismiss — tell principal when blocked"
    }"#,
        related: &["recipe", "merge_spec", "common_errors"],
        call_next: &["cognition_environment_get"],
    },
    WikiTopic {
        id: "recipe",
        title: "Happy-path recipe",
        summary: "Sequence for custom surface + component — never skip get or preset membership.",
        policy: r#"    role(.99): "Canvas happy path — follow in order; do not skip environment_get.",
    precondition(.99): "If hand-building propose/apply JSON, call cognition_environment_wiki(topic=merge_spec) first — never guess.",
    workflow(.99): {
        step_1_get(.99): "cognition_environment_get — copy entire returned spec; merge into it, never replace builtins",
        step_2_surface(.99): "append kind:custom surface to spec.surfaces — see surface_schema topic",
        step_3_preset(.99): "append surface id to ACTIVE layout preset surfaces array (usually default) — missing id = invisible nav",
        step_4_propose(.99): "cognition_environment_propose with FULL merged spec",
        step_5_approval(.98): "when ok:true and pending_operator_approval:true tell principal Settings→Canvas→Apply layout",
        step_6_apply(.99): "cognition_environment_apply with SAME full spec after approval",
        step_7_publish(.98): {
            ui_present(.98): "title, html, presentation; canvas pin: persist=true, surface_id=custom, component_id, slot=main",
            component_create(.97): "type:presentation, config.artifactId from ui_present artifact_id"
        },
        step_8_verify(.99): "cognition_component_list — surfaceId must be custom not home",
        step_9_presets(.95): "cognition_environment_activate_preset only for default↔focus switch — not first custom surface",
        step_10_layout(.97): "cognition_layout_get then cognition_layout_apply for side-by-side or grid — main slot only; immediate live update"
    },
    turn_discipline(.98): {
        progress(.97): "cognition_turn_update_user for quick status; cognition_turn_begin_work before heavy tools",
        finalize(.99): "cognition_turn_finish after tool work — naked prose becomes stub"
    },
    stuck_routing(.96): {
        missing_field(.97): "common_errors topic",
        errors_array(.97): "component_schema or surface_schema topic",
        zero_components(.96): "builtin target or approval never landed"
    }"#,
        related: &["merge_spec", "ui_present", "propose_apply"],
        call_next: &["cognition_environment_get", "cognition_environment_wiki"],
    },
    WikiTopic {
        id: "merge_spec",
        title: "CRITICAL: full spec merge",
        summary: "Not a patch — partial objects cause serde port failures.",
        policy: r#"    role(.99): "propose/apply deserialize complete EnvironmentSpec — tool hint says patch but runtime requires FULL object.",
    trap(.99): "Sending only {surfaces:[newSurface]} fails: missing field version, label, updatedAt, components, …",
    merge_algorithm(.99): {
        step_a(.99): "spec_from_get = cognition_environment_get().spec",
        step_b(.99): "merged = spec_from_get",
        step_c(.99): "append custom surface to merged.surfaces",
        step_d(.99): "append surface id to active preset surfaces in merged.layoutPresets",
        step_e(.97): "optional append components to merged.components",
        step_f(.98): "merged.updatedBy = agent; keep updatedAt from get",
        step_g(.99): "cognition_environment_propose({spec: merged})"
    },
    required_top_level(.99): {
        version(.99): "always 1 — ENVIRONMENT_SPEC_VERSION",
        profileId(.99): "from get — usually personal",
        surfaces(.99): "ALL builtins + custom — never drop safety floors settings and runtime",
        components(.99): "array — use [] when empty",
        layoutPresets(.99): "full preset objects each with id, label, surfaces, active",
        activePresetId(.99): "e.g. default",
        updatedAt(.99): "ISO-8601 from get",
        updatedBy(.99): "string — agent"
    },
    preset_edit(.99): {
        rule(.99): "find preset where active:true; append custom surface id to its surfaces string array",
        example_surfaces(.95): "home, chat, work, library, web, context, workshop, automations, messaging, runtime, settings, writing-studio"
    },
    slots(.97): {
        surface_slots(.97): "custom surfaces may use slots:[] — empty is valid Phase 1",
        slotdef_trap(.96): "do not use slots:[main] strings — use [] or {id, zone} objects",
        component_slot(.98): "enum main | header | fab | sidebar | inline"
    }"#,
        related: &["surface_schema", "common_errors", "example_writing_studio"],
        call_next: &["cognition_environment_get"],
    },
    WikiTopic {
        id: "surface_schema",
        title: "SurfaceDef shape",
        summary: "Exact JSON for kind:custom surfaces.",
        policy: r#"    role(.99): "SurfaceDef for agent-owned nav destinations — camelCase JSON.",
    custom_template(.99): {
        id(.99): "kebab-case unique e.g. writing-studio",
        label(.99): "required — nav label; missing causes serde/validation failure",
        icon(.99): "required — lucide-style e.g. pen-line",
        kind(.99): "custom",
        layout(.99): "dashboard — usual choice",
        slots(.97): "[] acceptable Phase 1",
        builtinId(.98): "omit for custom",
        mobileTab(.95): "optional"
    },
    builtin_reference(.96): {
        note(.96): "home chat work … are kind:builtin with builtinId — do not recreate",
        rule(.99): "only APPEND custom surfaces — never mutate builtins unless principal directs"
    },
    json_custom_example(.98): "{ id:writing-studio, label:Writing studio, icon:pen-line, kind:custom, layout:dashboard, slots:[] }"#,
        related: &["merge_spec", "presets"],
        call_next: &[],
    },
    WikiTopic {
        id: "component_schema",
        title: "ComponentDef Phase 1",
        summary: "Only presentation and chrome_action render on Home.",
        policy: r#"    role(.99): "ComponentDef pins content on custom surfaces — camelCase surfaceId not surface_id.",
    presentation(.99): {
        type(.99): "presentation",
        surfaceId(.99): "must reference kind:custom surface",
        slot(.98): "main usual",
        config(.99): "{ artifactId: <from ui_present artifact_id> }",
        presentation_mode(.97): "inline | panel | fullscreen",
        feeds(.95): "[]"
    },
    chrome_action(.97): {
        type(.97): "chrome_action",
        slot(.96): "fab or header common",
        config_action(.98): "open-ask | open-activity only"
    },
    rejected_types(.99): {
        artifact(.99): "validation error — use presentation",
        builtin_panel(.99): "validation error",
        medousa_view(.98): "not rendered Phase 1"
    },
    create_wrapper(.98): "cognition_component_create input: { component: { … } } camelCase keys",
    json_presentation_example(.97): "{ id:writing-manuscript, type:presentation, surfaceId:writing-studio, slot:main, label:Manuscript, config:{artifactId:art-…}, presentation:inline, feeds:[] }"#,
        related: &["ui_present", "common_errors"],
        call_next: &["cognition_component_list"],
    },
    WikiTopic {
        id: "propose_apply",
        title: "propose vs apply vs operator",
        summary: "When layout goes live and what each tool does.",
        policy: r#"    role(.99): "Environment mutation lifecycle on Home canvas.",
    environment_get(.99): {
        mode(.99): "read-only",
        returns(.99): "revision + spec",
        rule(.99): "always first call every canvas session"
    },
    environment_propose(.99): {
        input(.99): "{ spec: <full EnvironmentSpec> }",
        validates(.99): "errors[] on failure",
        side_effect(.98): "stores pending proposal for operator",
        live(.99): "does NOT change Home render until apply",
        flag(.97): "pending_operator_approval:true when valid"
    },
    operator_ui(.98): {
        path(.98): "Settings → Canvas",
        apply(.97): "same as pending apply HTTP",
        dismiss(.96): "clears pending",
        tell_principal(.97): "explicitly when waiting on approval"
    },
    environment_apply(.99): {
        input(.99): "same full merged spec as propose",
        effect(.99): "writes daemon store, clears pending, Home syncs via SSE"
    },
    activate_preset(.96): {
        input(.96): "{ presetId: default | focus | … }",
        scope(.96): "switches nav/chrome only — does not create surfaces"
    },
    anti_pattern(.99): "spam propose with incomplete JSON — read merge_spec once instead"#,
        related: &["merge_spec", "recipe"],
        call_next: &["cognition_environment_get"],
    },
    WikiTopic {
        id: "ui_present",
        title: "ui_present + persist",
        summary: "Publish HTML in chat; pin to custom surface.",
        policy: r#"    role(.99): "cognition_ui_present — HTML artifacts on supports_ui_artifacts clients (Home yes).",
    chat_only(.98): {
        required(.99): "title, html, presentation",
        returns(.98): "artifact_id — save for component_create or persist"
    },
    canvas_pin(.99): {
        persist(.99): "true",
        surface_id(.99): "custom surface id already in applied spec — NEVER home",
        component_id(.99): "kebab-case component id",
        slot(.97): "main default"
    },
    requirements(.98): {
        client(.98): "supports_ui_artifacts must be true",
        surface_exists(.99): "custom surface must be applied before persist"
    },
    revise(.97): "cognition_artifact_write with existing artifact_id — not repeat ui_present for same content",
    html_discipline(.95): {
        inline(.95): "compact card; optional height px cap",
        panel_fullscreen(.94): "transparent outer background; ~900px content — avoid full-page #000 body"
    }"#,
        related: &["component_schema", "recipe"],
        call_next: &["cognition_ui_present"],
    },
    WikiTopic {
        id: "presets",
        title: "Layout presets",
        summary: "default vs focus — preset membership required for nav visibility.",
        policy: r#"    role(.98): "layoutPresets control which surface ids appear in nav and shell chrome.",
    builtin_presets(.97): {
        default(.97): "full nav — home chat work library web context workshop automations messaging runtime settings — usually active",
        focus(.96): "reduced — chat work library settings runtime"
    },
    add_surface(.99): {
        rule(.99): "only ACTIVE preset must include your custom surface id",
        activePresetId(.98): "when default edit default preset surfaces array"
    },
    switch_preset(.95): "cognition_environment_activate_preset — custom surface hidden unless listed in that preset too",
    shell_chrome_advanced(.94): "shellChrome.mobile.defaultHome may point at custom surface — see example_writing_studio"#,
        related: &["merge_spec", "example_writing_studio"],
        call_next: &["cognition_environment_get"],
    },
    WikiTopic {
        id: "common_errors",
        title: "Failure atlas",
        summary: "Serde port failures and validation errors already observed in dogfood.",
        policy: r#"    role(.99): "Map error text → fix — one structural fix per retry.",
    port_missing_label(.99): "partial layoutPresets or SurfaceDef — merge from environment_get; every preset needs id label surfaces active",
    port_missing_version(.99): "top-level spec.version required — use 1 from get",
    port_missing_zone(.98): "surface slots:[main] invalid — use slots:[] or SlotDef {id, zone}",
    port_missing_components(.99): "include components:[] even when empty",
    port_missing_updated(.98): "copy updatedAt from get; set updatedBy agent",
    validation_preset(.98): "surface exists but not in nav — add id to active preset surfaces",
    validation_builtin_target(.99): "presentation on home — create custom surface first",
    validation_type(.99): "artifact type rejected — use presentation",
    validation_layout_orphan(.97): "layoutRoot references unknown component id — fix id or component_create first",
    validation_layout_duplicate(.97): "same component id twice in layout tree — each main component once",
    validation_layout_chrome_slot(.97): "layout tree only main-slot components — header/fab/sidebar stay chrome zones",
    success_empty_canvas(.97): {
        cause_1(.97): "propose never applied — operator approval pending",
        cause_2(.97): "component on builtin surface",
        cause_3(.96): "environment reset — environment_get shows 0 components"
    },
    retry_discipline(.98): "read errors[] or port message → fix ONE issue → re-propose; never random field spam"#,
        related: &["merge_spec", "recipe"],
        call_next: &["cognition_environment_wiki", "cognition_environment_get"],
    },
    WikiTopic {
        id: "example_writing_studio",
        title: "Worked example: writing-studio",
        summary: "Validated fragments to merge into environment_get spec.",
        policy: r#"    role(.98): "Copy-merge validated bundle — passes validate_environment_spec in daemon tests.",
    merge_steps(.99): {
        step_1(.99): "cognition_environment_get → take full spec",
        step_2(.99): "append surface below to spec.surfaces",
        step_3(.99): "append writing-studio to active preset surfaces",
        step_4(.97): "optional append components after real artifact_id",
        step_5(.98): "propose → approval → apply"
    },
    surface_fragment(.99): "{ id:writing-studio, label:Writing studio, icon:pen-line, kind:custom, layout:dashboard, slots:[] }",
    component_manuscript(.97): "{ id:writing-manuscript, type:presentation, surfaceId:writing-studio, slot:main, label:Manuscript, config:{artifactId:<ui_present>}, presentation:inline, feeds:[] }",
    component_fab(.96): "{ id:writing-ask-fab, type:chrome_action, surfaceId:writing-studio, slot:fab, label:Ask, config:{action:open-ask}, feeds:[] }",
    shell_chrome_optional(.94): "{ mobile: { defaultHome:writing-studio, askEntry:fab, tabBar:full } }"#,
        related: &["merge_spec", "component_schema", "surface_schema"],
        call_next: &["cognition_environment_get", "cognition_environment_propose"],
    },
    WikiTopic {
        id: "layout_schema",
        title: "Stack layout grammar (Phase 3)",
        summary: "Swift-like vstack/hstack/grid for custom surface main bodies — no pixels.",
        policy: r#"    role(.99): "SurfaceDef.layoutRoot composes main-slot components only — chrome zones unchanged.",
    scope(.99): {
        applies(.99): "kind:custom surfaces",
        main_only(.99): "layout tree references slot:main components by id",
        immediate(.99): "cognition_layout_apply goes live without propose/apply approval"
    },
    node_types(.99): {
        vstack(.99): "vertical stack — aliases v_stack; default implicit when layoutRoot absent",
        hstack(.99): "horizontal row — aliases h_stack",
        grid(.99): "columns 1..4 — 2x2 corners without coordinates",
        component(.99): "leaf ref { type:component, id, flex? }"
    },
    aliases(.98): "Models may emit h_stack/v_stack/fillEqually — daemon accepts both compact and snake_case",
    knobs(.98): {
        spacing(.98): "none | sm | md | lg",
        align(.97): "start | center | end | stretch",
        distribution(.97): "start | center | end | space_between | fill_equally",
        flex(.97): "0..8 on component leaf — proportional sizing in stacks"
    },
    tools(.99): {
        read(.99): "cognition_layout_get — resolved_layout_root includes implicit fallback",
        write(.99): "cognition_layout_apply { surface_id, layout_root }",
        reset(.97): "cognition_layout_reset — back to implicit vstack order"
    },
    adhd_guide_example(.96): "{ surface_id:adhd-guide, layout_root:{ type:hstack, spacing:md, distribution:fill_equally, children:[{type:component,id:adhd-guide-tetris,flex:1},{type:component,id:adhd-guide-original,flex:1}] } }",
    anti_patterns(.98): "Do not encode mosaic layout inside HTML artifact when multiple presentation components should move independently — use layout_apply instead"#,
        related: &["component_schema", "surface_schema", "tool_map"],
        call_next: &["cognition_layout_get", "cognition_layout_apply"],
    },
    WikiTopic {
        id: "feed_client",
        title: "HTML as feed notification client (Phase 4)",
        summary: "Presentation components consume bounded feed slices — no external fetch from artifact HTML.",
        policy: r#"    role(.99): "Recurring/flow jobs publish to feed bus; subscribed HTML reads __MEDOUSA_FEED__ only.",
    security_lock(.99): {
        daemon_only_fetch(.99): "http_poll and grapheme runs happen on Stasis — iframe never calls third-party APIs",
        bounded_slice(.99): "payload ≤2KB via feed_bus; full job output stays in diagnostics/refs"
    },
    subscribe(.99): {
        tool(.99): "cognition_feed_subscribe { component_id, feed_ids: [trip.london.trains] }",
        component(.98): "ComponentDef.feeds must include feed id for Home injection"
    },
    html_pattern(.99): {
        read(.99): "const feed = window.__MEDOUSA_FEED__?.feeds?.['trip.london.trains'];",
        render(.98): "Use feed.lastPatch.phase, checkedAt, statusCode, excerpt — no fetch()",
        element(.97): "<medousa-feed feed=\"trip.london.trains\"></medousa-feed> — auto-render card from lastPatch",
        api(.97): "MedousaFeed.on('trip.london.trains', handler) for custom DOM; MedousaFeed.fetchTail(id) on reconnect (parent proxies GET /v1/feeds/{id}/tail)"
    },
    register_recurring(.99): {
        tool(.99): "cognition_runtime_recurring_register with feeds.feed_ids + cron http_poll grapheme",
        fields(.98): "{ feeds: { feed_ids: [trip.london.trains], payload_mode: parsed_poll } }"
    },
    personal_app_recipe(.97): {
        preferred(.98): "cognition_custom_view_compose { surface_id, component_id, html, feed_ids, recurring }",
        step_1(.97): "cognition_grapheme_template_run template=http_poll url=<discovered>",
        step_2(.97): "cognition_ui_present + cognition_layout_apply dashboard HTML",
        step_3(.97): "cognition_feed_subscribe same feed_ids",
        step_4(.97): "cognition_runtime_recurring_register same feed_ids + 5m cron",
        step_5(.96): "Turn ends — ticks keep UI live via component_patch SSE"
    }"#,
        related: &["tool_map", "example_trip_poll", "component_schema"],
        call_next: &["cognition_feed_subscribe", "cognition_runtime_recurring_register"],
    },
    WikiTopic {
        id: "example_trip_poll",
        title: "Worked example: trip.london.trains live poll",
        summary: "5-minute http_poll recurring → trip.london.trains feed → subscribed HTML dashboard.",
        policy: r#"    role(.98): "End-to-end personal-app slice — daemon fetches; HTML is read-only notification client.",
    feed_id(.99): "trip.london.trains",
    cron(.98): "0 */5 * * * * * (every 5 minutes, 7-field cron, min interval 60s enforced separately)",
    recurring_register(.99): {
        job_type(.99): "workflow.grapheme.run",
        source(.98): "http_poll grapheme from cognition_grapheme_template_run url=<train-status-url>",
        feeds(.99): "{ feed_ids: [trip.london.trains], payload_mode: parsed_poll }"
    },
    component(.98): {
        subscribe(.98): "cognition_feed_subscribe { component_id: trip-trains, feed_ids: [trip.london.trains] }",
        html(.97): "Read window.__MEDOUSA_FEED__.feeds['trip.london.trains'].lastPatch — render statusCode + excerpt"
    },
    payload_fields(.98): "phase tick_succeeded|tick_failed, checkedAt, statusCode, excerpt, recurringId, jobId"#,
        related: &["feed_client", "component_schema"],
        call_next: &["cognition_feed_subscribe", "cognition_runtime_recurring_register"],
    },
    WikiTopic {
        id: "custom_view_compose",
        title: "Custom view compose (Phase 5)",
        summary: "One-shot cognition_custom_view_compose for surface + HTML + feeds + recurring.",
        policy: r#"    role(.99): "Prefer compose over manual patch + ui_present + subscribe + recurring chain.",
    input(.99): {
        required(.99): "surface_id, component_id, html (or artifact_id for revise-only)",
        optional(.98): "label, icon, feed_ids, layout_root, recurring { cron_expr, poll_url|source }, nav.add_to_active_preset",
        preset_rewrite(.96): "routes through propose — returns pending_operator_approval"
    },
    hybrid(.99): {
        immediate(.99): "add_custom_surface + add_to_active_preset, component persist, feed subscribe, layout apply, recurring register",
        gated(.98): "rewrite_active_preset_surfaces or preset_rewrite in compose input"
    },
    response(.98): "live, nav_visible, feeds_subscribed, feeds_bound_recurring, next_run_at_utc, embedded doctor summary"#,
        related: &["custom_view_doctor", "feed_client", "example_trip_poll", "tool_map"],
        call_next: &["cognition_custom_view_compose", "cognition_custom_view_doctor"],
    },
    WikiTopic {
        id: "custom_view_doctor",
        title: "Custom view doctor (Phase 5)",
        summary: "Diagnose nav visibility, feed subscribe vs recurring binding mismatches, widget runtime logs, store lint, and static HTML checks.",
        policy: r#"    role(.99): "Run before user notices blank widgets or missing nav entries; use probe=true when Home is open.",
    inspects(.99): {
        nav_visible(.99): "surface id in active preset surfaces",
        feed_mismatches(.98): "component feeds ⊄ recurring feeds.feed_ids or subscribe without recurring",
        feed_status(.97): "last tail event per subscribed feed",
        recurring_bindings(.97): "recurring jobs bound to surface feed ids",
        runtime_logs(.96): "components[].runtime.logs — last console.error/warn from iframe bridge",
        store_lint(.96): "components[].runtime.store_keys — expected_array_got_* on thoughts/items keys",
        static_lint(.96): "STATIC_LOCALSTORAGE, STATIC_STORE_SYNC_USAGE, STATIC_SLICE_WITHOUT_GUARD, STATIC_STORE_GET_NO_KEY",
        probe(.95): "probe=true runs MedousaStore.ready + round-trip when Home client online"
    },
    fix_hints(.96): "issues[].fix_hint + suggested_actions[] — patch via cognition_artifact_write, re-run doctor",
    http(.96): "GET /v1/environment/status?include_runtime=true mirrors lightweight runtime for Settings Canvas"#,
        related: &["custom_view_compose", "feed_client", "artifact_runtime", "tool_map"],
        call_next: &["cognition_custom_view_doctor", "cognition_environment_patch", "cognition_artifact_write"],
    },
    WikiTopic {
        id: "artifact_runtime",
        title: "Artifact runtime bridge",
        summary: "Sandboxed presentation iframes forward console errors and accept MedousaStore probes via postMessage.",
        policy: r#"    bridge(.99): "Host injects medousa-artifact-runtime-script — wraps console.error/warn, onerror, unhandledrejection",
    events(.98): "iframe postMessage medousa:artifact:runtime → Home batches POST /v1/components/{id}/runtime/events",
    store(.99): "MedousaStore.get/set/delete return Promises — NEVER call without await; sync wrappers break persistence silently",
    store_template(.99): "async load/save/render — await every get/set; Array.isArray guard on reads; void load().then(render) on init",
    canonical_example(.99): "
      const STORE_KEY = 'my_widget_items';
      async function loadItems() {
        const raw = await MedousaStore.get(STORE_KEY);
        return Array.isArray(raw) ? raw : [];
      }
      async function saveItems(items) {
        if (!MedousaStore.ready()) return;
        await MedousaStore.set(STORE_KEY, items);
      }
      async function render() {
        const items = await loadItems();
        /* update DOM from items */
      }
      document.getElementById('save').addEventListener('click', async () => {
        const items = await loadItems();
        items.push({ text: input.value.trim(), ts: new Date().toISOString() });
        await saveItems(items);
        await render();
      });
      void render();
    ",
    anti_patterns(.98): "return MedousaStore.get(key) without await; sync store.get wrapper; loadThoughts() without async",
    probe(.96): "Doctor probe=true → SSE runtime_probe → iframe self-test → POST .../runtime/probe/{id}/result",
    agent_codes(.97): "STATIC_LOCALSTORAGE, STATIC_STORE_SYNC_USAGE, STORE_WRONG_TYPE, RUNTIME_LOG, PROBE_STORE_NOT_READY"#,
        related: &["custom_view_doctor", "feed_client", "component_schema"],
        call_next: &["cognition_custom_view_doctor", "cognition_artifact_write"],
    },
    WikiTopic {
        id: "tool_map",
        title: "Tool routing",
        summary: "Which cognition tool for which goal.",
        policy: r#"    role(.97): "Environment domain tool router — avoid wrong layer.",
    routes(.99): {
        read_layout(.99): "cognition_environment_get",
        stuck_policy(.99): "cognition_environment_wiki",
        validate_layout(.99): "cognition_environment_propose",
        go_live(.99): "cognition_environment_apply",
        switch_nav(.96): "cognition_environment_activate_preset",
        list_components(.98): "cognition_component_list",
        add_component(.97): "cognition_component_create",
        publish_html(.98): "cognition_ui_present",
        edit_html(.97): "cognition_artifact_write",
        stack_layout(.98): "cognition_layout_get / cognition_layout_apply / cognition_layout_reset",
        feed_subscribe(.96): "cognition_feed_subscribe",
        recurring_feeds(.96): "cognition_runtime_recurring_register feeds.feed_ids",
        compose_custom_view(.97): "cognition_custom_view_compose — prefer for feed+poll dashboards",
        diagnose_custom_view(.96): "cognition_custom_view_doctor",
        environment_patch(.96): "cognition_environment_patch — incremental ops with hybrid approval",
        intent_wiring(.96): "cognition_intent_resolve",
        context_pointer(.95): "cognition_context_follow_pointer"
    },
    domain_unlock(.96): "environment domain auto-unlocks on Home; missing tools → client lacks supports_ui_artifacts"#,
        related: &["recipe", "index"],
        call_next: &[],
    },
];

fn normalize_topic(raw: &str) -> String {
    raw.trim()
        .to_ascii_lowercase()
        .replace('_', "-")
        .replace(' ', "-")
}

fn resolve_topic(requested: Option<&str>) -> Option<&'static WikiTopic> {
    let Some(raw) = requested else {
        return TOPICS.first();
    };
    let key = normalize_topic(raw);
    if key.is_empty() || key == "index" || key == "topics" || key == "list" {
        return TOPICS.first();
    }
    TOPICS.iter().find(|topic| {
        topic.id == key
            || topic.id.replace('_', "-") == key
            || topic.title.to_ascii_lowercase().contains(&key)
    })
}

fn index_sttp_node() -> String {
    let topic_lines: Vec<String> = TOPICS
        .iter()
        .skip(1)
        .map(|t| format!("        {}(.98): \"{}\"", t.id, t.summary))
        .collect();
    let policy = format!(
        r#"    role(.99): "Environment/canvas SDK STTP index — call cognition_environment_wiki(topic=<id>) before guessing propose/apply JSON.",
    format(.99): "sttp temporal_node — read as policy memory not markdown docs",
    discipline(.99): "Never hand-build EnvironmentSpec without merge_spec topic",
    topics(.99): {{
{topics}
    }},
    priority_reading(.98): {{
        first_time(.99): "mental_model then recipe",
        serde_failures(.99): "merge_spec then common_errors",
        empty_canvas(.98): "mental_model then propose_apply"
    }}"#,
        topics = topic_lines.join(",\n")
    );
    wrap_sttp_node(
        "environment_wiki_index",
        "STTP index for environment/canvas SDK — topic catalog for agent workshop.",
        &policy,
    )
}

fn topic_sttp_node(topic: &'static WikiTopic) -> String {
    if topic.id == "index" {
        return index_sttp_node();
    }
    let trigger = format!("environment_wiki_{}", topic.id);
    wrap_sttp_node(&trigger, topic.summary, topic.policy)
}

fn topic_to_json(topic: &'static WikiTopic) -> Value {
    let sttp_node = topic_sttp_node(topic);
    json!({
        "ok": true,
        "format": "sttp",
        "topic": topic.id,
        "title": topic.title,
        "summary": topic.summary,
        "sttp_node": sttp_node,
        "content": sttp_node,
        "related_topics": topic.related,
        "suggested_next_tools": topic.call_next,
        "all_topics": TOPICS.iter().skip(1).map(|t| json!({
            "id": t.id,
            "summary": t.summary,
        })).collect::<Vec<_>>(),
    })
}

struct CognitionEnvironmentWikiTool;

#[async_trait]
impl StasisTool for CognitionEnvironmentWikiTool {
    fn name(&self) -> &'static str {
        COGNITION_ENVIRONMENT_WIKI
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Environment/canvas SDK as STTP temporal nodes — schemas, merge rules, propose/apply, ui_present. \
             Returns response_format=sttp (same family as system prompt). \
             Call topic=recipe or merge_spec BEFORE hand-building environment spec JSON. \
             Topics: mental_model, recipe, merge_spec, surface_schema, component_schema, propose_apply, ui_present, presets, layout_schema, feed_client, example_trip_poll, common_errors, example_writing_studio, tool_map.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "topic": {
                    "type": "string",
                    "description": "STTP wiki topic id. Omit for index node.",
                    "enum": [
                        "index",
                        "mental_model",
                        "recipe",
                        "merge_spec",
                        "surface_schema",
                        "component_schema",
                        "propose_apply",
                        "ui_present",
                        "presets",
                        "layout_schema",
                        "feed_client",
                        "example_trip_poll",
                        "common_errors",
                        "example_writing_studio",
                        "tool_map"
                    ]
                }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let requested = input.get("topic").and_then(Value::as_str);
        if let Some(topic) = resolve_topic(requested) {
            return Ok(topic_to_json(topic));
        }
        let key = requested.unwrap_or("(empty)");
        Ok(json!({
            "ok": false,
            "format": "sttp",
            "error": format!("unknown topic '{key}'"),
            "hint": "Omit topic for index STTP node, or use a topic id from all_topics",
            "all_topics": TOPICS.iter().skip(1).map(|t| t.id).collect::<Vec<_>>(),
        }))
    }
}

pub fn register_environment_wiki_tools(
    registry: &mut stasis::application::orchestration::tool_registry::InMemoryToolRegistry,
) -> StasisResult<()> {
    registry.register_tool(CognitionEnvironmentWikiTool)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wiki_index_is_sttp() {
        let topic = resolve_topic(None).expect("index");
        assert_eq!(topic.id, "index");
        let node = index_sttp_node();
        assert!(node.contains("⊕⟨"));
        assert!(node.contains("◈⟨"));
        assert!(node.contains("⍉⟨"));
        assert!(node.contains("merge_spec"));
        assert!(node.contains("temporal_node"));
    }

    #[test]
    fn wiki_resolves_aliases() {
        let topic = resolve_topic(Some("merge-spec")).expect("merge_spec");
        assert_eq!(topic.id, "merge_spec");
    }

    #[test]
    fn wiki_topic_wraps_sttp_envelope() {
        let topic = resolve_topic(Some("recipe")).expect("recipe");
        let node = topic_sttp_node(topic);
        assert!(node.contains("environment_wiki_recipe"));
        assert!(node.contains("step_1_get(.99)"));
        assert!(node.contains("cognition_environment_get"));
    }

    #[tokio::test]
    async fn wiki_invoke_returns_sttp_recipe() {
        let tool = CognitionEnvironmentWikiTool;
        let out = tool
            .invoke(json!({ "topic": "recipe" }))
            .await
            .expect("invoke");
        assert_eq!(out["ok"], true);
        assert_eq!(out["format"], "sttp");
        assert_eq!(out["topic"], "recipe");
        let sttp = out["sttp_node"].as_str().expect("sttp_node");
        assert!(sttp.contains("◈⟨"));
        assert!(sttp.contains("cognition_environment_get"));
    }

    #[tokio::test]
    async fn wiki_unknown_topic_lists_ids() {
        let tool = CognitionEnvironmentWikiTool;
        let out = tool
            .invoke(json!({ "topic": "nope" }))
            .await
            .expect("invoke");
        assert_eq!(out["ok"], false);
        assert_eq!(out["format"], "sttp");
        assert!(out["all_topics"].as_array().unwrap().len() > 5);
    }
}
