//! Retained personal calendar. Dates are local for timed events and UTC calendar
//! dates for all-day events. Only the workshop daemon reads/writes calendar files.
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};
use medousa_sdk::MedousaClient;
use medousa_types::{
    CalendarAlarm, CalendarEvent, CalendarListQuery, CalendarListResponse, CalendarWriteRequest,
};
use rndr_core::*;
use rndr_ui::controls::HostIntent;
use std::{
    collections::HashMap,
    sync::atomic::{AtomicU64, Ordering},
};
#[path = "calendar_view.rs"]
mod view;
pub fn chrome(w: &mut World) {
    view::chrome(w);
}
static NEXT: AtomicU64 = AtomicU64::new(0x4341_0000);
fn next() -> u64 {
    NEXT.fetch_add(1, Ordering::Relaxed)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Month,
    Week,
    Day,
}
#[derive(Clone, Debug)]
pub struct Command {
    serial: u64,
    kind: Kind,
}
#[derive(Clone, Debug)]
enum Kind {
    List(CalendarListQuery),
    Save(Box<CalendarWriteRequest>),
    Delete(String, String),
}
pub struct Update {
    command: Command,
    result: Result<Response, String>,
}
enum Response {
    Listed(CalendarListResponse),
    Saved,
    Deleted(bool),
}
#[derive(Clone)]
enum Action {
    Shift(i32),
    Today,
    Mode(Mode),
    Day(NaiveDate),
    New,
    Open(usize),
    AllDay,
    Save,
    Cancel,
    Delete,
    ConfirmDelete,
    Refresh,
    AddAlarm(i32),
    RemoveAlarm(usize),
    Search,
    ClearSearch,
    ToggleSearch,
    ToggleAgenda,
    ToggleSource,
}
#[derive(Clone)]
struct Draft {
    original: Option<CalendarEvent>,
    values: [String; 7],
    all_day: bool,
    alarms: Vec<CalendarAlarm>,
}
// title, start date, start time, end date (exclusive for all-day), end time,
// location, description. Advanced metadata is preserved, not silently cleared.
impl Draft {
    fn new(day: NaiveDate) -> Self {
        Self {
            original: None,
            values: [
                String::new(),
                day.to_string(),
                "09:00".into(),
                day.to_string(),
                "10:00".into(),
                String::new(),
                String::new(),
            ],
            all_day: false,
            alarms: vec![],
        }
    }
    fn event(e: CalendarEvent) -> Self {
        let mut d = Self::new(event_day(&e));
        let end = e.dtend.unwrap_or(
            e.dtstart
                + if e.all_day {
                    Duration::days(1)
                } else {
                    Duration::hours(1)
                },
        );
        d.values = [
            e.summary.clone(),
            event_day(&e).to_string(),
            e.dtstart.with_timezone(&Local).format("%H:%M").to_string(),
            if e.all_day {
                end.date_naive().to_string()
            } else {
                end.with_timezone(&Local).date_naive().to_string()
            },
            end.with_timezone(&Local).format("%H:%M").to_string(),
            e.location.clone().unwrap_or_default(),
            e.description.clone().unwrap_or_default(),
        ];
        d.all_day = e.all_day;
        d.alarms = e.alarms.clone();
        d.original = Some(e);
        d
    }
    fn request(&self, path: &str) -> Result<CalendarWriteRequest, String> {
        if self.values[0].trim().is_empty() {
            return Err("Give the event a title.".into());
        }
        if self
            .original
            .as_ref()
            .is_some_and(|e| e.rrule.is_some() || e.recurrence_id.is_some())
        {
            return Err("Recurring events are read-only in this first native slice. Edit the series in the reference app.".into());
        }
        let start = bound(&self.values[1], &self.values[2], self.all_day)?;
        let end = bound(&self.values[3], &self.values[4], self.all_day)?;
        if end <= start {
            return Err("End must be after start. All-day end dates are exclusive.".into());
        }
        let old = self.original.as_ref();
        Ok(CalendarWriteRequest {
            uid: old.map(|e| e.uid.clone()),
            summary: self.values[0].trim().into(),
            description: nonempty(&self.values[6]),
            location: nonempty(&self.values[5]),
            dtstart: start,
            dtend: Some(end),
            all_day: self.all_day,
            rrule: old.and_then(|e| e.rrule.clone()),
            calendar_path: Some(old.map_or(path, |e| &e.calendar_path).into()),
            note_path: old.and_then(|e| e.note_path.clone()),
            alarms: self.alarms.clone(),
        })
    }
}
fn recurring(d: &Draft) -> bool {
    d.original
        .as_ref()
        .is_some_and(|e| e.rrule.is_some() || e.recurrence_id.is_some())
}
fn search_matches(s: &State, e: &CalendarEvent) -> bool {
    let start = match s.mode {
        Mode::Month => grid(s.day),
        Mode::Week => week(s.day),
        Mode::Day => s.day,
    };
    let days = match s.mode {
        Mode::Month => 42,
        Mode::Week => 7,
        Mode::Day => 1,
    };
    (0..days).any(|i| on_day(e, start + Duration::days(i)))
        && [
            &e.summary,
            e.location.as_deref().unwrap_or(""),
            e.description.as_deref().unwrap_or(""),
        ]
        .iter()
        .any(|text| text.to_lowercase().contains(&s.search))
}
fn nonempty(s: &str) -> Option<String> {
    if s.is_empty() { None } else { Some(s.into()) }
}
fn bound(day: &str, time: &str, all_day: bool) -> Result<DateTime<Utc>, String> {
    let date = NaiveDate::parse_from_str(day, "%Y-%m-%d")
        .map_err(|_| "Use dates in YYYY-MM-DD format.".to_string())?;
    if all_day {
        return Ok(date.and_hms_opt(0, 0, 0).unwrap().and_utc());
    }
    let dt = NaiveDateTime::parse_from_str(&format!("{day} {time}"), "%Y-%m-%d %H:%M")
        .map_err(|_| "Use time in 24-hour HH:MM format.".to_string())?;
    Local.from_local_datetime(&dt).single().map(|t| t.with_timezone(&Utc)).ok_or_else(||"This local time is ambiguous or does not exist due to daylight saving. Choose another time.".into())
}
fn event_day(e: &CalendarEvent) -> NaiveDate {
    if e.all_day {
        e.dtstart.date_naive()
    } else {
        e.dtstart.with_timezone(&Local).date_naive()
    }
}
fn on_day(e: &CalendarEvent, day: NaiveDate) -> bool {
    if e.all_day {
        return day >= e.dtstart.date_naive()
            && day
                < e.dtend
                    .map(|v| v.date_naive())
                    .unwrap_or_else(|| e.dtstart.date_naive().succ_opt().unwrap());
    }
    let start = day.and_hms_opt(0, 0, 0).unwrap();
    let end = day.succ_opt().unwrap().and_hms_opt(0, 0, 0).unwrap();
    let a = e.dtstart.with_timezone(&Local).naive_local();
    let b = e
        .dtend
        .unwrap_or(e.dtstart)
        .with_timezone(&Local)
        .naive_local();
    a < end && (b > start || (a == b && a >= start))
}
fn week(day: NaiveDate) -> NaiveDate {
    day - Duration::days(day.weekday().num_days_from_sunday() as i64)
}
fn grid(day: NaiveDate) -> NaiveDate {
    week(day.with_day(1).unwrap())
}
fn shift(day: NaiveDate, mode: Mode, n: i32) -> NaiveDate {
    match mode {
        Mode::Day => day + Duration::days(n as i64),
        Mode::Week => day + Duration::days(n as i64 * 7),
        Mode::Month => {
            let months = day.year() * 12 + day.month0() as i32 + n;
            let first =
                NaiveDate::from_ymd_opt(months.div_euclid(12), months.rem_euclid(12) as u32 + 1, 1)
                    .unwrap();
            let next = if first.month() == 12 {
                NaiveDate::from_ymd_opt(first.year() + 1, 1, 1).unwrap()
            } else {
                first.with_month(first.month() + 1).unwrap()
            };
            first
                .with_day(day.day().min((next - Duration::days(1)).day()))
                .unwrap()
        }
    }
}
struct State {
    root: Entity,
    mode: Mode,
    day: NaiveDate,
    events: Vec<CalendarEvent>,
    path: String,
    draft: Option<Draft>,
    fields: Vec<Entity>,
    actions: HashMap<u64, Action>,
    queue: Vec<Command>,
    read: Option<u64>,
    mutation: Option<u64>,
    started: bool,
    redraw: bool,
    message: String,
    uncertain: bool,
    confirm: bool,
    narrow: bool,
    search: String,
    search_input: String,
    search_field: Option<Entity>,
    show_search: bool,
    show_agenda: bool,
    show_source: bool,
}
impl State {
    fn enqueue(&mut self, kind: Kind) -> u64 {
        let serial = next();
        self.queue.push(Command { serial, kind });
        serial
    }
    fn refresh(&mut self) {
        let start = match self.mode {
            Mode::Month => grid(self.day),
            Mode::Week => week(self.day),
            Mode::Day => self.day,
        };
        let count = match self.mode {
            Mode::Month => 42,
            Mode::Week => 7,
            Mode::Day => 1,
        };
        // Include UTC all-day bounds as well as local timed bounds, including DST.
        let from = (start - Duration::days(1))
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        let to = (start + Duration::days(count + 1))
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        self.read = Some(self.enqueue(Kind::List(CalendarListQuery {
            from: Some(from),
            to: Some(to),
            path: Some(self.path.clone()),
        })));
        self.redraw = true;
    }
}
pub fn mount(w: &mut World, parent: Entity) {
    let root = w.spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::column()
    },));
    set_parent(w, root, parent);
    crate::native_style::flat(w, root, crate::native_style::canvas());
    w.insert_resource(State {
        root,
        mode: Mode::Month,
        day: Local::now().date_naive(),
        events: vec![],
        path: "calendar/personal.ics".into(),
        draft: None,
        fields: vec![],
        actions: HashMap::new(),
        queue: vec![],
        read: None,
        mutation: None,
        started: false,
        redraw: true,
        message: String::new(),
        uncertain: false,
        confirm: false,
        narrow: false,
        search: String::new(),
        search_input: String::new(),
        search_field: None,
        show_search: false,
        show_agenda: false,
        show_source: false,
    });
}
pub fn activate(w: &mut World) {
    if let Some(s) = w.resource_mut::<State>() {
        if !s.started {
            s.started = true;
            s.refresh();
        }
    }
}
fn capture(w: &World, s: &mut State) {
    if let Some(l) = s.search_field.and_then(|e| w.get::<Label>(e)) {
        s.search_input = l.text.clone();
    }
    if let Some(d) = &mut s.draft {
        for (i, e) in s.fields.iter().enumerate() {
            if let Some(l) = w.get::<Label>(*e) {
                d.values[i] = l.text.clone();
            }
        }
    }
}
pub fn can_deactivate(w: &World) -> bool {
    w.resource::<State>().is_none_or(|s| {
        s.fields
            .iter()
            .chain(s.search_field.iter())
            .all(|e| !w.has::<ImePreview>(*e))
    })
}
pub fn sync(w: &mut World) {
    let can_render = can_deactivate(w);
    let Some(mut s) = w
        .resource_mut::<State>()
        .map(|s| std::mem::replace(s, empty_state(s.root)))
    else {
        return;
    };
    capture(w, &mut s);
    let narrow = w.resource::<Viewport>().is_some_and(|v| v.width < 1000.);
    if narrow != s.narrow {
        s.narrow = narrow;
        s.redraw = true;
    }
    if s.redraw && can_render {
        view::render(w, &mut s);
        s.redraw = false;
    }
    w.insert_resource(s);
}
// Only used while the real state is temporarily borrowed out for UI building.
fn empty_state(root: Entity) -> State {
    State {
        root,
        mode: Mode::Month,
        day: Local::now().date_naive(),
        events: vec![],
        path: String::new(),
        draft: None,
        fields: vec![],
        actions: HashMap::new(),
        queue: vec![],
        read: None,
        mutation: None,
        started: false,
        redraw: false,
        message: String::new(),
        uncertain: false,
        confirm: false,
        narrow: false,
        search: String::new(),
        search_input: String::new(),
        search_field: None,
        show_search: false,
        show_agenda: false,
        show_source: false,
    }
}
pub fn action(w: &mut World, host: &HostIntent) -> bool {
    let HostIntent::Command(id) = host else {
        return false;
    };
    let Some(a) = w
        .resource::<State>()
        .and_then(|s| s.actions.get(id))
        .cloned()
    else {
        return false;
    };
    if !can_deactivate(w) {
        return true;
    }
    let root = w.resource::<State>().unwrap().root;
    let mut s = std::mem::replace(w.resource_mut::<State>().unwrap(), empty_state(root));
    capture(w, &mut s);
    if s.mutation.is_none() {
        match a {
            Action::Shift(n) if s.draft.is_none() => {
                s.day = shift(s.day, s.mode, n);
                s.refresh();
            }
            Action::Today if s.draft.is_none() => {
                s.day = Local::now().date_naive();
                s.refresh();
            }
            Action::Mode(m) if s.draft.is_none() => {
                s.mode = m;
                s.refresh();
            }
            Action::Day(d) if s.draft.is_none() => {
                s.day = d;
                s.show_agenda = true;
                s.refresh();
            }
            Action::Refresh => {
                s.refresh();
            }
            Action::ToggleSearch if s.draft.is_none() => {
                s.show_search = !s.show_search;
            }
            Action::ToggleAgenda if s.draft.is_none() => {
                s.show_agenda = !s.show_agenda;
            }
            Action::ToggleSource if s.draft.is_none() => {
                s.show_source = !s.show_source;
            }
            Action::Search if s.draft.is_none() => {
                s.search = s.search_input.trim().to_lowercase();
            }
            Action::ClearSearch if s.draft.is_none() => {
                s.search.clear();
                s.search_input.clear();
                s.search_field = None;
            }
            Action::AddAlarm(minutes) if !s.uncertain => {
                if let Some(d) = &mut s.draft {
                    if !recurring(d)
                        && minutes > 0
                        && !d.alarms.iter().any(|a| a.trigger_minutes_before == minutes)
                    {
                        d.alarms.push(CalendarAlarm {
                            trigger_minutes_before: minutes,
                            action: "display".into(),
                        });
                        d.alarms.sort_by_key(|a| a.trigger_minutes_before);
                    }
                }
            }
            Action::RemoveAlarm(index) if !s.uncertain => {
                if let Some(d) = &mut s.draft {
                    if !recurring(d) && index < d.alarms.len() {
                        d.alarms.remove(index);
                    }
                }
            }
            Action::New if s.draft.is_none() => {
                s.draft = Some(Draft::new(s.day));
                s.message.clear();
                s.uncertain = false;
            }
            Action::Open(i) if s.draft.is_none() => {
                if let Some(e) = s.events.get(i) {
                    s.draft = Some(Draft::event(e.clone()));
                    s.message.clear();
                    s.uncertain = false;
                }
            }
            Action::Cancel => {
                s.draft = None;
                s.confirm = false;
                s.uncertain = false;
            }
            Action::AllDay => {
                s.fields.clear();
                if let Some(d) = &mut s.draft {
                    d.all_day = !d.all_day;
                    if d.all_day && d.values[1] == d.values[3] {
                        if let Ok(day) = NaiveDate::parse_from_str(&d.values[1], "%Y-%m-%d") {
                            d.values[3] = day.succ_opt().unwrap().to_string();
                        }
                    }
                }
            }
            Action::Save if !s.uncertain => {
                if let Some(d) = &s.draft {
                    match d.request(&s.path) {
                        Ok(r) => {
                            s.read = None;
                            s.mutation = Some(s.enqueue(Kind::Save(Box::new(r))));
                            s.message = "Saving…".into();
                        }
                        Err(e) => s.message = e,
                    }
                }
            }
            Action::Delete if !s.uncertain => {
                s.confirm = true;
            }
            Action::ConfirmDelete if s.confirm && !s.uncertain => {
                if let Some(e) = s
                    .draft
                    .as_ref()
                    .and_then(|d| d.original.as_ref())
                    .filter(|e| e.rrule.is_none() && e.recurrence_id.is_none())
                {
                    let (uid, path) = (e.uid.clone(), e.calendar_path.clone());
                    s.read = None;
                    s.mutation = Some(s.enqueue(Kind::Delete(uid, path)));
                    s.message = "Deleting…".into();
                }
            }
            _ => {}
        }
        s.redraw = true;
    }
    w.insert_resource(s);
    true
}
pub fn take_commands(w: &mut World) -> Vec<Command> {
    w.resource_mut::<State>()
        .map(|s| std::mem::take(&mut s.queue))
        .unwrap_or_default()
}
pub async fn execute(client: &MedousaClient, command: Command) -> Update {
    let result = match &command.kind {
        Kind::List(q) => client.calendar().list_events(q).await.map(Response::Listed),
        Kind::Save(r) => {
            if let Some(uid) = &r.uid {
                client.calendar().update_event(uid, r).await
            } else {
                client.calendar().create_event(r).await
            }
        }
        .map(|_| Response::Saved),
        Kind::Delete(uid, path) => client
            .calendar()
            .delete_event(
                uid,
                &medousa_types::CalendarExportQuery {
                    path: Some(path.clone()),
                },
            )
            .await
            .map(|r| Response::Deleted(r.deleted)),
    }
    .map_err(|e| e.to_string());
    Update { command, result }
}
pub fn apply(w: &mut World, u: Update) {
    let Some(mut s) = w
        .resource_mut::<State>()
        .map(|s| std::mem::replace(s, empty_state(s.root)))
    else {
        return;
    };
    capture(w, &mut s);
    let reading = matches!(u.command.kind, Kind::List(_));
    if (if reading { s.read } else { s.mutation }) != Some(u.command.serial) {
        w.insert_resource(s);
        return;
    }
    if reading {
        s.read = None;
    } else {
        s.mutation = None;
    }
    match u.result {
        Ok(Response::Listed(r)) => {
            s.events = r.events;
            s.events.sort_by_key(|e| e.dtstart);
            s.path = r.calendar_path;
        }
        Ok(Response::Saved) | Ok(Response::Deleted(true)) => {
            s.draft = None;
            s.confirm = false;
            s.uncertain = false;
            s.message = if matches!(u.command.kind, Kind::Save(_)) {
                "Event saved."
            } else {
                "Event deleted."
            }
            .into();
            s.refresh();
        }
        Ok(Response::Deleted(false)) => {
            s.message = "Event was not deleted. Refresh to inspect the current calendar.".into();
        }
        Err(e) => {
            s.message = if reading {
                format!("Calendar could not be loaded: {e}")
            } else {
                s.uncertain = true;
                format!(
                    "Could not confirm the change: {e}. It may have reached the daemon. Draft retained; close and refresh to inspect before trying again."
                )
            };
        }
    }
    s.redraw = true;
    w.insert_resource(s);
}
#[cfg(test)]
#[path = "calendar_tests.rs"]
mod tests;
