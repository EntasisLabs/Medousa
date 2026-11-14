//! Negative branches of the existing offline-only observation predicate.
//! These tests enable no observation session, descriptor, daemon or network.
use super::*;
use rndr_render::observe::ObservationContent;

fn assert_branch_denied(setup: impl FnOnce(&mut World)) {
    let mut app = crate::app();
    app.world_mut()
        .insert_resource(crate::composer::SyntheticEditor);
    app.update();
    app.world_mut().insert_resource(FixtureObservation);
    app.world_mut().insert_resource(ObservationContent::Deny);
    app.update();
    assert!(matches!(
        app.world().resource::<ObservationContent>(),
        Some(ObservationContent::Public)
    ));
    setup(app.world_mut());
    app.update();
    assert!(matches!(
        app.world().resource::<ObservationContent>(),
        Some(ObservationContent::Deny)
    ));
    // Repeated sync must not accidentally re-publicize the private fixture.
    app.update();
    assert!(matches!(
        app.world().resource::<ObservationContent>(),
        Some(ObservationContent::Deny)
    ));
}

#[test]
fn unknown_native_state_denies_even_empty_opted_in_fixture() {
    let mut app = crate::app();
    app.update();
    app.world_mut().insert_resource(FixtureObservation);
    for _ in 0..3 {
        app.update();
        assert!(matches!(
            app.world().resource::<ObservationContent>(),
            Some(ObservationContent::Deny)
        ));
    }
}

#[test]
fn history_denies_observation() {
    assert_branch_denied(|world| {
        world.resource_mut::<Model>().unwrap().fixture_rows =
            vec![("user".into(), "synthetic private history".into())];
    });
}

#[test]
fn live_messages_deny_observation() {
    assert_branch_denied(|world| {
        world
            .resource_mut::<Model>()
            .unwrap()
            .chat
            .set_fixture(vec![("assistant".into(), "synthetic live suffix".into())]);
    });
}

#[test]
fn recovery_denies_observation() {
    assert_branch_denied(|world| {
        let m = world.resource_mut::<Model>().unwrap();
        m.chat
            .fail("Unconfirmed admission · private draft preserved".into());
    });
}

#[test]
fn command_sender_denies_observation() {
    // A channel alone denies Public even with no private text or worker.
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    assert_branch_denied(|world| {
        world.resource_mut::<Model>().unwrap().commands = Some(tx);
    });
    assert!(
        rx.try_recv().is_err(),
        "observation never dispatches commands"
    );
}

#[test]
fn empty_without_opt_in_remains_denied() {
    let mut app = crate::app();
    app.world_mut().insert_resource(ObservationContent::Deny);
    for _ in 0..3 {
        app.update();
        assert!(!app.world().has_resource::<FixtureObservation>());
        assert!(matches!(
            app.world().resource::<ObservationContent>(),
            Some(ObservationContent::Deny)
        ));
    }
}
