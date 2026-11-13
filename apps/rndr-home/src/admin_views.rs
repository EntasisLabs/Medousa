//! Shared searchable list/detail presentation for sanitized existing service models.
use crate::admin_model::{Desktop, ReadState, Resource, Route};
use crate::desktop::{Connection, Handles, nav_button};
use crate::presentation::label;
use crate::shell::{muted, row};
use rndr_core::*;

pub fn render(world: &mut World, h: &Handles, route: Route, narrow: bool) {
    crate::settings_views::render(world, h, route, narrow);
}
pub fn dataset(world: &mut World, parent: Entity, resource: Resource) {
    let (state, query, selected) = {
        let d = world.resource::<Desktop>().unwrap();
        (
            d.reads
                .get(&resource)
                .cloned()
                .unwrap_or(ReadState::NotLoaded),
            d.query.clone(),
            d.selected,
        )
    };
    let connected = world.resource::<Connection>().unwrap().sender.is_some();
    let header = row(world, parent, Size::Px(34.));
    label(
        world,
        header,
        if resource == Resource::Activity {
            "Jobs snapshot"
        } else {
            resource.title()
        },
        true,
    );
    let refresh = nav_button(world, header, "admin:refresh", "Refresh read");
    world.get_mut::<Layout>(refresh).unwrap().width = Size::Px(112.);
    world.get_mut::<Button>(refresh).unwrap().enabled =
        connected && !matches!(state, ReadState::Loading(_));
    muted(
        world,
        parent,
        if resource == Resource::Activity {
            "Read-only snapshot · refresh explicitly · maximum 100 work cards · no operations executed"
        } else {
            "Read-only snapshot · refresh explicitly · maximum 500 entries · no operations executed"
        },
    );
    match state {
        ReadState::NotLoaded => {
            muted(
                world,
                parent,
                if connected {
                    "Not loaded. Refresh to request this service's status."
                } else {
                    "Offline / not connected. No live data or fixture results loaded. Restart with an explicit local data directory to read the daemon."
                },
            );
        }
        ReadState::Loading(_) => {
            muted(
                world,
                parent,
                "Loading… previous results hidden until this read settles. Each service read has a 12-second deadline.",
            );
        }
        ReadState::Error(error) => {
            muted(world, parent, error.message());
        }
        ReadState::Ready(rows) => {
            let source = if world
                .resource::<crate::desktop_fixtures::SyntheticDesktop>()
                .is_some()
            {
                "FIXTURE · synthetic snapshot · no network"
            } else if connected {
                "Live read snapshot · configured daemon · not continuous liveness"
            } else {
                "Retained snapshot · transport offline · not proof of current state"
            };
            muted(world, parent, source);
            if let Some(record) = selected.and_then(|i| rows.get(i)) {
                nav_button(world, parent, "admin:back", "‹ Back to results");
                label(world, parent, &record.title, true);
                for (key, value) in &record.fields {
                    label(world, parent, key, true);
                    muted(world, parent, value);
                }
                muted(
                    world,
                    parent,
                    "Details are from the last completed read, not a live subscription. Effectful controls remain in the existing Medousa app.",
                );
            } else {
                let filtered: Vec<_> = rows
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| r.matches(&query))
                    .collect();
                muted(
                    world,
                    parent,
                    &format!("{} of {} entries", filtered.len(), rows.len()),
                );
                if rows.is_empty() {
                    muted(
                        world,
                        parent,
                        "No entries returned by this service. This is a successful empty result, not a disconnected state.",
                    );
                } else if filtered.is_empty() {
                    muted(
                        world,
                        parent,
                        "No matching entries. Clear or edit the local filter; no server request is made while typing.",
                    );
                }
                for (index, record) in filtered {
                    let title = if record.title.chars().count() > 64 {
                        format!("{}…", record.title.chars().take(64).collect::<String>())
                    } else {
                        record.title.clone()
                    };
                    let entry = nav_button(world, parent, &format!("record:{index}"), &title);
                    world.insert(
                        entry,
                        Semantics {
                            name: Some(format!("Open snapshot details: {}", record.title)),
                            ..Default::default()
                        },
                    );
                    if let Some((key, value)) = record.fields.first() {
                        muted(world, parent, &format!("{key}: {value}"));
                    }
                }
            }
        }
    }
}
