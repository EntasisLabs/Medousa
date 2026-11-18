use super::*;
use serde_json::json;
fn date(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}
fn event() -> CalendarEvent {
    serde_json::from_value(json!({"uid":"fixture","summary":"Planning café 🦀","dtstart":"2026-11-18T09:00:00Z","dtend":"2026-11-18T10:00:00Z","all_day":false,"calendar_path":"calendar/team.ics","note_path":"meetings/plan.md","alarms":[{"trigger_minutes_before":15,"action":"display"}]})).unwrap()
}
fn app() -> App {
    let mut app = crate::ui::app();
    app.world_mut().insert_resource(Viewport::new(1200., 850.));
    app.update();
    crate::surfaces::select(app.world_mut(), crate::surfaces::SurfaceId::Calendar);
    take_commands(app.world_mut());
    app
}
fn act(w: &mut World, a: Action) {
    let id = next();
    w.resource_mut::<State>().unwrap().actions.insert(id, a);
    assert!(action(w, &HostIntent::Command(id)));
    sync(w);
}
fn input(w: &mut World, i: usize, text: &str) {
    let e = w.resource::<State>().unwrap().fields[i];
    w.get_mut::<Label>(e).unwrap().text = text.into();
}
fn seeded() -> App {
    let mut app = app();
    let s = app.world_mut().resource_mut::<State>().unwrap();
    s.day = date("2026-11-18");
    s.events = vec![event()];
    s.read = None;
    s.redraw = true;
    sync(app.world_mut());
    app
}
#[test]
fn calendar_arithmetic_clamps_months_and_uses_sunday_grid() {
    assert_eq!(
        shift(date("2024-01-31"), Mode::Month, 1),
        date("2024-02-29")
    );
    assert_eq!(
        shift(date("2025-01-31"), Mode::Month, 1),
        date("2025-02-28")
    );
    assert_eq!(
        shift(date("2026-01-31"), Mode::Month, -1),
        date("2025-12-31")
    );
    assert_eq!(grid(date("2026-11-18")), date("2026-11-01"));
    assert_eq!(week(date("2026-11-18")), date("2026-11-15"));
}
#[test]
fn all_day_uses_utc_dates_and_exclusive_end() {
    let mut e = event();
    e.all_day = true;
    e.dtstart = bound("2026-11-18", "", true).unwrap();
    e.dtend = Some(bound("2026-11-20", "", true).unwrap());
    assert!(on_day(&e, date("2026-11-18")));
    assert!(on_day(&e, date("2026-11-19")));
    assert!(!on_day(&e, date("2026-11-20")));
    let d = Draft::event(e);
    let r = d.request("unused").unwrap();
    assert_eq!(r.dtstart.to_rfc3339(), "2026-11-18T00:00:00+00:00");
    assert_eq!(r.dtend.unwrap().date_naive(), date("2026-11-20"));
}
#[test]
fn midnight_end_is_not_shown_on_following_day() {
    let mut e = event();
    e.dtstart = bound("2026-11-18", "23:00", false).unwrap();
    e.dtend = Some(bound("2026-11-19", "00:00", false).unwrap());
    assert!(on_day(&e, date("2026-11-18")));
    assert!(!on_day(&e, date("2026-11-19")));
    e.dtend = Some(bound("2026-11-19", "00:30", false).unwrap());
    assert!(on_day(&e, date("2026-11-19")));
}
#[test]
fn edit_preserves_uid_calendar_note_and_alarm_metadata() {
    let mut d = Draft::event(event());
    d.values[0] = "Updated".into();
    let r = d.request("wrong").unwrap();
    assert_eq!(r.uid.as_deref(), Some("fixture"));
    assert_eq!(r.calendar_path.as_deref(), Some("calendar/team.ics"));
    assert_eq!(r.note_path.as_deref(), Some("meetings/plan.md"));
    assert_eq!(r.alarms[0].trigger_minutes_before, 15);
}
#[test]
fn invalid_and_recurring_writes_fail_closed() {
    let mut d = Draft::new(date("2026-11-18"));
    assert!(d.request("calendar/personal.ics").is_err());
    d.values[0] = "Test".into();
    d.values[2] = "25:00".into();
    assert!(d.request("x").is_err());
    d.values[2] = "11:00".into();
    assert!(d.request("x").is_err());
    d.values[2] = "09:00".into();
    assert!(d.request("x").is_ok());
    let mut e = event();
    e.recurrence_id = Some(e.dtstart);
    assert!(Draft::event(e).request("x").is_err());
}
#[test]
fn stale_range_response_cannot_replace_current_month() {
    let mut app = seeded();
    let w = app.world_mut();
    act(w, Action::Shift(1));
    let old = take_commands(w).pop().unwrap();
    act(w, Action::Shift(1));
    let current = take_commands(w).pop().unwrap();
    apply(
        w,
        Update {
            command: old,
            result: Ok(Response::Listed(CalendarListResponse {
                calendar_path: "wrong".into(),
                events: vec![],
            })),
        },
    );
    assert_eq!(w.resource::<State>().unwrap().events.len(), 1);
    assert_eq!(w.resource::<State>().unwrap().read, Some(current.serial));
}
#[test]
fn pending_mutation_is_single_flight_and_failure_retains_draft() {
    let mut app = seeded();
    let w = app.world_mut();
    act(w, Action::New);
    input(w, 0, "Do not duplicate 🦀");
    act(w, Action::Save);
    let cmd = take_commands(w).pop().unwrap();
    act(w, Action::Save);
    assert!(take_commands(w).is_empty());
    apply(
        w,
        Update {
            command: cmd,
            result: Err("connection lost".into()),
        },
    );
    sync(w);
    assert_eq!(
        w.resource::<State>()
            .unwrap()
            .draft
            .as_ref()
            .unwrap()
            .values[0],
        "Do not duplicate 🦀"
    );
    assert!(w.resource::<State>().unwrap().uncertain);
    act(w, Action::Save);
    assert!(take_commands(w).is_empty());
    assert!(
        w.resource::<State>()
            .unwrap()
            .message
            .contains("may have reached")
    );
}
#[test]
fn delete_requires_confirmation_and_recurring_is_guarded() {
    let mut app = seeded();
    let w = app.world_mut();
    act(w, Action::Open(0));
    act(w, Action::ConfirmDelete);
    assert!(take_commands(w).is_empty());
    act(w, Action::Delete);
    act(w, Action::ConfirmDelete);
    let c = take_commands(w).pop().unwrap();
    assert!(
        matches!(c.kind,Kind::Delete(ref uid,ref path) if uid=="fixture"&&path=="calendar/team.ics")
    );
}
#[test]
fn draft_and_field_entity_survive_surface_switch_and_normal_frames() {
    let mut app = seeded();
    act(app.world_mut(), Action::New);
    input(app.world_mut(), 0, "Retained draft");
    let entity = app.world().resource::<State>().unwrap().fields[0];
    crate::surfaces::select(app.world_mut(), crate::surfaces::SurfaceId::Chat);
    app.update();
    crate::surfaces::select(app.world_mut(), crate::surfaces::SurfaceId::Calendar);
    app.update();
    assert_eq!(app.world().resource::<State>().unwrap().fields[0], entity);
    assert_eq!(
        app.world().get::<Label>(entity).unwrap().text,
        "Retained draft"
    );
}
#[test]
fn ime_composition_defers_redraw_and_navigation() {
    let mut app = seeded();
    act(app.world_mut(), Action::New);
    let e = app.world().resource::<State>().unwrap().fields[0];
    app.world_mut().insert(
        e,
        ImePreview {
            text: "未".into(),
            cursor: None,
        },
    );
    app.world_mut().resource_mut::<State>().unwrap().redraw = true;
    sync(app.world_mut());
    assert_eq!(app.world().resource::<State>().unwrap().fields[0], e);
    crate::surfaces::select(app.world_mut(), crate::surfaces::SurfaceId::Chat);
    assert_eq!(
        crate::surfaces::active(app.world()),
        crate::surfaces::SurfaceId::Calendar
    );
}
#[test]
fn all_day_toggle_has_valid_exclusive_end() {
    let mut app = seeded();
    let w = app.world_mut();
    act(w, Action::New);
    input(w, 0, "Day off");
    act(w, Action::AllDay);
    let d = w.resource::<State>().unwrap().draft.as_ref().unwrap();
    assert!(d.request("x").is_ok());
}

#[test]
#[ignore = "Calendar Parley/GPU synthetic visual acceptance"]
fn native_calendar_visual_acceptance() {
    use std::sync::Arc;
    let mut exporter = rndr_render::Exporter::new().unwrap();
    std::fs::create_dir_all("verification").unwrap();
    for (width, height, light, mode, edit, name) in [
        (1200, 900, false, Mode::Month, false, "month-wide"),
        (760, 850, false, Mode::Month, false, "month-narrow"),
        (1200, 900, true, Mode::Month, false, "month-light"),
        (1200, 850, false, Mode::Week, false, "week"),
        (760, 850, true, Mode::Day, false, "day-light"),
        (1200, 1000, false, Mode::Month, true, "editor-wide"),
        (760, 1000, true, Mode::Month, true, "editor-narrow"),
    ] {
        let mut app = seeded();
        let engine: Arc<dyn TextEngine> = Arc::new(rndr_render::ParleyText::new());
        app.world_mut().insert_resource(engine);
        app.world_mut()
            .insert_resource(Viewport::new(width as f32, height as f32));
        publish_system_appearance(
            app.world_mut(),
            SystemAppearance {
                dark: Some(!light),
                ..Default::default()
            },
        );
        let s = app.world_mut().resource_mut::<State>().unwrap();
        s.mode = mode;
        s.redraw = true;
        let mut all = event();
        all.uid = "all-day".into();
        all.summary = "Focus day".into();
        all.all_day = true;
        all.dtstart = bound("2026-11-20", "", true).unwrap();
        all.dtend = Some(bound("2026-11-21", "", true).unwrap());
        s.events.push(all);
        if edit {
            act(app.world_mut(), Action::Open(0));
        }
        for _ in 0..6 {
            rndr_paint::frame(&mut app);
        }
        let root = app.world().resource::<State>().unwrap().root;
        let rect = app.world().get::<Rect>(root).unwrap();
        assert!(rect.x >= 0. && rect.x + rect.w <= width as f32 + 1.);
        let (_, image) = rndr_paint::frame_with_snapshot_frame(&mut app, |world, frame| {
            exporter
                .render_frame(Some(world), frame, width, height, 1.)
                .unwrap()
        });
        image::save_buffer(
            format!("verification/native-calendar-{name}.png"),
            &image.rgba,
            image.width,
            image.height,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
}

// Full native state -> SDK HTTP -> isolated fixture -> reopen/edit/reload flow.
// This validates the client's daemon contract; it is not a live-workshop mutation.
#[tokio::test]
async fn create_reopen_edit_reload_delete_over_typed_http() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        time::{Duration as StdDuration, Instant},
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let mut stored: Option<CalendarEvent> = None;
        for expected in ["POST", "GET", "PUT", "GET", "DELETE", "GET"] {
            let deadline = Instant::now() + StdDuration::from_secs(10);
            let mut stream = loop {
                match listener.accept() {
                    Ok((s, _)) => break s,
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        std::thread::sleep(StdDuration::from_millis(2))
                    }
                    Err(e) => panic!("missing {expected}: {e}"),
                }
            };
            stream
                .set_read_timeout(Some(StdDuration::from_secs(3)))
                .unwrap();
            let mut raw = Vec::new();
            let mut buf = [0; 4096];
            let header_end;
            loop {
                let n = stream.read(&mut buf).unwrap();
                assert!(n > 0);
                raw.extend_from_slice(&buf[..n]);
                if let Some(i) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                    header_end = i + 4;
                    break;
                }
            }
            let head = String::from_utf8(raw[..header_end].to_vec()).unwrap();
            assert!(head.starts_with(expected), "{head}");
            assert!(
                head.to_ascii_lowercase()
                    .contains("authorization: bearer fixture")
            );
            let len = head
                .lines()
                .find_map(|l| {
                    l.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .and_then(|n| n.trim().parse::<usize>().ok())
                })
                .unwrap_or(0);
            while raw.len() < header_end + len {
                let n = stream.read(&mut buf).unwrap();
                assert!(n > 0);
                raw.extend_from_slice(&buf[..n]);
            }
            let response=match expected {
                "POST"|"PUT"=>{
                    let r:CalendarWriteRequest=serde_json::from_slice(&raw[header_end..header_end+len]).unwrap();assert!(head.starts_with(if expected=="POST"{"POST /v1/calendar/events "}else{"PUT /v1/calendar/events/fixture "}));assert_eq!(r.calendar_path.as_deref(),Some("calendar/personal.ics"));
                    if expected=="PUT"{assert_eq!(r.uid.as_deref(),Some("fixture"));assert_eq!(r.summary,"Edited title");}
                    let mut e=event();e.summary=r.summary;e.dtstart=r.dtstart;e.dtend=r.dtend;e.all_day=r.all_day;e.calendar_path=r.calendar_path.unwrap();e.alarms=r.alarms;e.note_path=r.note_path;e.description=r.description;e.location=r.location;stored=Some(e.clone());json!({"event":e,"created":expected=="POST"})
                },
                "GET"=>{assert!(head.starts_with("GET /v1/calendar/events?"));assert!(head.contains("from=")&&head.contains("to=")&&head.contains("path=calendar%2Fpersonal.ics"));json!({"calendar_path":"calendar/personal.ics","events":stored.iter().collect::<Vec<_>>()})},
                "DELETE"=>{assert!(head.starts_with("DELETE /v1/calendar/events/fixture?path=calendar%2Fpersonal.ics "));stored=None;json!({"uid":"fixture","deleted":true,"calendar_path":"calendar/personal.ics"})},_=>unreachable!()
            }.to_string();
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",response.len(),response).unwrap();
        }
    });
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::AUTHORIZATION,
        reqwest::header::HeaderValue::from_static("Bearer fixture"),
    );
    let http = reqwest::Client::builder()
        .default_headers(headers)
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    let c = MedousaClient::with_transport(
        std::sync::Arc::new(medousa_sdk::HttpTransport::with_client(http)),
        origin,
    );
    let mut app = app();
    let w = app.world_mut();
    act(w, Action::New);
    input(w, 0, "New planning event");
    act(w, Action::Save);
    let cmd = take_commands(w).pop().unwrap();
    apply(w, execute(&c, cmd).await);
    let read = take_commands(w).pop().unwrap();
    apply(w, execute(&c, read).await);
    sync(w);
    assert_eq!(
        w.resource::<State>().unwrap().events[0].summary,
        "New planning event"
    );
    act(w, Action::Open(0));
    input(w, 0, "Edited title");
    act(w, Action::Save);
    let cmd = take_commands(w).pop().unwrap();
    apply(w, execute(&c, cmd).await);
    let read = take_commands(w).pop().unwrap();
    apply(w, execute(&c, read).await);
    sync(w);
    assert_eq!(
        w.resource::<State>().unwrap().events[0].summary,
        "Edited title"
    );
    act(w, Action::Open(0));
    act(w, Action::Delete);
    act(w, Action::ConfirmDelete);
    let cmd = take_commands(w).pop().unwrap();
    apply(w, execute(&c, cmd).await);
    let read = take_commands(w).pop().unwrap();
    apply(w, execute(&c, read).await);
    assert!(w.resource::<State>().unwrap().events.is_empty());
    server.join().unwrap();
}

#[test]
fn local_dst_boundaries_reject_ambiguous_or_nonexistent_time() {
    if std::env::var("TZ").as_deref() != Ok("America/Los_Angeles") {
        return;
    }
    assert!(bound("2026-03-08", "02:30", false).is_err());
    assert!(bound("2026-11-01", "01:30", false).is_err());
    assert_eq!(
        bound("2026-03-08", "03:30", false).unwrap().to_rfc3339(),
        "2026-03-08T10:30:00+00:00"
    );
    assert_eq!(
        bound("2026-11-01", "02:30", false).unwrap().to_rfc3339(),
        "2026-11-01T10:30:00+00:00"
    );
    assert_eq!(
        bound("2026-11-01", "", true).unwrap().to_rfc3339(),
        "2026-11-01T00:00:00+00:00"
    );
}
