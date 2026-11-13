//! Explicitly synthetic GPU evidence. Never launched with live workers/credentials.
use crate::admin_model::{Desktop, ReadError, ReadState, Record, Resource, Route};
use crate::desktop::{Connection, Handles};
use crate::{presentation, state::Model};
use rndr_core::*;
use rndr_ui::{Appearance, Contrast, UiTheme};
use std::{path::PathBuf, sync::Arc};

pub struct SyntheticDesktop;
pub fn export(app: &mut App, dir: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    assert!(app.world().resource::<Model>().unwrap().commands.is_none());
    assert!(
        app.world()
            .resource::<Connection>()
            .unwrap()
            .sender
            .is_none()
    );
    std::fs::create_dir_all(&dir)?;
    app.world_mut().insert_resource(SyntheticDesktop);
    app.world_mut()
        .insert_resource(Arc::new(rndr_render::ParleyText::new()) as Arc<dyn TextEngine>);
    app.update();
    let mut exporter = rndr_render::Exporter::new()?;
    for (theme_name, appearance, contrast) in [
        ("dark", Appearance::Dark, Contrast::Standard),
        ("light", Appearance::Light, Contrast::Standard),
        ("high-contrast", Appearance::Dark, Contrast::High),
    ] {
        presentation::apply_theme(
            app.world_mut(),
            UiTheme {
                appearance,
                contrast,
                ..Default::default()
            },
        );
        for (width_name, width, height) in [
            ("wide", 1120., 820.),
            ("narrow", 480., 760.),
            ("wide-650", 1120., 650.),
            ("narrow-650", 480., 650.),
        ] {
            app.world_mut()
                .insert_resource(Viewport::new(width, height));
            for (name, route, resource, state, detail) in cases() {
                let d = app.world_mut().resource_mut::<Desktop>().unwrap();
                d.reads.clear();
                if let Some(resource) = resource {
                    d.reads.insert(resource, state);
                }
                crate::desktop::navigate(app.world_mut(), route);
                app.world_mut().resource_mut::<Desktop>().unwrap().selected = detail;
                for _ in 0..8 {
                    app.update();
                }
                let h = app.world().resource::<Handles>().unwrap().clone();
                // Always visibly distinguish these images from real connected UI.
                app.world_mut().get_mut::<Label>(h.title).unwrap().text =
                    format!("FIXTURE · {}", route.title());
                app.world_mut().get_mut::<Label>(h.footer).unwrap().text =
                    "SYNTHETIC GPU RENDER · NO NETWORK · NOT NATIVE WINDOW / INPUT PROOF".into();
                if name == "settings-chooser" {
                    crate::desktop::action(app.world_mut(), "settings:choose");
                    for _ in 0..8 {
                        app.update();
                    }
                }
                let frame = rndr_paint::frame(app);
                let image = exporter.render_frame(
                    Some(app.world()),
                    &frame,
                    width as u32,
                    height as u32,
                    1.,
                )?;
                std::fs::write(
                    dir.join(format!("{theme_name}-{width_name}-{name}.png")),
                    rndr_render::encode_png(&image)?,
                )?;
            }
        }
    }
    Ok(())
}
type Case = (
    &'static str,
    Route,
    Option<Resource>,
    ReadState,
    Option<usize>,
);
fn cases() -> Vec<Case> {
    let mut cases: Vec<Case> = (0..9)
        .map(|i| {
            (
                crate::admin_model::SETTINGS[i].0,
                Route::Settings(i),
                None,
                ReadState::NotLoaded,
                None,
            )
        })
        .collect();
    for (name, state, detail) in [
        (
            "runtime-list",
            ReadState::Ready(vec![
                Record::new(
                    "Synthetic Codex",
                    vec![
                        ("Available", "Yes (fixture)".into()),
                        ("Account status", "unknown (fixture)".into()),
                    ],
                ),
                Record::new(
                    "Synthetic Cursor",
                    vec![
                        ("Available", "No (fixture)".into()),
                        ("Binary present", "No (fixture)".into()),
                    ],
                ),
            ]),
            None,
        ),
        (
            "runtime-detail",
            ReadState::Ready(vec![Record::new(
                "Synthetic Codex",
                vec![
                    ("Available", "Yes (fixture)".into()),
                    ("Account status", "unknown (fixture)".into()),
                    (
                        "Qualification",
                        "No login, execution or inference performed".into(),
                    ),
                ],
            )]),
            Some(0),
        ),
        ("runtime-empty", ReadState::Ready(vec![]), None),
        ("runtime-loading", ReadState::Loading(1), None),
        (
            "runtime-permission",
            ReadState::Error(ReadError::Permission),
            None,
        ),
        (
            "runtime-error",
            ReadState::Error(ReadError::Unavailable),
            None,
        ),
    ] {
        cases.push((
            name,
            Route::Data(Resource::Runtimes),
            Some(Resource::Runtimes),
            state,
            detail,
        ));
    }
    cases.push((
        "mcp-disconnected",
        Route::Data(Resource::Mcp),
        Some(Resource::Mcp),
        ReadState::Ready(vec![Record::new(
            "Synthetic disconnected server",
            vec![
                ("Connected", "No (fixture)".into()),
                ("Enabled", "Yes (fixture)".into()),
                ("Tools", "0".into()),
            ],
        )]),
        Some(0),
    ));
    cases.push((
        "models",
        Route::Data(Resource::Models),
        Some(Resource::Models),
        ReadState::Ready(vec![Record::new(
            "Synthetic local model",
            vec![
                ("Provider", "Local (fixture)".into()),
                ("Availability", "Catalog only; inference untested".into()),
            ],
        )]),
        None,
    ));
    cases.push((
        "automations",
        Route::Data(Resource::Schedules),
        Some(Resource::Schedules),
        ReadState::Ready(vec![Record::new(
            "Synthetic weekly report",
            vec![
                ("Enabled", "No (fixture)".into()),
                ("Schedule", "0 9 * * MON".into()),
                ("Timezone", "UTC".into()),
            ],
        )]),
        Some(0),
    ));
    cases.push((
        "settings-chooser",
        Route::Settings(3),
        None,
        ReadState::NotLoaded,
        None,
    ));
    let work = Record::new(
        "Work item 1 · In flight",
        vec![
            ("Board state", "In flight (fixture)".into()),
            ("Card ID", "job-synthetic-001".into()),
            ("Created UTC", "2026-11-13T00:00:00Z".into()),
            ("Updated UTC", "2026-11-13T01:00:00Z".into()),
            ("Snapshot revision", "42 (fixture)".into()),
            (
                "Privacy",
                "Prompt-derived titles, status prose, outputs and diagnostics withheld".into(),
            ),
        ],
    );
    for (name, state, detail) in [
        (
            "activity-list",
            ReadState::Ready(vec![
                work.clone(),
                Record::new(
                    "Work item 2 · Blocked",
                    vec![("Board state", "Blocked (fixture)".into())],
                ),
            ]),
            None,
        ),
        ("activity-detail", ReadState::Ready(vec![work]), Some(0)),
        ("activity-empty", ReadState::Ready(vec![]), None),
        ("activity-offline", ReadState::NotLoaded, None),
        ("activity-loading", ReadState::Loading(1), None),
        (
            "activity-error",
            ReadState::Error(ReadError::Unavailable),
            None,
        ),
        (
            "activity-permission",
            ReadState::Error(ReadError::Permission),
            None,
        ),
    ] {
        cases.push((
            name,
            Route::Data(Resource::Activity),
            Some(Resource::Activity),
            state,
            detail,
        ));
    }
    cases
}
