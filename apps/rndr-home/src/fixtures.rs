//! Synthetic exports and deny-by-default offline fixture observation only.
use crate::composer::reset_prompt;
use crate::{menus, shell::Ui, state::Model};
use rndr_core::*;
use std::{path::PathBuf, sync::Arc};

#[cfg(all(test, feature = "observe"))]
#[path = "observation_tests.rs"]
mod observation_tests;

#[cfg(feature = "observe")]
pub(super) struct FixtureObservation;

pub(super) fn export(app: &mut App, dir: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(&dir)?;
    app.world_mut()
        .insert_resource(Arc::new(rndr_render::ParleyText::new()) as Arc<dyn TextEngine>);
    let mut exporter = rndr_render::Exporter::new()?;
    for (name, width, height, transcript) in [
        ("empty", 1120., 820., false),
        ("chat", 1120., 820., true),
        ("narrow", 520., 760., true),
        ("draft", 1120., 820., true),
        ("models", 1120., 820., true),
        ("behavior", 1120., 820., true),
        ("appearance", 1120., 820., true),
        ("depth", 1120., 820., true),
        ("runtime", 1120., 820., true),
        ("history-away", 1120., 820., true),
        ("history-away-narrow", 480., 760., true),
        ("history-latest", 480., 760., true),
    ] {
        app.world_mut()
            .insert_resource(Viewport::new(width, height));
        app.update();
        menus::close(app.world_mut());
        let ui = *app.world().resource::<Ui>().unwrap();
        reset_prompt(app.world_mut(), ui.composer, "");
        {
            let m = app.world_mut().resource_mut::<Model>().unwrap();
            m.models = vec![
                ("ChatGPT · synthetic".into(), "GPT-6.1 Sol".into()),
                ("Anthropic · synthetic".into(), "Claude · example".into()),
                ("Local · synthetic".into(), "Local model · example".into()),
            ];
            m.selection = Some(0);
        }
        if transcript {
            app.world_mut().resource_mut::<Model>().unwrap().history_messages = vec![
                    ("user".into(), "Can we make this feel more like the current chat?".into()),
                    ("assistant".into(), "Yes — a conversation, not a document.\n\nThe composer now **grows with your draft**, and the model button opens a searchable menu instead of cycling.\n\n```sh\ncargo run --manifest-path apps/rndr-home/Cargo.toml -- --offline\n```\n\nMode changes use the same daemon contract as the Svelte app. Behavior supports next-turn response depth; external runtimes and the remaining controls are explicitly marked as not ported.\n\nThis is synthetic offline visual evidence. **No inference was sent.**".into()),
                ];
        }
        if name.starts_with("history-") {
            app.world_mut().resource_mut::<Model>().unwrap().history_messages = (0..40)
                .map(|i| ("assistant".into(), format!("Synthetic message {i}\n\nRead earlier history without losing your place. New replies do not pull you away.\n\n{}", "A bounded offline transcript paragraph. ".repeat(8))))
                .collect();
            reset_prompt(
                app.world_mut(),
                ui.composer,
                "Unsent synthetic draft · preserved",
            );
        }
        if name == "draft" {
            reset_prompt(
                app.world_mut(),
                ui.composer,
                "A longer draft should grow naturally.\nShift+Enter adds another line.\nThe transcript keeps the remaining space.\nAnd the editor is capped at six visual lines.\nFive\nSix\nSeven — internally scrollable",
            );
        }
        for _ in 0..5 {
            app.update();
        }
        match name {
            "models" => menus::open(app.world_mut(), menus::Kind::Model, ui.model),
            "behavior" => menus::open(app.world_mut(), menus::Kind::Behavior, ui.behavior),
            "appearance" => menus::open(app.world_mut(), menus::Kind::Appearance, ui.behavior),
            "runtime" => menus::open(app.world_mut(), menus::Kind::Runtime, ui.runtime),
            "depth" => menus::open(app.world_mut(), menus::Kind::Depth, ui.behavior),
            _ => {}
        }
        for _ in 0..4 {
            app.update();
        }
        if name.starts_with("history-") {
            app.world_mut()
                .get_mut::<Scroll>(ui.conversation)
                .unwrap()
                .offset[1] = 0.;
            for _ in 0..8 {
                app.update();
            }
            if name == "history-latest" {
                assert!(crate::conversation::jump_to_latest(
                    app.world_mut(),
                    ui.conversation
                ));
                for _ in 0..8 {
                    app.update();
                }
            }
            assert_eq!(
                app.world().get::<Label>(ui.composer).unwrap().text,
                "Unsent synthetic draft · preserved"
            );
            assert!(app.world().resource::<Model>().unwrap().commands.is_none());
        }
        let frame = rndr_paint::frame(app);
        let image =
            exporter.render_frame(Some(app.world()), &frame, width as u32, height as u32, 1.)?;
        std::fs::write(
            dir.join(format!("{name}.png")),
            rndr_render::encode_png(&image)?,
        )?;
    }
    Ok(())
}

#[cfg(feature = "observe")]
pub(super) fn sync_observation(world: &mut World) {
    if world.has_resource::<FixtureObservation>() {
        let ui = *world.resource::<Ui>().unwrap();
        use rndr_render::observe::ObservationContent;
        // Explicit fixture opt-in must not expose text someone types into the
        // native window while a read-only observer is attached.
        let m = world.resource::<Model>().unwrap();
        let public = crate::composer::native_input_ready(world)
            && m.commands.is_none()
            && m.history_messages.is_empty()
            && m.chat.live_messages.is_empty()
            && m.recovery.is_empty()
            && world.get::<Label>(ui.composer).unwrap().text.is_empty();
        world.insert_resource(if public {
            ObservationContent::Public
        } else {
            ObservationContent::Deny
        });
    }
}
