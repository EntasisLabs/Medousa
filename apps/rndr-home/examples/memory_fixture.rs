//! Offline-only conversation scaling harness. No daemon, credentials or inference.
//! cargo run --manifest-path apps/rndr-home/Cargo.toml --example memory_fixture -- 1000 --native
#[path = "../src/conversation.rs"]
#[allow(dead_code)]
mod conversation;
use rndr_core::*;
use std::{sync::Arc, time::Instant};

struct Fixture {
    root: Entity,
    messages: Vec<(String, String)>,
}
#[derive(Clone)]
struct Report;
impl Event for Report {}
#[derive(Clone)]
struct Exit;
impl Event for Exit {}
fn update(world: &mut World) {
    // Move the vector, never its strings, across the ECS mutation boundary.
    let fixture = world
        .resource_mut::<Option<Fixture>>()
        .unwrap()
        .take()
        .unwrap();
    conversation::sync_iter(
        world,
        fixture.root,
        fixture
            .messages
            .iter()
            .map(|(r, t)| (r.as_str(), t.as_str())),
    );
    if !events::<Report>(world).is_empty() {
        report(world, &fixture);
    }
    if !events::<Exit>(world).is_empty() {
        world.resource_mut::<Input>().unwrap().quit = true;
    }
    *world.resource_mut::<Option<Fixture>>().unwrap() = Some(fixture);
}
fn report(world: &World, fixture: &Fixture) {
    eprintln!(
        "memory-fixture messages={} source_bytes={} layouts={} markdown_views={}",
        fixture.messages.len(),
        fixture.messages.iter().map(|m| m.1.len()).sum::<usize>(),
        world.entities_with::<Layout>().len(),
        world.entities_with::<MarkdownView>().len()
    );
}
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let count = args
        .first()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(1000)
        .min(10000);
    let native = args.iter().any(|s| s == "--native");
    let mut app = App::new();
    app.add_plugin(rndr_ui::UiPlugin);
    app.add_plugin(MarkdownPlugin);
    rndr_ui::apply_theme_bridge(
        app.world_mut(),
        rndr_ui::UiTheme {
            appearance: rndr_ui::Appearance::Dark,
            ..Default::default()
        },
    );
    app.world_mut().insert_resource(WindowConfig {
        title: "Medousa · synthetic memory fixture · no network".into(),
        size: LogicalSize::new(1120.0, 820.0),
        ..Default::default()
    });
    app.world_mut()
        .insert_resource(Viewport::new(1120.0, 820.0));
    app.world_mut()
        .insert_resource(Arc::new(rndr_render::ParleyText::new()) as Arc<dyn TextEngine>);
    let parent = app.world_mut().spawn((Layout {
        width: Size::Px(1120.0),
        height: Size::Px(820.0),
        ..Layout::column()
    },));
    let root = conversation::setup(app.world_mut(), parent);
    let paragraph = "A synthetic paragraph with **bold text**, `inline code`, and ordinary prose for measuring chat history.\n\n";
    let messages = (0..count)
        .map(|i| {
            (
                "assistant".into(),
                format!("Message {i}\n\n{}", paragraph.repeat(10)),
            )
        })
        .collect();
    app.world_mut()
        .insert_resource(Some(Fixture { root, messages }));
    app.add_system(Schedule::Update, update);
    if native {
        let _report = after(app.world_mut(), std::time::Duration::from_secs(3), Report);
        let _exit = after(app.world_mut(), std::time::Duration::from_secs(15), Exit);
        rndr_render::run(app);
    } else {
        let start = Instant::now();
        for _ in 0..12 {
            app.update();
        }
        report(
            app.world(),
            app.world()
                .resource::<Option<Fixture>>()
                .unwrap()
                .as_ref()
                .unwrap(),
        );
        eprintln!(
            "headless_12_updates_ms={:.2}",
            start.elapsed().as_secs_f64() * 1000.0
        );
    }
}
