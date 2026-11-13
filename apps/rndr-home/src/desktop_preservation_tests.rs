//! Permanent reviewer reproductions and bounded correction regressions.
use super::*;

#[cfg(test)]
mod independent_review_tests {
    use super::*;
    fn settle(a: &mut App) {
        for _ in 0..8 {
            a.update();
        }
    }
    #[test]
    fn focus_on_preferences_control_survives_responsive_rebuild() {
        let mut a = app();
        a.update();
        desktop::navigate(a.world_mut(), admin_model::Route::Settings(0));
        settle(&mut a);
        let e = a
            .world()
            .entities_with::<Button>()
            .into_iter()
            .find(|e| a.world().get::<Button>(*e).unwrap().action == "appearance:light")
            .unwrap();
        request_focus(a.world_mut(), e);
        settle(&mut a);
        assert_eq!(a.world().resource::<Focus>().unwrap().entity, Some(e));
        a.world_mut().insert_resource(Viewport::new(480., 760.));
        settle(&mut a);
        let f = a.world().resource::<Focus>().unwrap().entity;
        assert!(
            f.is_some_and(|e| a.world().has::<Focusable>(e)),
            "focus lost/deleted on responsive rebuild: {f:?}"
        );
    }
    #[test]
    fn unrelated_reply_preserves_current_dataset_scroll() {
        use admin_model::{Desktop, ReadState, Record, Resource, Route};
        let mut a = app();
        a.update();
        a.world_mut()
            .resource_mut::<Desktop>()
            .unwrap()
            .reads
            .insert(
                Resource::Runtimes,
                ReadState::Ready(
                    (0..80)
                        .map(|i| {
                            Record::new(format!("Row {i}"), vec![("Status", "Snapshot".into())])
                        })
                        .collect(),
                ),
            );
        let serial = a
            .world_mut()
            .resource_mut::<Desktop>()
            .unwrap()
            .begin(Resource::Health)
            .unwrap();
        desktop::navigate(a.world_mut(), Route::Data(Resource::Runtimes));
        settle(&mut a);
        let body = a.world().resource::<desktop::Handles>().unwrap().body;
        a.world_mut().get_mut::<Scroll>(body).unwrap().offset[1] = -200.;
        settle(&mut a);
        let before = a.world().get::<Scroll>(body).unwrap().offset;
        assert!(before[1] < 0.);
        send(
            a.world_mut(),
            admin_backend::ReadReply {
                resource: Resource::Health,
                serial,
                result: Ok(vec![]),
            },
        );
        settle(&mut a);
        assert_eq!(
            a.world().get::<Scroll>(body).unwrap().offset,
            before,
            "background Health reply reset Runtimes scroll"
        );
    }
}

#[cfg(test)]
mod correction_tests {
    use super::*;
    use admin_model::{Desktop, ReadState, Record, Resource, Route};
    fn settle(a: &mut App) {
        for _ in 0..8 {
            a.update();
        }
    }
    fn button(a: &App, action: &str) -> Entity {
        a.world()
            .entities_with::<Button>()
            .into_iter()
            .find(|e| a.world().get::<Button>(*e).unwrap().action == action)
            .expect(action)
    }
    fn focused_action(a: &App) -> String {
        let e = a
            .world()
            .resource::<Focus>()
            .unwrap()
            .entity
            .expect("focus survives");
        assert!(a.world().has::<Focusable>(e));
        a.world()
            .get::<Button>(e)
            .expect("button focus")
            .action
            .clone()
    }
    fn rows(n: usize) -> Vec<Record> {
        (0..n)
            .map(|i| Record::new(format!("Row {i}"), vec![("Status", "Snapshot".into())]))
            .collect()
    }
    fn dataset(a: &mut App) -> Entity {
        a.world_mut()
            .resource_mut::<Desktop>()
            .unwrap()
            .reads
            .insert(Resource::Runtimes, ReadState::Ready(rows(80)));
        desktop::navigate(a.world_mut(), Route::Data(Resource::Runtimes));
        settle(a);
        let body = a.world().resource::<desktop::Handles>().unwrap().body;
        a.world_mut().get_mut::<Scroll>(body).unwrap().offset[1] = -200.;
        settle(a);
        body
    }
    #[test]
    fn keyboard_tab_and_enter_continue_from_migrated_appearance_control() {
        let mut a = app();
        a.update();
        desktop::navigate(a.world_mut(), Route::Settings(0));
        settle(&mut a);
        let e = button(&a, "appearance:light");
        request_focus(a.world_mut(), e);
        a.world_mut().insert_resource(Viewport::new(480., 650.));
        settle(&mut a);
        a.world_mut().resource_mut::<Input>().unwrap().keys =
            vec![KeyEvent::named(NamedKey::Enter, Modifiers::new())];
        a.update();
        settle(&mut a);
        assert_eq!(presentation::theme(a.world()).appearance, Appearance::Light);
        a.world_mut().resource_mut::<Input>().unwrap().keys =
            vec![KeyEvent::named(NamedKey::Tab, Modifiers::new())];
        a.update();
        settle(&mut a);
        assert_eq!(focused_action(&a), "appearance:dark");
        a.world_mut().resource_mut::<Input>().unwrap().keys =
            vec![KeyEvent::named(NamedKey::Enter, Modifiers::new())];
        a.update();
        settle(&mut a);
        assert_eq!(presentation::theme(a.world()).appearance, Appearance::Dark);
        assert_eq!(focused_action(&a), "appearance:dark");
        a.world_mut().resource_mut::<Input>().unwrap().keys = vec![KeyEvent::named(
            NamedKey::Tab,
            Modifiers {
                shift: true,
                ..Modifiers::new()
            },
        )];
        a.update();
        settle(&mut a);
        assert_eq!(focused_action(&a), "appearance:light");
        assert!(!composer::native_input_ready(a.world()));
    }
    #[test]
    fn appearance_focus_identity_activation_and_accessible_name_survive_roundtrip_resize() {
        for action in ["appearance:light", "appearance:dark", "appearance:contrast"] {
            let mut a = app();
            a.update();
            desktop::navigate(a.world_mut(), Route::Settings(0));
            settle(&mut a);
            let e = button(&a, action);
            assert!(request_focus(a.world_mut(), e));
            let name = a.world().get::<Label>(e).unwrap().text.clone();
            for width in [480., 1120., 480., 1120.] {
                a.world_mut().insert_resource(Viewport::new(width, 650.));
                settle(&mut a);
                assert_eq!(focused_action(&a), action);
                let f = a.world().resource::<Focus>().unwrap().entity.unwrap();
                assert_eq!(a.world().get::<Label>(f).unwrap().text, name);
                let tree = access_tree(a.world());
                let node = tree.nodes.iter().find(|n| n.id == access_id(f)).unwrap();
                assert_eq!(node.name, name);
                assert!(node.focused && node.focusable && node.enabled);
                assert!(access_details(a.world(), f).activate);
                assert_eq!(try_activate(a.world_mut(), f), ActivationResult::Accepted);
                settle(&mut a);
                assert_eq!(focused_action(&a), action);
            }
        }
    }
    #[test]
    fn settings_nav_pager_and_resource_link_focus_migrate_or_use_visible_fallback() {
        for (route, action, expected) in [
            (Route::Settings(0), "settings:1", "settings:1"), // wide sidebar -> next
            (Route::Settings(0), "settings:8", "settings:8"), // wide sidebar -> previous
            (Route::Settings(0), "settings:4", "desk:settings"), // no narrow equivalent
            (Route::Settings(3), "data:2", "data:2"),         // body link not rail
        ] {
            let mut a = app();
            a.update();
            desktop::navigate(a.world_mut(), route);
            settle(&mut a);
            // For the resource link choose a descendant in the body (not sidebar).
            let e = if action == "data:2" {
                let body = a.world().resource::<desktop::Handles>().unwrap().body;
                a.world()
                    .entities_with::<Button>()
                    .into_iter()
                    .find(|e| {
                        a.world().get::<Button>(*e).unwrap().action == action
                            && a.world().get::<Parent>(*e).unwrap().entity == body
                    })
                    .unwrap()
            } else {
                button(&a, action)
            };
            assert!(request_focus(a.world_mut(), e));
            a.world_mut().insert_resource(Viewport::new(480., 650.));
            settle(&mut a);
            assert_eq!(focused_action(&a), expected);
            a.world_mut().insert_resource(Viewport::new(1120., 650.));
            settle(&mut a);
            assert_eq!(focused_action(&a), expected);
            let f = a.world().resource::<Focus>().unwrap().entity.unwrap();
            assert_eq!(try_activate(a.world_mut(), f), ActivationResult::Accepted);
            settle(&mut a);
            assert!(!composer::native_input_ready(a.world()));
        }
    }
    #[test]
    fn offroute_begin_and_completion_do_not_invalidate_selection_or_entities() {
        let mut a = app();
        a.update();
        let body = dataset(&mut a);
        desktop::action(a.world_mut(), "record:5");
        settle(&mut a);
        let e = button(&a, "admin:back");
        request_focus(a.world_mut(), e);
        let serial = a
            .world_mut()
            .resource_mut::<Desktop>()
            .unwrap()
            .begin(Resource::Health)
            .unwrap();
        settle(&mut a);
        assert_eq!(a.world().resource::<Desktop>().unwrap().selected, Some(5));
        assert_eq!(a.world().resource::<Focus>().unwrap().entity, Some(e));
        send(
            a.world_mut(),
            admin_backend::ReadReply {
                resource: Resource::Health,
                serial,
                result: Ok(rows(3)),
            },
        );
        settle(&mut a);
        assert_eq!(a.world().resource::<Desktop>().unwrap().selected, Some(5));
        assert_eq!(a.world().resource::<Focus>().unwrap().entity, Some(e));
        assert_eq!(a.world().get::<Scroll>(body).unwrap().offset, [0., 0.]);
        assert_eq!(
            a.world().resource::<Desktop>().unwrap().reads[&Resource::Health],
            ReadState::Ready(rows(3))
        );
    }
    #[test]
    fn visible_refresh_restores_position_then_clamps_shorter_results_and_navigation_resets() {
        let mut a = app();
        a.update();
        let body = dataset(&mut a);
        for width in [480., 1120., 480., 1120.] {
            a.world_mut().insert_resource(Viewport::new(width, 650.));
            settle(&mut a);
            assert_eq!(a.world().get::<Scroll>(body).unwrap().offset[1], -200.);
        }
        for count in [90, 70, 2, 0] {
            let before = a.world().get::<Scroll>(body).unwrap().offset[1];
            let serial = a
                .world_mut()
                .resource_mut::<Desktop>()
                .unwrap()
                .begin(Resource::Runtimes)
                .unwrap();
            settle(&mut a);
            send(
                a.world_mut(),
                admin_backend::ReadReply {
                    resource: Resource::Runtimes,
                    serial,
                    result: Ok(rows(count)),
                },
            );
            settle(&mut a);
            assert_eq!(
                a.world()
                    .entities_with::<Button>()
                    .into_iter()
                    .filter(|e| a
                        .world()
                        .get::<Button>(*e)
                        .unwrap()
                        .action
                        .starts_with("record:"))
                    .count(),
                count
            );
            let scroll = a.world().get::<Scroll>(body).unwrap();
            let port = a.world().get::<Rect>(body).unwrap();
            let expected = before.max((port.h - scroll.content[1]).min(0.));
            assert!(
                (scroll.offset[1] - expected).abs() < 0.5,
                "count={count} {scroll:?}, expected {expected}"
            );
            if count >= 70 {
                assert_eq!(scroll.offset[1], -200.);
            } else {
                assert_eq!(scroll.offset[1], 0.);
            }
        }
        dataset(&mut a);
        desktop::navigate(a.world_mut(), Route::Settings(0));
        settle(&mut a);
        desktop::navigate(a.world_mut(), Route::Data(Resource::Runtimes));
        settle(&mut a);
        assert_eq!(a.world().get::<Scroll>(body).unwrap().offset, [0., 0.]);
    }
    #[test]
    fn refresh_from_selected_detail_clears_index_and_shows_new_results() {
        let mut a = app();
        a.update();
        dataset(&mut a);
        desktop::action(a.world_mut(), "record:5");
        settle(&mut a);
        let serial = a
            .world_mut()
            .resource_mut::<Desktop>()
            .unwrap()
            .begin(Resource::Runtimes)
            .unwrap();
        settle(&mut a);
        assert_eq!(a.world().resource::<Desktop>().unwrap().selected, None);
        send(
            a.world_mut(),
            admin_backend::ReadReply {
                resource: Resource::Runtimes,
                serial,
                result: Ok(vec![Record::new("New data", vec![])]),
            },
        );
        settle(&mut a);
        assert_eq!(
            a.world().get::<Label>(button(&a, "record:0")).unwrap().text,
            "New data"
        );
        assert!(
            !a.world().entities_with::<Button>().into_iter().any(|e| a
                .world()
                .get::<Button>(e)
                .unwrap()
                .action
                == "admin:back")
        );
    }
    #[test]
    fn resized_admin_updates_preserve_retained_chat_draft_session_and_scroll() {
        let (mut a, mut chat_rx) = ui_tests::ready_unverified();
        let ui = *a.world().resource::<Ui>().unwrap();
        a.world_mut()
            .resource_mut::<Model>()
            .unwrap()
            .history_messages = (0..1000)
            .map(|i| {
                (
                    "assistant".into(),
                    format!("Synthetic {i}\n\n{}", "Paragraph. ".repeat(40)),
                )
            })
            .collect();
        reset_prompt(a.world_mut(), ui.composer, "retained correction draft");
        settle(&mut a);
        a.world_mut()
            .get_mut::<Scroll>(ui.conversation)
            .unwrap()
            .offset[1] = -200.;
        settle(&mut a);
        let before = a.world().get::<Scroll>(ui.conversation).unwrap().offset;
        dataset(&mut a);
        for width in [480., 1120.] {
            a.world_mut().insert_resource(Viewport::new(width, 650.));
            settle(&mut a);
        }
        let serial = a
            .world_mut()
            .resource_mut::<Desktop>()
            .unwrap()
            .begin(Resource::Health)
            .unwrap();
        send(
            a.world_mut(),
            admin_backend::ReadReply {
                resource: Resource::Health,
                serial,
                result: Ok(rows(3)),
            },
        );
        settle(&mut a);
        desktop::navigate(a.world_mut(), Route::Chat);
        settle(&mut a);
        assert_eq!(
            a.world().get::<Label>(ui.composer).unwrap().text,
            "retained correction draft"
        );
        assert_eq!(
            a.world().get::<Scroll>(ui.conversation).unwrap().offset,
            before
        );
        assert_eq!(
            a.world()
                .resource::<Model>()
                .unwrap()
                .chat
                .session
                .as_deref(),
            Some("s")
        );
        assert!(!composer::native_input_ready(a.world()));
        assert!(!a.world().get::<Button>(ui.send).unwrap().enabled);
        assert!(chat_rx.try_recv().is_err());
    }
}
