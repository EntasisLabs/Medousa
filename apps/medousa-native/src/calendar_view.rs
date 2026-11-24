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
    /// Fill at rest: the plane the button sits on.
    rest: Token,
    /// Selected paints the accent, as the mini month's chosen day does.
    accent: bool,
}
impl Component for SolidButton {}
/// Days outside the shown month read quieter than the month's own.
struct Muted;
impl Component for Muted {}
fn solid(w: &mut World, h: controls::ControlHandle, selected: bool) {
    solid_on(w, h, selected, visual::canvas(), false);
}
fn solid_on(w: &mut World, h: controls::ControlHandle, selected: bool, rest: Token, accent: bool) {
    w.insert(
        h.root,
        SolidButton {
            label: h.label,
            selected,
            rest,
            accent,
        },
    );
}
pub(super) fn chrome(w: &mut World) {
    let buttons: Vec<_> = w
        .query::<SolidButton>()
        .iter()
        .map(|(root, b)| (*root, b.label, b.selected, b.rest, b.accent))
        .collect();
    let hover = w
        .resource::<Input>()
        .and_then(|input| hit_test(w, input.pointer.position[0], input.pointer.position[1]));
    let theme = w
        .resource::<rndr_ui::UiTheme>()
        .cloned()
        .unwrap_or_default();
    for (root, label, selected, rest, accent) in buttons {
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
        } else if selected && accent {
            Token::Accent
        } else if selected {
            Token::Selected
        } else {
            rest
        };
        let face = w.get::<Parent>(label).unwrap().entity;
        let border = w.get::<Parent>(face).unwrap().entity;
        for entity in [border, face] {
            visual::plane(w, entity, fill);
        }
        let typography = w.get::<rndr_ui::UiStyle>(label).and_then(|s| s.typography);
        w.insert(
            label,
            rndr_ui::UiStyle {
                typography,
                foreground: Some(
                    if !enabled {
                        Token::Disabled
                    } else if high_press {
                        Token::Canvas
                    } else if selected && accent && !pressed && !hovered {
                        Token::OnAccent
                    } else if w.has::<Muted>(root) && !selected {
                        Token::Secondary
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
/// A day number uses its whole round face and is never elided.
fn numeral(w: &mut World, h: controls::ControlHandle) {
    let label = h.label;
    // Every layer between the root and the label spans the whole face, so the
    // selected and today fills are round, with the number centred inside.
    let mut node = w.get::<Parent>(label).map(|p| p.entity);
    while let Some(e) = node.filter(|&e| e != h.root) {
        if let Some(layout) = w.get_mut::<Layout>(e) {
            layout.width = Size::Fill;
            layout.height = Size::Fill;
        }
        w.insert(e, Align::center());
        node = w.get::<Parent>(e).map(|p| p.entity);
    }
    w.insert(h.root, Align::center());
    w.insert(
        label,
        LabelBox {
            inset_x: 0.,
            inset_y: 0.,
            min_height: 0.,
        },
    );
    if let Some(l) = w.get_mut::<Label>(label) {
        l.overflow = TextOverflow::Visible;
    }
}
/// A text action resting on the sidebar's navigation plane.
fn side_button(
    w: &mut World,
    p: Entity,
    s: &mut State,
    name: &str,
    a: Action,
    on: bool,
    enabled: bool,
) {
    let id = next();
    let h = visual::document_action(w, p, id, name, enabled);
    solid_on(w, h, on, visual::navigation(), false);
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
    numeral(w, h);
    w.insert(h.root, Radius(14.));
    if let Some(sem) = w.get_mut::<Semantics>(h.root) {
        sem.name = Some(format!("Select {}", day.format("%A, %B %e, %Y")));
    }
    s.actions.insert(id, Action::Day(day));
}
fn time(e: &CalendarEvent) -> String {
    if e.all_day {
        "All day".into()
    } else {
        e.dtstart.with_timezone(&Local).format("%H:%M").to_string()
    }
}
fn event(w: &mut World, p: Entity, s: &mut State, i: usize, compact: bool) {
    let e = &s.events[i];
    let time = time(e);
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
fn tool(
    w: &mut World,
    p: Entity,
    s: &mut State,
    icon: Icon,
    name: &str,
    a: Action,
    on: bool,
    enabled: bool,
) {
    let id = next();
    let h = visual::icon_action(w, p, id, icon, name, enabled);
    solid_on(w, h, on, visual::navigation(), false);
    s.actions.insert(id, a);
}
fn centered(w: &mut World, p: Entity, height: f32) -> Entity {
    let e = panel(
        w,
        p,
        Layout {
            width: Size::Fill,
            height: Size::Px(height),
            ..Layout::column()
        },
    );
    w.insert(
        e,
        Align {
            items: Items::Center,
            justify: Justify::Center,
        },
    );
    e
}
/// The visible period, the same span [`State::refresh`] loads.
fn period(s: &State) -> (NaiveDate, i64) {
    match s.mode {
        Mode::Month => (grid(s.day), 42),
        Mode::Week => (week(s.day), 7),
        Mode::Day => (s.day, 1),
    }
}
fn mini_day(w: &mut World, p: Entity, s: &mut State, day: NaiveDate, busy: bool) {
    let id = next();
    let mut props = ButtonProps::new(day.day().to_string(), HostIntent::Command(id));
    props.enabled = s.draft.is_none() && s.mutation.is_none();
    props.variant = controls::ButtonVariant::Quiet;
    let h = controls::Button::mount(w, p, UiId(id), props).unwrap();
    let today = day == Local::now().date_naive();
    let selected = day == s.day;
    solid_on(w, h, selected || today, visual::navigation(), selected);
    w.insert(
        h.label,
        rndr_ui::UiStyle {
            typography: Some(if busy || today {
                Typography::Emphasis
            } else {
                Typography::Caption
            }),
            ..Default::default()
        },
    );
    if day.month() != s.day.month() {
        w.insert(h.root, Muted);
    }
    *w.get_mut::<Layout>(h.root).unwrap() = Layout {
        width: Size::Px(30.),
        height: Size::Px(30.),
        ..Layout::column()
    };
    numeral(w, h);
    w.insert(h.root, Radius(15.));
    if let Some(sem) = w.get_mut::<Semantics>(h.root) {
        sem.name = Some(format!("Select {}", day.format("%A, %B %e, %Y")));
    }
    s.actions.insert(id, Action::Day(day));
}
/// Medousa-home's calendar rail: tools, a mini month and the agenda for the
/// visible period. Search narrows the agenda as well as the main view.
fn sidebar(w: &mut World, p: Entity, s: &mut State) {
    let enabled = s.draft.is_none() && s.mutation.is_none();
    let tools = panel(
        w,
        p,
        Layout {
            width: Size::Fill,
            height: Size::Px(visual::ACTION_SIZE),
            gap: 2.,
            ..Layout::row()
        },
    );
    w.insert(
        tools,
        Align {
            items: Items::Center,
            ..Default::default()
        },
    );
    let title = text(w, tools, "Calendar", Typography::Emphasis).unwrap();
    w.get_mut::<Layout>(title).unwrap().width = Size::Fill;
    tool(
        w,
        tools,
        s,
        Icon::Plus,
        "New event",
        Action::New,
        false,
        enabled,
    );
    tool(
        w,
        tools,
        s,
        Icon::RefreshCw,
        "Refresh",
        Action::Refresh,
        false,
        true,
    );
    let searching = s.show_search;
    tool(
        w,
        tools,
        s,
        Icon::Search,
        "Search",
        Action::ToggleSearch,
        searching,
        enabled,
    );
    if s.draft.is_none() && s.show_search {
        let search_bar = col(w, p);
        s.search_field = Some(field(
            w,
            search_bar,
            "Search title, location, description · current view only",
            &s.search_input,
            false,
            enabled,
        ));
        let search_actions = row(w, search_bar);
        side_button(
            w,
            search_actions,
            s,
            "Search",
            Action::Search,
            false,
            enabled,
        );
        side_button(
            w,
            search_actions,
            s,
            "Clear search",
            Action::ClearSearch,
            false,
            enabled,
        );
    }
    let month = panel(
        w,
        p,
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            gap: 2.,
            ..Layout::column()
        },
    );
    let heading = panel(
        w,
        month,
        Layout {
            width: Size::Fill,
            height: Size::Px(30.),
            ..Layout::row()
        },
    );
    w.insert(
        heading,
        Align {
            items: Items::Center,
            ..Default::default()
        },
    );
    let name = text(
        w,
        heading,
        &s.day.format("%B %Y").to_string(),
        Typography::Label,
    )
    .unwrap();
    w.get_mut::<Layout>(name).unwrap().width = Size::Fill;
    side_button(w, heading, s, "Today", Action::Today, false, enabled);
    let weekdays = panel(
        w,
        month,
        Layout {
            width: Size::Fill,
            height: Size::Px(22.),
            ..Layout::row()
        },
    );
    for letter in ["S", "M", "T", "W", "T", "F", "S"] {
        let cell = centered(w, weekdays, 22.);
        text(w, cell, letter, Typography::Caption).unwrap();
    }
    let start = grid(s.day);
    let (from, count) = period(s);
    for r in 0..6 {
        let line = panel(
            w,
            month,
            Layout {
                width: Size::Fill,
                height: Size::Px(32.),
                ..Layout::row()
            },
        );
        for c in 0..7 {
            let day = start + Duration::days(r * 7 + c);
            let cell = centered(w, line, 32.);
            let loaded = day >= from && day < from + Duration::days(count);
            let busy = loaded && s.events.iter().any(|e| on_day(e, day));
            mini_day(w, cell, s, day, busy);
        }
    }
    visual::rule(w, p);
    let list = panel(
        w,
        p,
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            gap: 2.,
            ..Layout::column()
        },
    );
    w.insert(list, Scroll::default());
    let mut buckets = 0;
    for offset in 0..count {
        let day = from + Duration::days(offset);
        let indices: Vec<_> = s
            .events
            .iter()
            .enumerate()
            .filter(|(_, e)| on_day(e, day) && (s.search.is_empty() || search_matches(s, e)))
            .map(|(i, _)| i)
            .collect();
        if indices.is_empty() {
            continue;
        }
        let heading = panel(
            w,
            list,
            Layout {
                width: Size::Fill,
                height: Size::Px(26.),
                padding: 4.,
                ..Layout::row()
            },
        );
        w.insert(
            heading,
            Align {
                items: Items::End,
                ..Default::default()
            },
        );
        text(
            w,
            heading,
            &day.format("%a, %b %-d").to_string().to_uppercase(),
            Typography::Caption,
        )
        .unwrap();
        for i in indices {
            let id = next();
            let e = &s.events[i];
            let mut props = ButtonProps::new(
                format!("{}   {}", time(e), e.summary),
                HostIntent::Command(id),
            );
            props.enabled = enabled;
            props.variant = controls::ButtonVariant::Quiet;
            let h = controls::Button::mount(w, list, UiId(id), props).unwrap();
            *w.get_mut::<Layout>(h.root).unwrap() = Layout {
                width: Size::Fill,
                height: Size::Px(30.),
                ..Layout::row()
            };
            w.get_mut::<Label>(h.label).unwrap().overflow = TextOverflow::Ellipsis;
            solid_on(w, h, false, visual::navigation(), false);
            w.insert(h.root, Radius(6.));
            s.actions.insert(id, Action::Open(i));
        }
        buckets += 1;
        if buckets >= 14 {
            break;
        }
    }
    if buckets == 0 {
        let empty = centered(w, list, 120.);
        w.get_mut::<Layout>(empty).unwrap().gap = 8.;
        text(
            w,
            empty,
            if s.search.is_empty() {
                "No events in this range"
            } else {
                "No events match"
            },
            Typography::Secondary,
        )
        .unwrap();
    }
    let footer = row(w, p);
    let source = s.show_source;
    side_button(
        w,
        footer,
        s,
        "Calendar",
        Action::ToggleSource,
        source,
        enabled,
    );
    if s.show_source {
        label(w, p, &s.path.clone(), Typography::Caption);
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
    let panes = children_of(w, s.root);
    let (side, root) = (panes[0], panes[1]);
    for e in children_of(w, side).into_iter().chain(children_of(w, root)) {
        despawn(w, e);
    }
    s.actions.clear();
    s.fields.clear();
    s.search_field = None;
    *w.get_mut::<Layout>(root).unwrap() = Layout {
        width: Size::Fill,
        height: Size::Fill,
        min_width: 0.,
        padding: if s.narrow { 12. } else { 24. },
        gap: 8.,
        ..Layout::column()
    };
    sidebar(w, side, s);
    let header = panel(
        w,
        root,
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
    let tools = row(w, root);
    for (name, a) in [
        ("‹", Action::Shift(-1)),
        ("Today", Action::Today),
        ("›", Action::Shift(1)),
    ] {
        button(w, tools, s, name, a, enabled);
    }
    let modes = if s.narrow { tools } else { header };
    for (name, mode) in [
        ("Day", Mode::Day),
        ("Week", Mode::Week),
        ("Month", Mode::Month),
    ] {
        button(w, modes, s, name, Action::Mode(mode), enabled);
    }
    if s.read.is_some() {
        label(w, root, "Loading…", Typography::Secondary);
    }
    if !s.message.is_empty() {
        label(w, root, &s.message.clone(), Typography::Secondary);
    }
    let body = panel(
        w,
        root,
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
