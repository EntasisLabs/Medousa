use super::*;
use crate::native_style as visual;
use rndr_ui::{
    Token, Typography, UiId,
    controls::{self, ButtonProps, NavigationMeta},
    text,
};
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
    visual::document_action(w, p, id, name, enabled);
    s.actions.insert(id, a);
}
fn day_button(w: &mut World, p: Entity, s: &mut State, day: NaiveDate) {
    let id = next();
    let mut props = ButtonProps::new(day.day().to_string(), HostIntent::Command(id));
    props.enabled = s.draft.is_none() && s.mutation.is_none();
    let h = controls::NavigationRow::mount(
        w,
        p,
        UiId(id),
        props,
        NavigationMeta {
            selected: day == s.day,
            ..Default::default()
        },
    )
    .unwrap();
    *w.get_mut::<Layout>(h.root).unwrap() = Layout {
        width: Size::Fill,
        height: Size::Px(30.),
        ..Layout::column()
    };
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
        height: Size::Px(if compact { 27. } else { 38. }),
        ..Layout::row()
    };
    w.get_mut::<Label>(h.label).unwrap().overflow = TextOverflow::Ellipsis;
    visual::plane(w, h.root, Token::Selected);
    w.insert(h.root, Radius(4.));
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
    if recurring {
        label(
            w,
            p,
            "Recurring occurrence · read-only. Series editing will follow in a later parity pass.",
            Typography::Secondary,
        );
    }
    if let Some(e) = &d.original {
        if !e.alarms.is_empty() {
            label(
                w,
                p,
                &format!(
                    "{} existing alarm(s) preserved on save. Native notification delivery is not enabled yet.",
                    e.alarms.len()
                ),
                Typography::Secondary,
            );
        }
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
    *w.get_mut::<Layout>(s.root).unwrap() = Layout {
        width: Size::Fill,
        height: Size::Fill,
        padding: if s.narrow { 12. } else { 24. },
        gap: 16.,
        ..Layout::column()
    };
    let header = row(w, s.root);
    label(
        w,
        header,
        &format!("Calendar · {}", s.day.format("%B %Y")),
        Typography::Emphasis,
    );
    let tools = row(w, s.root);
    let enabled = s.draft.is_none() && s.mutation.is_none();
    for (name, a) in [
        ("Previous", Action::Shift(-1)),
        ("Today", Action::Today),
        ("Next", Action::Shift(1)),
        ("Day", Action::Mode(Mode::Day)),
        ("Week", Action::Mode(Mode::Week)),
        ("Month", Action::Mode(Mode::Month)),
        ("New event", Action::New),
        ("Refresh", Action::Refresh),
    ] {
        button(w, tools, s, name, a, enabled);
    }
    label(
        w,
        s.root,
        &format!(
            "{} · {}{}",
            s.path,
            match s.mode {
                Mode::Month => "Month",
                Mode::Week => "Week agenda",
                Mode::Day => "Day agenda",
            },
            if s.read.is_some() {
                " · Loading…"
            } else {
                ""
            }
        ),
        Typography::Secondary,
    );
    if !s.message.is_empty() {
        label(w, s.root, &s.message.clone(), Typography::Secondary);
    }
    let body = panel(
        w,
        s.root,
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            gap: 20.,
            ..Layout::row()
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
    w.insert(main, Scroll::default());
    match s.mode {
        Mode::Month => {
            let start = grid(s.day);
            let weekdays = panel(
                w,
                main,
                Layout {
                    width: Size::Fill,
                    height: Size::Hug,
                    gap: 4.,
                    ..Layout::row()
                },
            );
            for name in ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"] {
                let c = col(w, weekdays);
                label(w, c, name, Typography::Secondary);
            }
            for r in 0..6 {
                let line = panel(
                    w,
                    main,
                    Layout {
                        width: Size::Fill,
                        height: Size::Px(if s.narrow { 78. } else { 98. }),
                        gap: 4.,
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
                        if day == Local::now().date_naive() {
                            Token::Selected
                        } else {
                            visual::raised()
                        },
                    );
                    w.insert(cell, Radius(5.));
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
                        label(
                            w,
                            cell,
                            &format!("+{} more", indices.len() - limit),
                            Typography::Secondary,
                        );
                    }
                }
            }
            if s.narrow {
                visual::rule(w, main);
                agenda(w, main, s, s.day);
            } else {
                let rail = panel(
                    w,
                    body,
                    Layout {
                        width: Size::Px(250.),
                        height: Size::Fill,
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
