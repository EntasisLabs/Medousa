//! Provider-first composer menu. Browsing never changes the active route.
use super::*;
use rndr_ui::menu::{MenuAt, MenuEntry, MenuHandle, MenuItem, MenuPicked, open_menu};

// Leave room for the heading, back, search and paging controls in RNDR's
// bounded menu. Each page scrolls inside the native popup viewport.
const PAGE_SIZE: usize = 240;
const BACK: u64 = u64::MAX;
const PREVIOUS: u64 = BACK - 1;
const NEXT: u64 = BACK - 2;
const SEARCH_MODELS: u64 = BACK - 3;

#[derive(Default)]
pub(super) struct State {
    menu: Option<MenuHandle>,
    provider: Option<String>,
    page: usize,
    choices: Vec<Choice>,
    // Fence indices against a catalog refresh/session switch while open.
    snapshot: String,
}
#[derive(Clone)]
enum Choice {
    Provider(String),
    Model(String, String),
}

pub(super) fn handle(world: &World) -> Option<MenuHandle> {
    world.resource::<State>().and_then(|s| s.menu)
}
fn query(world: &World) -> String {
    let ui = world.resource::<Ui>().unwrap();
    world
        .get::<Label>(ui.model_filter)
        .unwrap()
        .text
        .trim()
        .to_lowercase()
}
fn snapshot(world: &World) -> String {
    let m = world.resource::<Model>().unwrap();
    format!(
        "{:?}{:?}{:?}{}",
        m.chat.session(),
        m.models,
        m.selected,
        query(world)
    )
}
fn available(world: &World) -> bool {
    let m = world.resource::<Model>().unwrap();
    crate::surfaces::active(world) == crate::surfaces::SurfaceId::Chat
        && !m.loading
        && !m.chat.busy()
        && !runtimes::busy(world)
        && matches!(runtimes::active(world), runtimes::ActiveRuntime::Native)
}
fn provider_label(provider: &str) -> String {
    match provider {
        "openai-codex" => "ChatGPT".into(),
        "openai" => "OpenAI".into(),
        "openrouter" => "OpenRouter".into(),
        "anthropic" => "Anthropic".into(),
        "google" => "Google".into(),
        "ollama" => "Ollama".into(),
        _ => provider.into(),
    }
}
fn providers(models: &[(String, String)], query: &str) -> Vec<String> {
    let mut providers = models
        .iter()
        .filter(|(p, n)| {
            format!("{p} {} {n}", provider_label(p))
                .to_lowercase()
                .contains(query)
        })
        .map(|(p, _)| p.clone())
        .collect::<Vec<_>>();
    providers.sort();
    providers.dedup();
    providers
}

/// Opening the trigger always starts at providers, even after a prior pick.
pub(super) fn open(world: &mut World) {
    if !available(world) {
        return;
    }
    let state = world.resource_mut::<State>().unwrap();
    state.provider = None;
    state.page = 0;
    show(world);
}
fn show(world: &mut World) {
    let m = world.resource::<Model>().unwrap();
    let state = world.resource::<State>().unwrap();
    let provider = state.provider.clone();
    let q = query(world);
    let choices = if let Some(provider) = &provider {
        m.models
            .iter()
            .filter(|(p, n)| {
                p == provider
                    && format!("{p} {} {n}", provider_label(p))
                        .to_lowercase()
                        .contains(&q)
            })
            .map(|(p, n)| Choice::Model(p.clone(), n.clone()))
            .collect::<Vec<_>>()
    } else {
        providers(&m.models, &q)
            .into_iter()
            .map(Choice::Provider)
            .collect()
    };
    let pages = choices.len().div_ceil(PAGE_SIZE).max(1);
    let page = state.page.min(pages - 1);
    let mut entries = vec![MenuEntry::Group(
        provider
            .as_deref()
            .map(provider_label)
            .unwrap_or_else(|| "Choose provider".into()),
    )];
    if provider.is_some() {
        entries.push(MenuItem::new(BACK, "‹ All providers").into());
    }
    entries.push(
        MenuItem::new(
            SEARCH_MODELS,
            if q.is_empty() {
                "Search models…".into()
            } else {
                format!("Search: {q}")
            },
        )
        .into(),
    );
    entries.push(MenuEntry::Separator);
    for (i, choice) in choices
        .iter()
        .enumerate()
        .skip(page * PAGE_SIZE)
        .take(PAGE_SIZE)
    {
        let (label, checked) = match choice {
            Choice::Provider(p) => (
                format!("{}  ›", provider_label(p)),
                m.selected
                    .and_then(|i| m.models.get(i))
                    .is_some_and(|(active, _)| active == p),
            ),
            Choice::Model(p, n) => (
                model_display_name(n),
                m.selected.and_then(|i| m.models.get(i)) == Some(&(p.clone(), n.clone())),
            ),
        };
        entries.push(MenuItem::new(i as u64, label).checked(checked).into());
    }
    if choices.is_empty() {
        entries.push(
            MenuItem::new(
                0,
                if q.is_empty() {
                    "No models available"
                } else {
                    "No matching models"
                },
            )
            .enabled(false)
            .into(),
        );
    }
    if pages > 1 {
        entries.push(MenuEntry::Separator);
        entries.push(MenuEntry::Group(format!("Page {} of {pages}", page + 1)));
        entries.push(
            MenuItem::new(PREVIOUS, "Previous page")
                .enabled(page > 0)
                .into(),
        );
        entries.push(
            MenuItem::new(NEXT, "Next page")
                .enabled(page + 1 < pages)
                .into(),
        );
    }
    let anchor = world.resource::<Ui>().unwrap().model;
    let result = open_menu(world, anchor, MenuAt::Below, &entries);
    let snapshot = snapshot(world);
    let state = world.resource_mut::<State>().unwrap();
    state.choices = choices;
    state.page = page;
    state.snapshot = snapshot;
    state.menu = result.ok();
}

pub(super) fn update(world: &mut World) {
    let Some(menu) = handle(world) else {
        return;
    };
    // RNDR has already closed the picked menu by Update; inspect events first.
    for event in events::<MenuPicked>(world).to_vec() {
        if event.menu != menu.root {
            continue;
        }
        if !available(world) || world.resource::<State>().unwrap().snapshot != snapshot(world) {
            menu.close(world);
            world.resource_mut::<State>().unwrap().menu = None;
            return;
        }
        match event.item {
            BACK => {
                let s = world.resource_mut::<State>().unwrap();
                s.provider = None;
                s.page = 0;
                show(world);
            }
            PREVIOUS | NEXT => {
                let s = world.resource_mut::<State>().unwrap();
                s.page = if event.item == NEXT {
                    s.page + 1
                } else {
                    s.page.saturating_sub(1)
                };
                show(world);
            }
            SEARCH_MODELS => {
                let ui = *world.resource::<Ui>().unwrap();
                world.resource_mut::<Model>().unwrap().model_search_open = true;
                world.remove::<Hidden>(ui.filter_row);
                request_focus(world, ui.model_filter);
                world.resource_mut::<State>().unwrap().menu = None;
            }
            id => {
                let choice = world
                    .resource::<State>()
                    .unwrap()
                    .choices
                    .get(id as usize)
                    .cloned();
                match choice {
                    Some(Choice::Provider(provider)) => {
                        let s = world.resource_mut::<State>().unwrap();
                        s.provider = Some(provider);
                        s.page = 0;
                        show(world);
                    }
                    Some(Choice::Model(provider, model)) => {
                        let m = world.resource_mut::<Model>().unwrap();
                        m.model_chosen = true;
                        m.selected = m
                            .models
                            .iter()
                            .position(|pair| pair == &(provider.clone(), model.clone()));
                        world.resource_mut::<State>().unwrap().menu = None;
                    }
                    None => {}
                }
            }
        }
        return;
    }
    if !menu.is_open(world) {
        world.resource_mut::<State>().unwrap().menu = None;
    }
}

pub(super) fn sync(world: &mut World, enabled: bool) {
    if let Some(menu) = handle(world)
        && (!enabled
            || !available(world)
            || world.resource::<State>().unwrap().snapshot != snapshot(world))
    {
        menu.close(world);
        world.resource_mut::<State>().unwrap().menu = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(models: Vec<(String, String)>) -> App {
        let mut app = app();
        app.world_mut().insert_resource(Viewport::new(1200., 850.));
        app.update();
        let m = app.world_mut().resource_mut::<Model>().unwrap();
        m.chat
            .hydrate("s".into(), "a".into(), vec![], None)
            .unwrap();
        m.models = models;
        m.selected = Some(0);
        app.update();
        app.update();
        app
    }
    fn pick(app: &mut App, id: u64) {
        let menu = handle(app.world()).expect("open menu");
        let item = menu.item(app.world(), id).expect("menu item");
        assert_eq!(
            try_activate(app.world_mut(), item),
            ActivationResult::Accepted
        );
        app.update();
        app.update();
    }
    fn two_providers() -> Vec<(String, String)> {
        vec![
            ("openai-codex".into(), "shared-name".into()),
            ("openrouter".into(), "shared-name".into()),
            ("openrouter".into(), "other".into()),
        ]
    }
    #[test]
    fn provider_navigation_is_nonmutating_and_models_are_scoped() {
        let mut app = fixture(two_providers());
        open(app.world_mut());
        assert_eq!(app.world().resource::<State>().unwrap().choices.len(), 2);
        pick(&mut app, 1);
        let state = app.world().resource::<State>().unwrap();
        assert_eq!(state.provider.as_deref(), Some("openrouter"));
        assert_eq!(state.choices.len(), 2);
        assert!(
            state
                .choices
                .iter()
                .all(|c| matches!(c, Choice::Model(p, _) if p == "openrouter"))
        );
        assert_eq!(app.world().resource::<Model>().unwrap().selected, Some(0));
        pick(&mut app, BACK);
        assert_eq!(app.world().resource::<State>().unwrap().provider, None);
        pick(&mut app, 1);
        pick(&mut app, 0);
        assert_eq!(app.world().resource::<Model>().unwrap().selected, Some(1));
        open(app.world_mut());
        assert_eq!(app.world().resource::<State>().unwrap().provider, None);
    }
    #[test]
    fn every_model_in_a_large_provider_catalog_is_reachable() {
        let models = (0..600)
            .map(|i| ("fixture".into(), format!("model {i}")))
            .collect();
        let mut app = fixture(models);
        open(app.world_mut());
        pick(&mut app, 0);
        pick(&mut app, NEXT);
        assert_eq!(app.world().resource::<State>().unwrap().page, 1);
        pick(&mut app, NEXT);
        assert_eq!(app.world().resource::<State>().unwrap().page, 2);
        pick(&mut app, 599);
        assert_eq!(app.world().resource::<Model>().unwrap().selected, Some(599));
    }
    #[test]
    fn search_and_catalog_refresh_preserve_route_and_reject_stale_picks() {
        let mut app = fixture(two_providers());
        let ui = *app.world().resource::<Ui>().unwrap();
        app.world_mut()
            .get_mut::<Label>(ui.model_filter)
            .unwrap()
            .text = "other".into();
        open(app.world_mut());
        assert_eq!(app.world().resource::<State>().unwrap().choices.len(), 1);
        pick(&mut app, 0);
        assert_eq!(app.world().resource::<State>().unwrap().choices.len(), 1);
        app.world_mut()
            .resource_mut::<Model>()
            .unwrap()
            .models
            .reverse();
        pick(&mut app, 0);
        assert!(handle(app.world()).is_none());
        // Stale menu index was not applied to the reordered catalog.
        assert_eq!(app.world().resource::<Model>().unwrap().selected, Some(0));
    }
    #[test]
    fn escape_and_surface_switch_close_without_changing_selection() {
        let mut app = fixture(two_providers());
        open(app.world_mut());
        app.world_mut().resource_mut::<Input>().unwrap().keys =
            vec![KeyEvent::named(NamedKey::Escape, Modifiers::default())];
        app.update();
        app.update();
        assert!(handle(app.world()).is_none());
        assert_eq!(app.world().resource::<Model>().unwrap().selected, Some(0));
        open(app.world_mut());
        crate::surfaces::select(app.world_mut(), crate::surfaces::SurfaceId::Notes);
        app.update();
        assert!(handle(app.world()).is_none());
    }
}

#[cfg(test)]
#[test]
#[ignore = "Provider-first picker Parley/GPU visual acceptance"]
fn provider_picker_visual_acceptance() {
    let engine: Arc<dyn TextEngine> = Arc::new(rndr_render::ParleyText::new());
    let mut exporter = rndr_render::Exporter::new().unwrap();
    std::fs::create_dir_all("verification").unwrap();
    for (width, dark, suffix) in [(1200, true, "dark"), (760, false, "light-narrow")] {
        let mut app = app();
        app.world_mut()
            .insert_resource(Viewport::new(width as f32, 850.));
        app.world_mut().insert_resource(engine.clone());
        app.world_mut()
            .insert_resource(rndr_ui::InteractionMotion::snap());
        app.update();
        let m = app.world_mut().resource_mut::<Model>().unwrap();
        m.chat
            .hydrate("s".into(), "a".into(), vec![], None)
            .unwrap();
        m.models = vec![
            ("anthropic".into(), "Claude Opus".into()),
            ("openai-codex".into(), "gpt-6.1-sol".into()),
            ("openai-codex".into(), "gpt-6.1".into()),
            ("openai-codex".into(), "gpt-6-mini".into()),
            (
                "openrouter".into(),
                "openrouter/long-provider-specific-model-name".into(),
            ),
        ];
        m.selected = Some(1);
        publish_system_appearance(
            app.world_mut(),
            SystemAppearance {
                dark: Some(dark),
                ..Default::default()
            },
        );
        for _ in 0..8 {
            rndr_paint::frame(&mut app);
        }
        open(app.world_mut());
        for stage in ["providers", "models"] {
            if stage == "models" {
                let item = handle(app.world()).unwrap().item(app.world(), 1).unwrap();
                assert_eq!(
                    try_activate(app.world_mut(), item),
                    ActivationResult::Accepted
                );
            }
            for _ in 0..8 {
                std::thread::sleep(std::time::Duration::from_millis(40));
                rndr_paint::frame(&mut app);
            }
            let menu = handle(app.world()).expect("visible picker");
            assert!(menu.is_open(app.world()));
            let rect = app.world().get::<Rect>(menu.root).unwrap();
            assert!(
                rect.x >= 0.
                    && rect.x + rect.w <= width as f32 + 1.
                    && rect.y >= 0.
                    && rect.y + rect.h <= 851.,
                "{rect:?}"
            );
            let (_, image) = rndr_paint::frame_with_snapshot_frame(&mut app, |world, frame| {
                exporter
                    .render_frame(Some(world), frame, width, 850, 1.)
                    .unwrap()
            });
            image::save_buffer(
                format!("verification/native-model-picker-{stage}-{suffix}.png"),
                &image.rgba,
                image.width,
                image.height,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
    }
}
