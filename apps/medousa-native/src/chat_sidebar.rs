//! Chat history presentation mirrors SessionSidebar + sessionHistoryGroups.
//! Wire summaries stay intact; grouping never changes catalog indices/routes.
use super::*;
use chrono::{DateTime, Local, NaiveDate, Utc};
use medousa_types::SessionHistorySummary;
use std::collections::HashSet;

const SECTION: u64 = 80;
const LABELS: [&str; 4] = ["Today", "Yesterday", "Previous 7 days", "Older"];
#[derive(Default)]
pub(super) struct State {
    summaries: HashMap<String, SessionHistorySummary>,
    collapsed: HashSet<usize>,
    initialized: bool,
    sections: [Option<Entity>; 4],
    revision: u64,
}
pub(super) fn receive(world: &mut World, summaries: Vec<SessionHistorySummary>, append: bool) {
    if world.resource::<State>().is_none() {
        world.insert_resource(State::default());
    }
    let s = world.resource_mut::<State>().unwrap();
    if !append {
        s.summaries.clear();
    }
    for row in summaries {
        s.summaries.insert(row.session_id.clone(), row);
    }
    s.revision += 1;
}
pub(super) fn action(world: &mut World, id: u64) -> bool {
    if !(SECTION..SECTION + 4).contains(&id) {
        return false;
    }
    if !world.resource::<Model>().unwrap().search_query.is_empty() {
        return true;
    }
    let s = world.resource_mut::<State>().unwrap();
    let group = (id - SECTION) as usize;
    if !s.collapsed.remove(&group) {
        s.collapsed.insert(group);
    }
    s.revision += 1;
    true
}
fn bucket(timestamp: Option<DateTime<Utc>>, today: NaiveDate) -> usize {
    let Some(timestamp) = timestamp else {
        return 3;
    };
    let days = (today - timestamp.with_timezone(&Local).date_naive()).num_days();
    match days {
        ..=0 => 0,
        1 => 1,
        2..=7 => 2,
        _ => 3,
    }
}
pub(super) fn render(
    world: &mut World,
    parent: Entity,
    rows: &[(String, String)],
    selected: Option<&str>,
) {
    if world.resource::<State>().is_none() {
        world.insert_resource(State::default());
    }
    let s = world.resource_mut::<State>().unwrap();
    if !s.initialized {
        s.collapsed.insert(3);
        s.initialized = true;
    }
    let revision = s.revision;
    let today = Local::now().date_naive();
    let key = format!(
        "{rows:?}{selected:?}{today}{revision}{}",
        world.resource::<Model>().unwrap().search_query
    );
    if world.resource::<Model>().unwrap().sidebar_key == key {
        return;
    }
    world.resource_mut::<Model>().unwrap().sidebar_key = key;
    let searching = !world.resource::<Model>().unwrap().search_query.is_empty();
    let s = world.resource::<State>().unwrap();
    let groups: Vec<Vec<usize>> = (0..4)
        .map(|g| {
            rows.iter()
                .enumerate()
                .filter_map(|(i, (id, _))| {
                    (bucket(s.summaries.get(id).and_then(|s| s.last_timestamp), today) == g)
                        .then_some(i)
                })
                .collect()
        })
        .collect();
    let collapsed = s.collapsed.clone();
    let metadata = s.summaries.clone();
    // Mount sections in canonical order even when the first catalog omits a bucket.
    for g in 0..4 {
        if world.resource::<State>().unwrap().sections[g].is_none() {
            let section = container(
                world,
                Some(parent),
                Layout {
                    width: Size::Fill,
                    height: Size::Hug,
                    gap: 2.,
                    ..Layout::column()
                },
            );
            world.resource_mut::<State>().unwrap().sections[g] = Some(section);
        }
    }
    for group in children_of(world, parent) {
        world.insert(group, Hidden);
    }
    for group in world
        .resource::<State>()
        .unwrap()
        .sections
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
    {
        for child in children_of(world, group) {
            world.insert(child, Hidden);
        }
    }
    for (g, indices) in groups.iter().enumerate().filter(|(_, v)| !v.is_empty()) {
        // A retained section survives selection/search/collapse without rebuilding controls.
        let section = if let Some(e) = world.resource::<State>().unwrap().sections[g] {
            e
        } else {
            let e = container(
                world,
                Some(parent),
                Layout {
                    width: Size::Fill,
                    height: Size::Hug,
                    gap: 2.,
                    ..Layout::column()
                },
            );
            world.resource_mut::<State>().unwrap().sections[g] = Some(e);
            e
        };
        world.remove::<Hidden>(section);
        let hidden = !searching && collapsed.contains(&g);
        let mut props = ButtonProps::new(
            format!(
                "{} {}",
                if hidden {
                    Icon::ChevronRight.scalar()
                } else {
                    Icon::ChevronDown.scalar()
                },
                LABELS[g]
            ),
            HostIntent::Command(SECTION + g as u64),
        );
        props.enabled = !searching;
        props.variant = controls::ButtonVariant::Quiet;
        let h = controls::NavigationRow::mount(
            world,
            section,
            UiId(SECTION + g as u64),
            props,
            NavigationMeta {
                secondary: Some(indices.len().to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        *world.get_mut::<Layout>(h.root).unwrap() = Layout {
            width: Size::Fill,
            height: Size::Px(26.),
            padding: 4.,
            ..Layout::row()
        };
        world.insert(
            h.label,
            UiRecipe(UiStyle {
                foreground: Some(visual::secondary().into()),
                typography: Some(Typography::Secondary),
                ..Default::default()
            }),
        );
        world.insert(
            h.root,
            Semantics {
                name: Some(LABELS[g].into()),
                description: if searching {
                    "Search results stay expanded"
                } else if hidden {
                    "Collapsed"
                } else {
                    "Expanded"
                }
                .into(),
                ..Default::default()
            },
        );
        world.remove::<Hidden>(h.root);
        if hidden {
            continue;
        }
        for &i in indices {
            let (id, title) = &rows[i];
            let props = ButtonProps::new(
                if title.trim().is_empty() {
                    "Untitled chat"
                } else {
                    title
                },
                HostIntent::Command(SESSION + i as u64),
            );
            let meta = metadata
                .get(id)
                .filter(|s| s.turns > 0)
                .map(|s| format!("{} turns", s.turns));
            let h = controls::NavigationRow::mount(
                world,
                section,
                UiId(SESSION + i as u64),
                props,
                NavigationMeta {
                    selected: selected == Some(id),
                    secondary: meta,
                    ..Default::default()
                },
            )
            .unwrap();
            world.get_mut::<Label>(h.label).unwrap().overflow = TextOverflow::Ellipsis;
            world.get_mut::<Layout>(h.root).unwrap().height = Size::Px(32.);
            world.insert(h.root, Radius(visual::CONTROL_RADIUS));
            world.remove::<Hidden>(h.root);
        }
    }
    if rows.is_empty() {
        // The empty state uses the same keyed controls as every other row.
        let h = controls::NavigationRow::mount(
            world,
            parent,
            UiId(0x5347_0100),
            ButtonProps {
                enabled: false,
                ..ButtonProps::new(
                    if searching {
                        "No matching conversations"
                    } else {
                        "No conversations yet"
                    },
                    HostIntent::Command(0),
                )
            },
            NavigationMeta::default(),
        )
        .unwrap();
        world.remove::<Hidden>(h.root);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> App {
        let mut app = crate::ui::app();
        app.update();
        let now = Utc::now();
        let rows: Vec<SessionHistorySummary> = [("today", 0), ("yesterday", 1), ("week", 4), ("old", 12)].into_iter().map(|(id, days)| serde_json::from_value(serde_json::json!({
            "session_id": id, "display_name": format!("{id} conversation"), "preview":"", "turns":12,
            "verification_runs":0, "last_timestamp": now - chrono::Duration::days(days)
        })).unwrap()).collect();
        app.world_mut().resource_mut::<Model>().unwrap().sessions = rows
            .iter()
            .map(|s| (s.session_id.clone(), s.display_name.clone().unwrap()))
            .collect();
        receive(app.world_mut(), rows, false);
        app.update();
        app
    }
    fn visible(w: &World, e: Entity) -> bool {
        !w.has::<Hidden>(e) && w.get::<Parent>(e).is_none_or(|p| visible(w, p.entity))
    }
    fn row(w: &World, group: usize, index: usize) -> Entity {
        let section = w.resource::<State>().unwrap().sections[group].unwrap();
        children_of(w, section)
            .into_iter()
            .find(|e| rndr_ui::identity(w, *e) == Some(UiId(SESSION + index as u64)))
            .unwrap()
    }
    #[test]
    fn collapse_search_and_section_order_preserve_routes_and_document() {
        let mut app = fixture();
        let document = app.world().resource::<Model>().unwrap().document;
        assert!(visible(app.world(), row(app.world(), 0, 0)));
        assert!(
            !app.world()
                .resource::<State>()
                .unwrap()
                .collapsed
                .contains(&0)
        );
        assert!(
            app.world()
                .resource::<State>()
                .unwrap()
                .collapsed
                .contains(&3)
        );
        action(app.world_mut(), SECTION);
        app.update();
        assert!(!visible(app.world(), row(app.world(), 0, 0)));
        app.world_mut()
            .resource_mut::<Model>()
            .unwrap()
            .search_query = "conversation".into();
        app.update();
        assert!(visible(app.world(), row(app.world(), 0, 0)));
        assert!(visible(app.world(), row(app.world(), 3, 3)));
        assert!(action(app.world_mut(), SECTION)); // disclosure is inert during search
        assert!(
            app.world()
                .resource::<State>()
                .unwrap()
                .collapsed
                .contains(&0)
        );
        app.world_mut()
            .resource_mut::<Model>()
            .unwrap()
            .search_query
            .clear();
        app.update();
        assert!(!visible(app.world(), row(app.world(), 3, 3)));
        assert_eq!(app.world().resource::<Model>().unwrap().document, document);
        let sections = app.world().resource::<State>().unwrap().sections;
        let parent = app.world().resource::<Ui>().unwrap().sessions;
        assert_eq!(
            &children_of(app.world(), parent)[..4],
            &sections.map(Option::unwrap)
        );
    }
    #[test]
    fn metadata_append_replaces_identity_and_rejects_stale_queries() {
        let mut app = fixture();
        let rows = app.world().resource::<Model>().unwrap().sessions.clone();
        let mut summary = app.world().resource::<State>().unwrap().summaries["today"].clone();
        summary.turns = 99;
        receive(app.world_mut(), vec![summary.clone()], true);
        assert_eq!(app.world().resource::<State>().unwrap().summaries.len(), 4);
        assert_eq!(
            app.world().resource::<State>().unwrap().summaries["today"].turns,
            99
        );
        let generation = app.world().resource::<Model>().unwrap().generation;
        apply_current_reply(
            app.world_mut(),
            Reply {
                generation,
                update: Update::SessionsPage {
                    query: "stale".into(),
                    rows: vec![],
                    summaries: vec![],
                    cursor: None,
                    append: false,
                },
            },
        );
        assert_eq!(app.world().resource::<Model>().unwrap().sessions, rows);
        assert_eq!(app.world().resource::<State>().unwrap().summaries.len(), 4);
    }
    #[test]
    #[ignore = "Synthetic history grouping GPU visual acceptance"]
    fn chat_sidebar_visual_acceptance() {
        let mut app = fixture();
        let engine: Arc<dyn TextEngine> = Arc::new(rndr_render::ParleyText::new());
        app.world_mut().insert_resource(engine);
        let mut exporter = rndr_render::Exporter::new().unwrap();
        for (width, height, dark, name) in
            [(1200, 850, true, "dark"), (760, 650, false, "light-narrow")]
        {
            publish_system_appearance(
                app.world_mut(),
                SystemAppearance {
                    dark: Some(dark),
                    ..Default::default()
                },
            );
            app.world_mut()
                .insert_resource(Viewport::new(width as f32, height as f32));
            for _ in 0..10 {
                rndr_paint::frame(&mut app);
            }
            let (_, image) = rndr_paint::frame_with_snapshot_frame(&mut app, |world, frame| {
                exporter
                    .render_frame(Some(world), frame, width, height, 1.)
                    .unwrap()
            });
            std::fs::create_dir_all("verification").unwrap();
            image::save_buffer(
                format!("verification/native-history-sidebar-{name}.png"),
                &image.rgba,
                image.width,
                image.height,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
    }
    #[test]
    fn history_uses_local_calendar_days_including_future_and_unknown_dates() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 16).unwrap();
        for (days, expected) in [(-1, 0), (0, 0), (1, 1), (2, 2), (7, 2), (8, 3)] {
            let day = today - chrono::Duration::days(days);
            let timestamp = day
                .and_hms_opt(12, 0, 0)
                .unwrap()
                .and_local_timezone(Local)
                .earliest()
                .unwrap()
                .with_timezone(&Utc);
            assert_eq!(bucket(Some(timestamp), today), expected);
        }
        assert_eq!(bucket(None, today), 3);
    }
}
