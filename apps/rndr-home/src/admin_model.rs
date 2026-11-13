//! App-local navigation and sanitized read snapshots. No credentials or write operations.
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Resource {
    Health,
    Models,
    Runtimes,
    Modes,
    Mcp,
    LocalEngine,
    Schedules,
    Sharing,
    Capacity,
    ReviewProviders,
    Activity,
}
impl Resource {
    pub const ALL: [Self; 11] = [
        Self::Health,
        Self::Models,
        Self::Runtimes,
        Self::Modes,
        Self::Mcp,
        Self::LocalEngine,
        Self::Schedules,
        Self::Sharing,
        Self::Capacity,
        Self::ReviewProviders,
        Self::Activity,
    ];
    pub fn title(self) -> &'static str {
        match self {
            Self::Health => "Workshop status",
            Self::Models => "Models & providers",
            Self::Runtimes => "External agent runtimes",
            Self::Modes => "Agent modes",
            Self::Mcp => "MCP gateway & servers",
            Self::LocalEngine => "Local inference engine",
            Self::Schedules => "Scheduled automations",
            Self::Sharing => "Shared workshop mode",
            Self::Capacity => "Worker capacity",
            Self::ReviewProviders => "Hosted review providers",
            Self::Activity => "Activity · jobs snapshot",
        }
    }
}

/// Names/order copied from production types/settings.ts, not a new taxonomy.
pub const SETTINGS: [(&str, &str, &str); 9] = [
    ("preferences", "Preferences", "Look, notifications & chrome"),
    (
        "accounts",
        "Accounts",
        "Connected services & review accounts",
    ),
    ("agent", "Medousa Agent", "Stance, memory & models"),
    (
        "runtime",
        "Runtime Controls",
        "Reach, shell, engine & versions",
    ),
    ("network", "Sharing", "Seats, phone, peers & channels"),
    (
        "connections",
        "External Agents",
        "External coding agents & connected conversations",
    ),
    ("packages", "Packages", "Channel adapters & installs"),
    ("mcp", "MCP", "Gateway & tool servers"),
    ("basement", "Connection", "Active workshop, engine & files"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    Chat,
    Settings(usize),
    Data(Resource),
    Deferred(&'static str),
}
impl Route {
    pub fn resource(self) -> Option<Resource> {
        match self {
            Self::Data(r) => Some(r),
            Self::Settings(1 | 5) => Some(Resource::Runtimes),
            Self::Settings(2) => Some(Resource::Models),
            Self::Settings(3 | 8) => Some(Resource::Health),
            Self::Settings(7) => Some(Resource::Mcp),
            Self::Settings(4) => Some(Resource::Sharing),
            _ => None,
        }
    }
    pub fn title(self) -> &'static str {
        match self {
            Self::Chat => "Chat",
            Self::Settings(i) => SETTINGS[i].1,
            Self::Data(r) => r.title(),
            Self::Deferred(name) => name,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub title: String,
    pub fields: Vec<(String, String)>,
}
impl Record {
    pub fn new(title: impl AsRef<str>, fields: Vec<(&str, String)>) -> Self {
        Self {
            title: display_text(title.as_ref()),
            fields: fields
                .into_iter()
                .map(|(k, v)| (k.into(), display_text(&v)))
                .collect(),
        }
    }
    pub fn matches(&self, query: &str) -> bool {
        let query = query.to_lowercase();
        self.title.to_lowercase().contains(&query)
            || self
                .fields
                .iter()
                .any(|(_, v)| v.to_lowercase().contains(&query))
    }
}
/// Rendering plain text, never interpreting markup/URLs; bound hostile labels.
/// Credentials, commands, auth_detail, prompts and raw service errors are not projected.
pub fn display_text(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).take(240).collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadState {
    NotLoaded,
    Loading(u64),
    Ready(Vec<Record>),
    Error(ReadError),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadError {
    Permission,
    Unavailable,
    Incompatible,
    Disconnected,
}
impl ReadError {
    pub fn message(self) -> &'static str {
        match self {
            Self::Permission => {
                "Permission denied. Use Connection in the existing Medousa app; no approval or credential entry here."
            }
            Self::Unavailable => {
                "Read unavailable. Check the daemon or service in the existing Medousa app, then retry."
            }
            Self::Incompatible => {
                "Unsupported or incompatible response. This preview will not guess service state."
            }
            Self::Disconnected => {
                "Connection worker stopped. Restart this preview after checking the existing app."
            }
        }
    }
}
pub struct Desktop {
    pub route: Route,
    pub last_settings: usize,
    pub reads: BTreeMap<Resource, ReadState>,
    pub serial: u64,
    pub selected: Option<usize>,
    pub query: String,
    pub dirty: bool,
    pub narrow: bool,
    /// Explicit navigation/filter/detail changes start at the top; rebuilds do not.
    pub reset_scroll: bool,
}
impl Default for Desktop {
    fn default() -> Self {
        Self {
            route: Route::Chat,
            last_settings: 0,
            reads: BTreeMap::new(),
            serial: 0,
            selected: None,
            query: String::new(),
            dirty: true,
            narrow: false,
            reset_scroll: true,
        }
    }
}
impl Desktop {
    pub fn navigate(&mut self, route: Route) {
        self.route = route;
        if let Route::Settings(index) = route {
            self.last_settings = index;
        }
        self.reset_scroll = true;
        self.query.clear();
        self.selected = None;
        self.dirty = true;
    }
    pub fn begin(&mut self, resource: Resource) -> Option<u64> {
        if matches!(self.reads.get(&resource), Some(ReadState::Loading(_))) {
            return None;
        }
        self.serial += 1;
        self.reads.insert(resource, ReadState::Loading(self.serial));
        if self.route.resource() == Some(resource) {
            // A refresh returns to results, not an index that may identify a new row.
            self.reset_scroll |= self.selected.take().is_some();
            self.dirty = true;
        }
        Some(self.serial)
    }
    pub fn settle(
        &mut self,
        resource: Resource,
        serial: u64,
        result: Result<Vec<Record>, ReadError>,
    ) {
        if self.reads.get(&resource) != Some(&ReadState::Loading(serial)) {
            return;
        }
        self.reads.insert(
            resource,
            match result {
                Ok(rows) => ReadState::Ready(rows),
                Err(e) => ReadState::Error(e),
            },
        );
        if self.route.resource() == Some(resource) {
            self.dirty = true;
        }
    }
}
