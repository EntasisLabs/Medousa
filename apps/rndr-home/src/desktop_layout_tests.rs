//! Synthetic Parley/layout checks; not native pointer/keyboard qualification.
use super::*;
use admin_model::{Desktop, ReadState, Record, Resource, Route};
use desktop::Handles;
use std::sync::Arc;
fn settle(app: &mut App) {
    for _ in 0..8 {
        app.update();
    }
}
#[test]
fn settings_and_admin_viewports_fit_both_widths_and_all_supported_themes() {
    let mut app = app();
    app.update();
    app.world_mut()
        .insert_resource(Arc::new(rndr_render::ParleyText::new()) as Arc<dyn TextEngine>);
    for (appearance, contrast) in [
        (Appearance::Dark, Contrast::Standard),
        (Appearance::Light, Contrast::Standard),
        (Appearance::Dark, Contrast::High),
    ] {
        presentation::apply_theme(
            app.world_mut(),
            UiTheme {
                appearance,
                contrast,
                ..Default::default()
            },
        );
        for (width, height) in [(480., 820.), (1120., 820.), (480., 650.), (1120., 650.)] {
            app.world_mut()
                .insert_resource(Viewport::new(width, height));
            let routes: Vec<_> = (0..9)
                .map(Route::Settings)
                .chain(Resource::ALL.into_iter().map(Route::Data))
                .collect();
            for route in routes {
                if let Some(resource) = route.resource() {
                    app.world_mut()
                        .resource_mut::<Desktop>()
                        .unwrap()
                        .reads
                        .insert(
                            resource,
                            ReadState::Ready(vec![Record::new(
                                "Synthetic long entry ".repeat(14),
                                vec![("Status", "Not connected (fixture)".into())],
                            )]),
                        );
                }
                desktop::navigate(app.world_mut(), route);
                settle(&mut app);
                let h = app.world().resource::<Handles>().unwrap();
                assert_eq!(app.world().has::<Hidden>(h.subtitle), width < 800.);
                assert!(app.world().get::<Rect>(h.title).unwrap().h <= 42.);
                for entity in [h.page, h.body, h.section_nav, h.footer] {
                    let r = app.world().get::<Rect>(entity).unwrap();
                    assert!(
                        r.x >= -0.5 && r.x + r.w <= width + 0.5,
                        "{route:?} {width}: {r:?}"
                    );
                    assert!(
                        r.y >= -0.5 && r.y + r.h <= height + 0.5,
                        "{route:?} {width}: {r:?}"
                    );
                }
                assert!(app.world().get::<Rect>(h.body).unwrap().h > 200.);
                if width > 800. {
                    assert!(app.world().has::<Scroll>(h.section_nav));
                }
                for (entity, _) in &h.nav {
                    let r = app.world().get::<Rect>(*entity).unwrap();
                    assert!(r.x >= -0.5 && r.x + r.w <= desktop::rail_width(app.world()) + 0.5);
                }
                let focused = app.world().resource::<Focus>().unwrap().entity.unwrap();
                assert!(
                    app.world().has::<Focusable>(focused),
                    "focus must not retain a deleted view control"
                );
            }
        }
    }
}
#[test]
fn settings_section_pager_and_sidebar_scroll_keep_last_section_reachable() {
    for width in [480., 1120.] {
        let mut app = app();
        app.update();
        app.world_mut().insert_resource(Viewport::new(width, 650.));
        desktop::navigate(app.world_mut(), Route::Settings(0));
        settle(&mut app);
        if width < 800. {
            for i in 1..9 {
                let action = format!("settings:{i}");
                let e = app
                    .world()
                    .entities_with::<Button>()
                    .into_iter()
                    .find(|e| app.world().get::<Button>(*e).unwrap().action == action)
                    .unwrap();
                assert_eq!(try_activate(app.world_mut(), e), ActivationResult::Accepted);
                settle(&mut app);
                assert_eq!(
                    app.world().resource::<Desktop>().unwrap().route,
                    Route::Settings(i)
                );
            }
        } else {
            let h = app.world().resource::<Handles>().unwrap().clone();
            assert!(app.world().has::<Scroll>(h.section_nav));
            app.world_mut()
                .get_mut::<Scroll>(h.section_nav)
                .unwrap()
                .offset[1] = -500.;
            settle(&mut app);
            let e = app
                .world()
                .entities_with::<Button>()
                .into_iter()
                .find(|e| app.world().get::<Button>(*e).unwrap().action == "settings:8")
                .unwrap();
            let r = *app.world().get::<Rect>(e).unwrap();
            let port = *app.world().get::<Rect>(h.section_nav).unwrap();
            // rndr retains layout Rects; Scroll translates descendants for painting/hit testing.
            let offset = app.world().get::<Scroll>(h.section_nav).unwrap().offset[1];
            assert!(
                r.y + offset >= port.y && r.y + offset + r.h <= port.y + port.h + 0.5,
                "last section must be visible after sidebar scroll: {r:?} {port:?} {offset}"
            );
        }
    }
}
