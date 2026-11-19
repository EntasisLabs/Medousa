use super::*;
use crate::native_style as visual;
use rndr_ui::{
    Token, Typography, UiId,
    controls::{self, ButtonProps},
    text,
};
/// Paint the actual control face, not a selected plane behind RNDR's inset
/// quiet button. Like the navigation rail, retain its root/gap focus layers.
struct SolidButton {
    label: Entity,
    selected: bool,
}
impl Component for SolidButton {}
fn solid(w: &mut World, h: controls::ControlHandle, selected: bool) {
    w.insert(
        h.root,
        SolidButton {
            label: h.label,
            selected,
        },
    );
}
pub(super) fn chrome(w: &mut World) {
    let buttons: Vec<_> = w
        .query::<SolidButton>()
        .iter()
        .map(|(root, b)| (*root, b.label, b.selected))
        .collect();
    let hover = w
        .resource::<Input>()
        .and_then(|input| hit_test(w, input.pointer.position[0], input.pointer.position[1]));
    let theme = w
        .resource::<rndr_ui::UiTheme>()
        .cloned()
        .unwrap_or_default();
    for (root, label, selected) in buttons {
        let enabled = !w.has::<ControlDisabled>(root);
        let pressed = enabled && interaction_pressed(w, root);
        let hovered = enabled && hover.is_some_and(|e| crate::surfaces::descendant(w, e, root));
        let high_press = pressed && theme.contrast == rndr_ui::Contrast::High;
        let fill = if high_press {
            Token::Text
        } else if pressed {
            Token::ControlPressed
        } else if hovered {
            Token::ControlHover
        } else if selected {
            Token::Selected
        } else {
            visual::canvas()
        };
        let face = w.get::<Parent>(label).unwrap().entity;
        let border = w.get::<Parent>(face).unwrap().entity;
        for entity in [border, face] {
            visual::plane(w, entity, fill);
        }
        w.insert(
            label,
            rndr_ui::UiStyle {
                foreground: Some(
                    if !enabled {
                        Token::Disabled
                    } else if high_press {
                        Token::Canvas
                    } else {
                        Token::Text
                    }
                    .into(),
                ),
                ..Default::default()
            },
        );
    }
}
fn panel(w: &mut World, p: Entity, l: Layout) -> Entity {
    let e = w.spawn((l,));
    set_parent(w, e, p);
    e
}
fn col(w: &mut World, p: Entity) -> Entity {
    panel(
        w,
        p,
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 8.,
            ..Layout::column()
        },
    )
}
fn row(w: &mut World, p: Entity) -> Entity {
    let e = panel(
        w,
        p,
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 6.,
            ..Layout::row()
        },
    );
    w.insert(e, Wrap { line_gap: 6. });
    e
}
fn label(w: &mut World, p: Entity, value: &str, kind: Typography) {
    let e = text(w, p, value, kind).unwrap();
    w.get_mut::<Layout>(e).unwrap().width = Size::Fill;
    w.get_mut::<Label>(e).unwrap().overflow = TextOverflow::Wrap;
}
fn button(w: &mut World, p: Entity, s: &mut State, name: &str, a: Action, enabled: bool) {
    let id = next();
    let h = visual::document_action(w, p, id, name, enabled);
    solid(w, h, matches!(&a, Action::Mode(m) if *m == s.mode));
    if let Some(sem) = w.get_mut::<Semantics>(h.root) {
        sem.name = Some(match &a {
            Action::Shift(-1) => "Previous period".into(),
            Action::Shift(1) => "Next period".into(),
            _ => name.into(),
        });
    }
    s.actions.insert(id, a);
}
fn day_button(w: &mut World, p: Entity, s: &mut State, day: NaiveDate) {
    let id = next();
    let mut props = ButtonProps::new(day.day().to_string(), HostIntent::Command(id));
    props.enabled = s.draft.is_none() && s.mutation.is_none();
    props.variant = controls::ButtonVariant::Quiet;
    let h = controls::Button::mount(w, p, UiId(id), props).unwrap();
    if day == Local::now().date_naive() {
        solid(w, h, true);
    }
    if day.month() != s.day.month() {
        w.insert(
            h.label,
            rndr_ui::UiStyle {
                foreground: Some(Token::Secondary.into()),
                ..Default::default()
            },
        );
    }
    *w.get_mut::<Layout>(h.root).unwrap() = Layout {
        width: Size::Px(28.),
        height: Size::Px(26.),
        ..Layout::column()
    };
    w.insert(h.root, Radius(14.));
    if let Some(sem) = w.get_mut::<Semantics>(h.root) {
        sem.name = Some(format!("Select {}", day.format("%A, %B %e, %Y")));
    }
    s.actions.insert(id, Action::Day(day));
}
fn event(w: &mut World, p: Entity, s: &mut State, i: usize, compact: bool) {
    let e = &s.events[i];
    let time = if e.all_day {
        "All day".into()
    } else {
        e.dtstart.with_timezone(&Local).format("%H:%M").to_string()
    };
    let name = format!(
        "{time}  {}{}",
        e.summary,
        if e.rrule.is_some() || e.recurrence_id.is_some() {
            " · repeats"
        } else {
            ""
        }
    );
    let id = next();
    let mut props = ButtonProps::new(&name, HostIntent::Command(id));
    props.enabled = s.draft.is_none() && s.mutation.is_none();
    props.variant = controls::ButtonVariant::Quiet;
    let h = controls::Button::mount(w, p, UiId(id), props).unwrap();
    *w.get_mut::<Layout>(h.root).unwrap() = Layout {
        width: Size::Fill,
        height: Size::Px(if compact { 20. } else { 38. }),
        ..Layout::row()
    };
    w.get_mut::<Label>(h.label).unwrap().overflow = TextOverflow::Ellipsis;
    solid(w, h, true);
    w.insert(h.root, Radius(3.));
    s.actions.insert(id, Action::Open(i));
}
fn agenda(w: &mut World, p: Entity, s: &mut State, day: NaiveDate) {
    label(
        w,
        p,
        &day.format("%A, %B %e").to_string(),
        Typography::Emphasis,
    );
    let indices: Vec<_> = s
        .events
        .iter()
        .enumerate()
        .filter(|(_, e)| on_day(e, day))
        .map(|(i, _)| i)
        .collect();
    if indices.is_empty() {
        label(w, p, "No events planned", Typography::Secondary);
    }
    for i in indices {
        event(w, p, s, i, false);
        if let Some(location) = s.events[i].location.clone().filter(|l| !l.is_empty()) {
            label(w, p, &location, Typography::Secondary);
        }
    }
}
fn field(w: &mut World, p: Entity, name: &str, value: &str, multi: bool, enabled: bool) -> Entity {
    label(w, p, name, Typography::Secondary);
    let e = w.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(if multi { 72. } else { 36. }),
            padding: 8.,
            ..Layout::column()
        },
        Label::new(value),
        Semantics {
            name: Some(name.into()),
            ..Default::default()
        },
    ));
    if enabled {
        w.insert(e, Focusable);
        w.insert(
            e,
            Prompt {
                submit_on_enter: false,
                max_lines: if multi { 3 } else { 1 },
                ..Default::default()
            },
        );
        w.insert(e, Placeholder::new(name));
    }
    set_parent(w, e, p);
    visual::plane(w, e, visual::rail());
    w.insert(e, Radius(5.));
    e
}
fn editor(w: &mut World, p: Entity, s: &mut State) {
    let d = s.draft.as_ref().unwrap().clone();
    let busy = s.mutation.is_some();
    let recurring = d
        .original
        .as_ref()
        .is_some_and(|e| e.rrule.is_some() || e.recurrence_id.is_some());
    label(
        w,
        p,
        if d.original.is_some() {
            "Event details"
        } else {
            "New event"
        },
        Typography::Emphasis,
    );
    label(
        w,
        p,
        &format!(
            "Times use this device’s local zone ({})",
            Local::now().format("%Z %:z")
        ),
        Typography::Secondary,
    );
    let names = [
        "Title",
        "Start date · YYYY-MM-DD",
        "Start time · HH:MM",
        "End date · YYYY-MM-DD",
        "End time · HH:MM",
        "Location",
        "Description",
    ];
    for (i, name) in names.iter().enumerate() {
        let holder = col(w, p);
        let e = field(w, holder, name, &d.values[i], i == 6, !busy && !recurring);
        s.fields.push(e);
        if i == 0 {
            let bar = row(w, p);
            button(
                w,
                bar,
                s,
                if d.all_day {
                    "All day · on"
                } else {
                    "All day · off"
                },
                Action::AllDay,
                !busy && !recurring,
            );
        }
        if d.all_day && (i == 2 || i == 4) {
            w.insert(holder, Hidden);
        }
    }
    if d.all_day {
        label(
            w,
            p,
            "All-day dates do not shift with time zones. End date is exclusive (the day after the last event day).",
            Typography::Secondary,
        );
    }
    label(w, p, "Event reminders", Typography::Emphasis);
    label(
        w,
        p,
        "Saved as calendar display alarms. Native notification delivery is not enabled.",
        Typography::Secondary,
    );
    if d.alarms.is_empty() {
        label(w, p, "No reminders", Typography::Secondary);
    }
    for (i, alarm) in d.alarms.iter().enumerate() {
        let bar = row(w, p);
        label(
            w,
            bar,
            &format!(
                "{} minutes before · {}",
                alarm.trigger_minutes_before, alarm.action
            ),
            Typography::Secondary,
        );
        button(
            w,
            bar,
            s,
            "Remove reminder",
            Action::RemoveAlarm(i),
            !busy && !recurring && !s.uncertain,
        );
    }
    label(w, p, "Add reminder before event", Typography::Secondary);
    let presets = row(w, p);
    for (name, minutes) in [
        ("+5 min", 5),
        ("+15 min", 15),
        ("+30 min", 30),
        ("+1 hour", 60),
        ("+1 day", 1440),
        ("+2 days", 2880),
        ("+1 week", 10080),
    ] {
        button(
            w,
            presets,
            s,
            name,
            Action::AddAlarm(minutes),
            !busy
                && !recurring
                && !s.uncertain
                && !d.alarms.iter().any(|a| a.trigger_minutes_before == minutes),
        );
    }
    if recurring {
        label(
            w,
            p,
            "Recurring occurrence · read-only. Series editing will follow in a later parity pass.",
            Typography::Secondary,
        );
    }
    if let Some(e) = &d.original {
        if let Some(path) = &e.note_path {
            label(w, p, &format!("Linked note: {path}"), Typography::Secondary);
        }
    }
    let bar = row(w, p);
    if s.confirm {
        button(
            w,
            bar,
            s,
            "Confirm delete",
            Action::ConfirmDelete,
            !busy && !recurring && !s.uncertain,
        );
        label(
            w,
            p,
            "Delete this event from the workshop calendar?",
            Typography::Secondary,
        );
    } else {
        button(
            w,
            bar,
            s,
            "Save event",
            Action::Save,
            !busy && !recurring && !s.uncertain,
        );
        if d.original.is_some() {
            button(
                w,
                bar,
                s,
                "Delete event",
                Action::Delete,
                !busy && !recurring && !s.uncertain,
            );
        }
    }
    button(
        w,
        bar,
        s,
        if s.uncertain {
            "Close and inspect calendar"
        } else {
            "Cancel"
        },
        Action::Cancel,
        !busy,
    );
}
pub(super) fn render(w: &mut World, s: &mut State) {
    if w.resource::<Focus>()
        .and_then(|f| f.entity)
        .is_some_and(|e| crate::surfaces::descendant(w, e, s.root))
    {
        w.resource_mut::<Focus>().unwrap().entity = None;
    }
    for e in children_of(w, s.root) {
        despawn(w, e);
    }
    s.actions.clear();
    s.fields.clear();
    s.search_field = None;
    *w.get_mut::<Layout>(s.root).unwrap() = Layout {
        width: Size::Fill,
        height: Size::Fill,
        padding: if s.narrow { 12. } else { 24. },
        gap: 8.,
        ..Layout::column()
    };
    let header = panel(
        w,
        s.root,
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 6.,
            ..Layout::row()
        },
    );
    label(
        w,
        header,
        &s.day.format("%B %Y").to_string(),
        Typography::Title,
    );
    let enabled = s.draft.is_none() && s.mutation.is_none();
    if !s.narrow {
        for (name, mode) in [
            ("Day", Mode::Day),
            ("Week", Mode::Week),
            ("Month", Mode::Month),
        ] {
            button(w, header, s, name, Action::Mode(mode), enabled);
        }
        button(w, header, s, "Search", Action::ToggleSearch, enabled);
        button(w, header, s, "+ New event", Action::New, enabled);
    }
    let tools = row(w, s.root);
    for (name, a) in [
        ("‹", Action::Shift(-1)),
        ("Today", Action::Today),
        ("›", Action::Shift(1)),
    ] {
        button(w, tools, s, name, a, enabled);
    }
    if s.narrow {
        for (name, mode) in [
            ("Day", Mode::Day),
            ("Week", Mode::Week),
            ("Month", Mode::Month),
        ] {
            button(w, tools, s, name, Action::Mode(mode), enabled);
        }
        button(w, tools, s, "Search", Action::ToggleSearch, enabled);
        button(w, tools, s, "+ New event", Action::New, enabled);
    }
    button(
        w,
        tools,
        s,
        if s.show_agenda {
            "Hide agenda"
        } else {
            "Agenda"
        },
        Action::ToggleAgenda,
        enabled,
    );
    button(w, tools, s, "Calendar", Action::ToggleSource, enabled);
    button(w, tools, s, "Refresh", Action::Refresh, enabled);
    if s.show_source {
        label(w, s.root, &s.path.clone(), Typography::Caption);
    }
    if s.read.is_some() {
        label(w, s.root, "Loading…", Typography::Secondary);
    }
    if !s.message.is_empty() {
        label(w, s.root, &s.message.clone(), Typography::Secondary);
    }
    if s.draft.is_none() && s.show_search {
        let search_bar = col(w, s.root);
        s.search_field = Some(field(
            w,
            search_bar,
            "Search title, location, description · current view only",
            &s.search_input,
            false,
            enabled,
        ));
        let search_actions = row(w, search_bar);
        button(w, search_actions, s, "Search", Action::Search, enabled);
        button(
            w,
            search_actions,
            s,
            "Clear search",
            Action::ClearSearch,
            enabled,
        );
    }
    let body = panel(
        w,
        s.root,
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            gap: 20.,
            ..if s.narrow && s.show_agenda {
                Layout::column()
            } else {
                Layout::row()
            }
        },
    );
    if s.draft.is_some() {
        let form = col(w, body);
        w.get_mut::<Layout>(form).unwrap().height = Size::Fill;
        w.insert(form, Scroll::default());
        editor(w, form, s);
        return;
    }
    let main = col(w, body);
    w.get_mut::<Layout>(main).unwrap().height = Size::Fill;
    if s.mode != Mode::Month || !s.search.is_empty() {
        w.insert(main, Scroll::default());
    }
    if !s.search.is_empty() {
        label(
            w,
            main,
            &format!(
                "Results for ‘{}’ · current {} view only",
                s.search,
                match s.mode {
                    Mode::Month => "Month",
                    Mode::Week => "Week",
                    Mode::Day => "Day",
                }
            ),
            Typography::Emphasis,
        );
        let indices: Vec<_> = s
            .events
            .iter()
            .enumerate()
            .filter(|(_, e)| search_matches(s, e))
            .map(|(i, _)| i)
            .collect();
        if indices.is_empty() {
            label(
                w,
                main,
                "No matching events in this view",
                Typography::Secondary,
            );
        }
        for i in indices {
            label(
                w,
                main,
                &event_day(&s.events[i]).format("%a, %b %e, %Y").to_string(),
                Typography::Secondary,
            );
            event(w, main, s, i, false);
        }
        return;
    }
    match s.mode {
        Mode::Month => {
            let start = grid(s.day);
            w.get_mut::<Layout>(main).unwrap().gap = 1.;
            visual::plane(w, main, Token::SubtleStroke);
            let weekdays = panel(
                w,
                main,
                Layout {
                    width: Size::Fill,
                    height: Size::Px(26.),
                    gap: 1.,
                    ..Layout::row()
                },
            );
            for name in ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"] {
                let c = col(w, weekdays);
                w.get_mut::<Layout>(c).unwrap().height = Size::Fill;
                visual::plane(w, c, visual::canvas());
                label(w, c, name, Typography::Caption);
            }
            for r in 0..6 {
                let line = panel(
                    w,
                    main,
                    Layout {
                        width: Size::Fill,
                        height: Size::Fill,
                        gap: 1.,
                        ..Layout::row()
                    },
                );
                for c in 0..7 {
                    let day = start + Duration::days(r * 7 + c);
                    let cell = col(w, line);
                    w.get_mut::<Layout>(cell).unwrap().height = Size::Fill;
                    w.get_mut::<Layout>(cell).unwrap().gap = 2.;
                    visual::plane(
                        w,
                        cell,
                        if day == s.day {
                            visual::raised()
                        } else if day.month() != s.day.month() || c == 0 || c == 6 {
                            visual::navigation()
                        } else {
                            visual::canvas()
                        },
                    );
                    w.insert(cell, Radius(0.));
                    day_button(w, cell, s, day);
                    let indices: Vec<_> = s
                        .events
                        .iter()
                        .enumerate()
                        .filter(|(_, e)| on_day(e, day))
                        .map(|(i, _)| i)
                        .collect();
                    let limit = if s.narrow { 1 } else { 2 };
                    for i in indices.iter().take(limit) {
                        event(w, cell, s, *i, true);
                    }
                    if indices.len() > limit {
                        button(
                            w,
                            cell,
                            s,
                            &format!("+{} more", indices.len() - limit),
                            Action::Day(day),
                            enabled,
                        );
                    }
                }
            }
            if s.show_agenda {
                let rail = panel(
                    w,
                    body,
                    Layout {
                        width: if s.narrow { Size::Fill } else { Size::Px(270.) },
                        height: if s.narrow { Size::Px(180.) } else { Size::Fill },
                        padding: 12.,
                        gap: 10.,
                        ..Layout::column()
                    },
                );
                visual::plane(w, rail, visual::navigation());
                w.insert(rail, Scroll::default());
                agenda(w, rail, s, s.day);
            }
        }
        Mode::Week => {
            for i in 0..7 {
                agenda(w, main, s, week(s.day) + Duration::days(i));
                visual::rule(w, main);
            }
        }
        Mode::Day => agenda(w, main, s, s.day),
    }
}
