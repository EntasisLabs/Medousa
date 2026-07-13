//! Read-only custom view / environment status for doctor tool and HTTP API.

use medousa_types::environment::{
    ComponentType, CustomViewComponentStatus, CustomViewFeedStatus, CustomViewRecurringBindingStatus,
    CustomViewSurfaceStatus, EnvironmentSpec, EnvironmentStatusResponse, SurfaceKind,
};
use medousa_types::layout::resolve_layout_root;
use serde_json::Value;
use stasis::domain::runtime::recurring::RecurringDefinition;
use stasis::prelude::RuntimeComposition;
use stasis::ports::outbound::runtime::recurring_store::RecurringStore;

use crate::component_runtime_diagnostics::{
    build_component_runtime_diagnostic, RuntimeDiagnosticOptions,
};
use crate::environment_store::EnvironmentHub;
use crate::feed_store::feed_store;
use crate::recurring_feed::{self, feeds_binding_to_json, RecurringFeedBinding};

pub fn surface_nav_visible(spec: &EnvironmentSpec, surface_id: &str) -> bool {
    active_preset_surface_ids(spec)
        .iter()
        .any(|id| id == surface_id)
}

pub fn active_preset_surface_ids(spec: &EnvironmentSpec) -> Vec<String> {
    if let Some(presets) = &spec.layout_presets {
        if let Some(active) = presets.iter().find(|preset| preset.active) {
            return active.surfaces.clone();
        }
        if let Some(id) = spec.active_preset_id.as_deref()
            && let Some(preset) = presets.iter().find(|preset| preset.id == id) {
                return preset.surfaces.clone();
            }
    }
    spec.surfaces.iter().map(|surface| surface.id.clone()).collect()
}

fn presentation_artifact_id(config: &Value) -> Option<String> {
    config
        .get("artifactId")
        .or_else(|| config.get("artifact_id"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

async fn list_recurring_definitions(
    runtime: &RuntimeComposition,
) -> stasis::prelude::Result<Vec<RecurringDefinition>> {
    match runtime {
        RuntimeComposition::InMemory(rt) => rt.recurring_store.list().await,
        RuntimeComposition::Surreal(rt) => rt.recurring_store.list().await,
    }
}

async fn collect_recurring_feed_bindings(
    runtime: Option<&RuntimeComposition>,
) -> Vec<(String, RecurringFeedBinding, Option<String>, Option<bool>)> {
    let Some(runtime) = runtime else {
        return Vec::new();
    };
    let Ok(definitions) = list_recurring_definitions(runtime).await else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    for definition in definitions {
        if let Some(binding) = recurring_feed::feed_binding_for_recurring(&definition.id).await {
            rows.push((
                definition.id.clone(),
                binding,
                Some(definition.cron_expr.clone()),
                Some(definition.enabled),
            ));
        }
    }
    rows
}

fn feed_mismatches_for_surface(
    subscribed: &[String],
    recurring_rows: &[(String, RecurringFeedBinding, Option<String>, Option<bool>)],
) -> Vec<String> {
    let mut mismatches = Vec::new();

    let relevant: Vec<_> = recurring_rows
        .iter()
        .filter(|(_, _, _, enabled)| enabled != &Some(false))
        .filter(|(_, binding, _, _)| {
            binding
                .feed_ids
                .iter()
                .any(|feed_id| subscribed.contains(feed_id))
        })
        .collect();

    let mut recurring_feed_ids = std::collections::HashSet::new();
    for (_, binding, _, _) in &relevant {
        for feed_id in &binding.feed_ids {
            recurring_feed_ids.insert(feed_id.clone());
        }
    }

    for feed_id in subscribed {
        if !recurring_feed_ids.contains(feed_id) {
            mismatches.push(format!(
                "component subscribes to '{feed_id}' but no recurring job binds that feed"
            ));
        }
    }

    for (recurring_id, binding, _, _) in &relevant {
        for feed_id in &binding.feed_ids {
            if !subscribed.contains(feed_id) {
                mismatches.push(format!(
                    "recurring '{recurring_id}' binds '{feed_id}' but this surface's components do not subscribe"
                ));
            }
        }
    }

    mismatches.sort();
    mismatches.dedup();
    mismatches
}

#[derive(Debug, Clone, Default)]
pub struct DoctorDiagnosticOptions {
    pub component_id_filter: Option<String>,
    pub include_runtime: bool,
    pub include_static_lint: bool,
    pub probe: bool,
    pub session_id: Option<String>,
}

pub async fn build_environment_status(
    hub: &EnvironmentHub,
    profile_id: &str,
    surface_filter: Option<&str>,
    runtime: Option<&RuntimeComposition>,
    diagnostics: Option<&DoctorDiagnosticOptions>,
) -> anyhow::Result<EnvironmentStatusResponse> {
    let record = hub.get(profile_id).await?;
    let spec = &record.spec;
    let pending = hub.pending(profile_id).await.is_some();
    let recurring_rows = collect_recurring_feed_bindings(runtime).await;

    let mut custom_surfaces = Vec::new();
    let mut feed_mismatch_count = 0usize;
    let mut nav_orphan_count = 0usize;

    for surface in spec
        .surfaces
        .iter()
        .filter(|surface| surface.kind == SurfaceKind::Custom)
    {
        if let Some(filter) = surface_filter
            && surface.id != filter {
                continue;
            }

        let nav_visible = surface_nav_visible(spec, &surface.id);
        if !nav_visible {
            nav_orphan_count += 1;
        }

        let mut components: Vec<CustomViewComponentStatus> = Vec::new();
        for component in spec
            .components
            .iter()
            .filter(|component| component.surface_id == surface.id)
        {
            if let Some(filter) = diagnostics.and_then(|opts| opts.component_id_filter.as_deref())
                && component.id != filter {
                    continue;
                }
            let mut runtime_diag = None;
            if let Some(opts) = diagnostics {
                let wants_runtime = opts.include_runtime || opts.include_static_lint || opts.probe;
                if wants_runtime && component.component_type == ComponentType::Presentation {
                    let should_probe = opts.probe
                        && opts
                            .component_id_filter
                            .as_deref()
                            .is_none_or(|id| id == component.id);
                    runtime_diag = Some(
                        build_component_runtime_diagnostic(
                            component,
                            &RuntimeDiagnosticOptions {
                                profile_id: profile_id.to_string(),
                                include_runtime: opts.include_runtime,
                                include_static_lint: opts.include_static_lint,
                                probe: should_probe,
                                session_id: opts.session_id.clone(),
                            },
                        )
                        .await,
                    );
                }
            }
            components.push(CustomViewComponentStatus {
                component_id: component.id.clone(),
                artifact_id: presentation_artifact_id(&component.config),
                feeds: component.feeds.clone(),
                runtime: runtime_diag,
            });
        }

        let mut subscribed_feed_ids = Vec::new();
        for component in &components {
            for feed_id in &component.feeds {
                if !subscribed_feed_ids.contains(feed_id) {
                    subscribed_feed_ids.push(feed_id.clone());
                }
            }
        }

        let mut feed_status = Vec::new();
        for feed_id in &subscribed_feed_ids {
            let events = feed_store().tail(profile_id, feed_id, 1).await;
            let last = events.last();
            feed_status.push(CustomViewFeedStatus {
                feed_id: feed_id.clone(),
                last_emitted_at_utc: last.map(|event| event.emitted_at_utc.to_rfc3339()),
                last_summary: last.map(|event| event.summary.clone()),
            });
        }

        let surface_recurring: Vec<CustomViewRecurringBindingStatus> = recurring_rows
            .iter()
            .filter(|(_, binding, _, _)| {
                binding
                    .feed_ids
                    .iter()
                    .any(|feed_id| subscribed_feed_ids.contains(feed_id))
            })
            .map(|(recurring_id, binding, cron_expr, enabled)| {
                CustomViewRecurringBindingStatus {
                    recurring_id: recurring_id.clone(),
                    feed_ids: binding.feed_ids.clone(),
                    cron_expr: cron_expr.clone(),
                    enabled: *enabled,
                }
            })
            .collect();

        let feed_mismatches =
            feed_mismatches_for_surface(&subscribed_feed_ids, &recurring_rows);
        feed_mismatch_count += feed_mismatches.len();

        custom_surfaces.push(CustomViewSurfaceStatus {
            surface_id: surface.id.clone(),
            label: surface.label.clone(),
            nav_visible,
            components,
            subscribed_feed_ids,
            feed_status,
            feed_mismatches,
            recurring_bindings: surface_recurring,
            layout_root: Some(resolve_layout_root(surface, &spec.components)),
        });
    }

    let hints = build_hints(nav_orphan_count, feed_mismatch_count, pending, diagnostics);

    Ok(EnvironmentStatusResponse {
        profile_id: profile_id.to_string(),
        revision: record.revision,
        active_preset_id: spec.active_preset_id.clone(),
        pending_proposal: pending,
        custom_surfaces,
        feed_mismatch_count,
        nav_orphan_count,
        hints,
    })
}

fn build_hints(
    nav_orphan_count: usize,
    feed_mismatch_count: usize,
    pending: bool,
    diagnostics: Option<&DoctorDiagnosticOptions>,
) -> Vec<String> {
    let mut hints = Vec::new();
    if nav_orphan_count > 0 {
        hints.push(
            "Some custom surfaces are not in the active preset — use cognition_environment_patch add_to_active_preset or cognition_custom_view_compose."
                .to_string(),
        );
    }
    if feed_mismatch_count > 0 {
        hints.push(
            "Feed subscribe and recurring feed bindings are mismatched — run cognition_custom_view_doctor and align feed_ids on component + recurring register."
                .to_string(),
        );
    }
    if pending {
        hints.push(
            "A layout proposal is pending operator approval in Settings → Canvas."
                .to_string(),
        );
    }
    if diagnostics.is_some_and(|opts| opts.include_runtime || opts.include_static_lint) {
        hints.push(
            "Widget runtime issues appear under components[].runtime — presentation fixes via cognition_artifact_write; media_embed has no artifact lint."
                .to_string(),
        );
    }
    hints
}

pub fn nav_visibility_fields(
    _spec: &EnvironmentSpec,
    surface_id: &str,
    nav_visible: bool,
) -> serde_json::Value {
    if nav_visible {
        serde_json::json!({
            "live": true,
            "nav_visible": true,
        })
    } else {
        serde_json::json!({
            "live": true,
            "nav_visible": false,
            "hint": format!(
                "Surface '{surface_id}' is not in the active layout preset — call cognition_environment_patch with add_to_active_preset or cognition_custom_view_compose."
            ),
        })
    }
}

pub fn recurring_feed_binding_json(binding: &RecurringFeedBinding) -> Value {
    feeds_binding_to_json(binding)
}
