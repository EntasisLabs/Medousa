//! Daemon-owned isolated Chromium worlds.
//!
//! Unlike Home's attached browser surfaces, these worlds live with the
//! workshop daemon. A portal may disconnect without stopping the browser;
//! profile identity, process ownership, control state, and the concrete driver
//! id remain explicit and durable until the owner cleans the world up.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use futures_util::{SinkExt, StreamExt};
use medousa_browser_bridge::{
    BROWSER_SCREENSHOT_SCHEMA_VERSION, BrowserControl, BrowserObservation,
    BrowserObservationCapture, BrowserObservationViewport, BrowserScreenshotCapture,
    BrowserSemanticNode, TabGroupManager, TabOpenedBy,
};
use medousa_world::{
    WorldActionPermit, WorldDriverCapability, WorldDriverId, WorldDriverKind,
    WorldDriverRegistration, WorldDriverTransport, WorldOwnership, WorldSurfaceKind,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use image::GenericImageView as _;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use uuid::Uuid;

const CATALOG_SCHEMA_VERSION: u16 = 1;
const DRIVER_PREFIX: &str = "driver:isolated-browser:";
const STARTUP_ATTEMPTS: usize = 200;
const STARTUP_POLL_MS: u64 = 50;
const CDP_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_PROFILE_ID_BYTES: usize = 80;
const MAX_DISPLAY_NAME_BYTES: usize = 120;
const MAX_URL_BYTES: usize = 8 * 1024;
const MAX_ACTION_STEPS: usize = 16;
const MAX_SCREENSHOT_BYTES: usize = 12 * 1024 * 1024;
const MAX_WORLDS_PER_PROFILE: usize = 32;
const MAX_CATALOG_WORLDS: usize = 256;
const MAX_RUNNING_WORLDS: usize = 8;

static GLOBAL_HOST: OnceLock<Arc<IsolatedBrowserHost>> = OnceLock::new();

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IsolatedBrowserProfile {
    Ephemeral,
    Persistent { profile_id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IsolatedBrowserRunState {
    Starting,
    Running,
    Paused,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsolatedBrowserWorld {
    pub world_id: String,
    pub owner_profile_id: String,
    pub authority_id: String,
    pub driver: WorldDriverRegistration,
    pub tab_group_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<String>,
    pub profile: IsolatedBrowserProfile,
    pub run_state: IsolatedBrowserRunState,
    pub control: BrowserControl,
    /// Host-local fence incremented whenever control or run state changes.
    /// Guarded batches re-check it between steps so a human pause/takeover
    /// cannot be hidden by a fast return-to-agent race.
    #[serde(default)]
    pub control_epoch: u64,
    pub view_attached: bool,
    pub headless: bool,
    pub url: String,
    #[serde(default)]
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<String>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

impl IsolatedBrowserWorld {
    pub fn agent_operable(&self) -> bool {
        self.run_state == IsolatedBrowserRunState::Running
            && self.control == BrowserControl::Agent
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateIsolatedBrowserWorldRequest {
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub profile: Option<IsolatedBrowserProfile>,
    #[serde(default)]
    pub initial_url: Option<String>,
    #[serde(default = "default_headless")]
    pub headless: bool,
}

fn default_headless() -> bool {
    true
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IsolatedBrowserLifecycleAction {
    Pause,
    Resume,
    Takeover,
    ReturnToAgent,
    AttachView,
    DetachView,
    Stop,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IsolatedBrowserLifecycleRequest {
    pub action: IsolatedBrowserLifecycleAction,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IsolatedBrowserNavigateRequest {
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IsolatedBrowserObserveRequest {
    #[serde(default)]
    pub since_revision: Option<u64>,
    #[serde(default = "default_observation_nodes")]
    pub max_nodes: usize,
}

fn default_observation_nodes() -> usize {
    256
}

#[derive(Debug, Clone, Deserialize)]
pub struct IsolatedBrowserScreenshotRequest {
    pub expected_document_id: String,
    pub expected_observation_revision: u64,
    #[serde(default = "default_screenshot_width")]
    pub max_width: u32,
}

fn default_screenshot_width() -> u32 {
    1280
}

#[derive(Debug, Clone, Deserialize)]
pub struct IsolatedBrowserCleanupQuery {
    #[serde(default)]
    pub delete_profile: bool,
}

#[derive(Debug, Clone)]
pub struct IsolatedBrowserContext {
    pub world_id: String,
    pub driver_id: String,
    pub tab_group_id: String,
    pub tab_id: String,
    pub url: String,
    pub control: BrowserControl,
}

#[derive(Debug)]
pub enum IsolatedBrowserError {
    Invalid(String),
    NotFound(String),
    Conflict(String),
    Unavailable(String),
    Driver(String),
    Store(String),
}

impl std::fmt::Display for IsolatedBrowserError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(message)
            | Self::NotFound(message)
            | Self::Conflict(message)
            | Self::Unavailable(message)
            | Self::Driver(message)
            | Self::Store(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for IsolatedBrowserError {}

#[derive(Debug, Default, Serialize, Deserialize)]
struct IsolatedBrowserCatalog {
    schema_version: u16,
    #[serde(default)]
    worlds: Vec<IsolatedBrowserWorld>,
}

struct BrowserRuntime {
    child: Child,
    websocket_url: String,
    target_id: String,
}

#[derive(Default)]
struct HostState {
    worlds: BTreeMap<String, IsolatedBrowserWorld>,
    runtimes: HashMap<String, BrowserRuntime>,
}

pub struct IsolatedBrowserHost {
    root: PathBuf,
    state: Mutex<HostState>,
    mutation: Mutex<()>,
}

impl IsolatedBrowserHost {
    pub async fn open(root: PathBuf) -> Result<Arc<Self>, IsolatedBrowserError> {
        tokio::fs::create_dir_all(root.join("worlds"))
            .await
            .map_err(|error| store_error("create isolated browser world root", error))?;
        tokio::fs::create_dir_all(root.join("profiles"))
            .await
            .map_err(|error| store_error("create isolated browser profile root", error))?;

        let catalog_path = root.join("worlds.json");
        let mut catalog = match tokio::fs::read(&catalog_path).await {
            Ok(bytes) => serde_json::from_slice::<IsolatedBrowserCatalog>(&bytes).map_err(|error| {
                IsolatedBrowserError::Store(format!(
                    "decode isolated browser catalog {}: {error}",
                    catalog_path.display()
                ))
            })?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                IsolatedBrowserCatalog {
                    schema_version: CATALOG_SCHEMA_VERSION,
                    worlds: Vec::new(),
                }
            }
            Err(error) => return Err(store_error("read isolated browser catalog", error)),
        };
        if catalog.schema_version != CATALOG_SCHEMA_VERSION {
            return Err(IsolatedBrowserError::Store(format!(
                "isolated browser catalog schema {} is not supported",
                catalog.schema_version
            )));
        }

        let now = now_ms();
        let mut recovered = false;
        let mut worlds = BTreeMap::new();
        let mut persistent_profiles = BTreeSet::new();
        for mut world in catalog.worlds.drain(..) {
            validate_loaded_world(&world)?;
            if world.control_epoch == 0 {
                world.control_epoch = 1;
                recovered = true;
            }
            if let IsolatedBrowserProfile::Persistent { profile_id } = &world.profile
                && !persistent_profiles.insert(profile_id.clone())
            {
                return Err(IsolatedBrowserError::Store(format!(
                    "isolated browser catalog attaches persistent profile '{profile_id}' more than once"
                )));
            }
            if matches!(
                world.run_state,
                IsolatedBrowserRunState::Starting
                    | IsolatedBrowserRunState::Running
                    | IsolatedBrowserRunState::Paused
            ) {
                world.run_state = IsolatedBrowserRunState::Stopped;
                world.failure = Some(
                    "workshop restarted; resume this world to reopen its isolated profile"
                        .to_string(),
                );
                world.tab_id = None;
                world.updated_at_ms = now;
                recovered = true;
            }
            ensure_authoritative_world(&world)?;
            if worlds.insert(world.world_id.clone(), world).is_some() {
                return Err(IsolatedBrowserError::Store(
                    "isolated browser catalog contains duplicate world ids".to_string(),
                ));
            }
        }

        let host = Arc::new(Self {
            root,
            state: Mutex::new(HostState {
                worlds,
                runtimes: HashMap::new(),
            }),
            mutation: Mutex::new(()),
        });
        if recovered {
            host.persist().await?;
        }
        Ok(host)
    }

    pub async fn open_default() -> Result<Arc<Self>, IsolatedBrowserError> {
        Self::open(crate::paths::medousa_data_dir().join("browser").join("isolated")).await
    }

    pub async fn list(&self, owner_profile_id: &str) -> Vec<IsolatedBrowserWorld> {
        self.refresh_exited_processes().await;
        self.state
            .lock()
            .await
            .worlds
            .values()
            .filter(|world| world.owner_profile_id == owner_profile_id)
            .cloned()
            .collect()
    }

    pub async fn get(
        &self,
        owner_profile_id: &str,
        world_id: &str,
    ) -> Result<IsolatedBrowserWorld, IsolatedBrowserError> {
        self.refresh_exited_processes().await;
        let state = self.state.lock().await;
        owned_world(&state, owner_profile_id, world_id).cloned()
    }

    pub async fn create(
        &self,
        owner_profile_id: &str,
        authority_id: &str,
        request: CreateIsolatedBrowserWorldRequest,
    ) -> Result<IsolatedBrowserWorld, IsolatedBrowserError> {
        let _mutation = self.mutation.lock().await;
        validate_owner(owner_profile_id)?;
        let profile = request.profile.unwrap_or(IsolatedBrowserProfile::Ephemeral);
        validate_profile(&profile)?;
        let initial_url = normalize_browser_url(request.initial_url.as_deref())?;
        let display_name = normalize_display_name(request.display_name.as_deref())?;
        {
            let state = self.state.lock().await;
            ensure_world_capacity(&state, owner_profile_id)?;
            ensure_profile_available(&state, &profile, None)?;
        }

        let suffix = Uuid::new_v4().simple().to_string();
        let driver_id = format!("{DRIVER_PREFIX}{suffix}");
        let tab_group = TabGroupManager::create_group(&driver_id, None, None);
        TabGroupManager::set_control(&tab_group.id, BrowserControl::Agent);
        let registered_world_id = crate::world_authority::register_owned_browser_world(
            authority_id,
            &driver_id,
            &tab_group.id,
            &format!("human:{owner_profile_id}"),
        )
        .map_err(IsolatedBrowserError::Conflict)?;
        let now = now_ms();
        let world = IsolatedBrowserWorld {
            world_id: registered_world_id.to_string(),
            owner_profile_id: owner_profile_id.to_string(),
            authority_id: authority_id.to_string(),
            driver: isolated_driver_registration(&driver_id, display_name),
            tab_group_id: tab_group.id,
            tab_id: None,
            profile,
            run_state: IsolatedBrowserRunState::Starting,
            control: BrowserControl::Agent,
            control_epoch: 1,
            view_attached: true,
            headless: request.headless,
            url: initial_url,
            title: String::new(),
            failure: None,
            created_at_ms: now,
            updated_at_ms: now,
        };

        {
            let mut state = self.state.lock().await;
            state.worlds.insert(world.world_id.clone(), world.clone());
        }
        self.persist().await?;

        match self.launch_world(&world.world_id).await {
            Ok(()) => self.get(owner_profile_id, &world.world_id).await,
            Err(error) => {
                self.mark_failed(&world.world_id, error.to_string()).await;
                self.persist().await?;
                Err(error)
            }
        }
    }

    pub async fn lifecycle(
        &self,
        owner_profile_id: &str,
        world_id: &str,
        action: IsolatedBrowserLifecycleAction,
    ) -> Result<IsolatedBrowserWorld, IsolatedBrowserError> {
        let _mutation = self.mutation.lock().await;
        self.refresh_exited_processes().await;

        match action {
            IsolatedBrowserLifecycleAction::Resume => {
                let snapshot = self.get(owner_profile_id, world_id).await?;
                ensure_authoritative_world(&snapshot)?;
                self.return_authoritative_control(&snapshot)?;
                let needs_launch = {
                    let mut state = self.state.lock().await;
                    let runtime_exists = state.runtimes.contains_key(world_id);
                    let world = owned_world_mut(&mut state, owner_profile_id, world_id)?;
                    match world.run_state {
                        IsolatedBrowserRunState::Running => {
                            world.control = BrowserControl::Agent;
                            world.control_epoch = world.control_epoch.saturating_add(1);
                            world.failure = None;
                            world.updated_at_ms = now_ms();
                            false
                        }
                        IsolatedBrowserRunState::Paused if runtime_exists => {
                            world.run_state = IsolatedBrowserRunState::Running;
                            world.control = BrowserControl::Agent;
                            world.control_epoch = world.control_epoch.saturating_add(1);
                            world.failure = None;
                            world.updated_at_ms = now_ms();
                            TabGroupManager::set_control(
                                &world.tab_group_id,
                                BrowserControl::Agent,
                            );
                            false
                        }
                        _ => {
                            world.run_state = IsolatedBrowserRunState::Starting;
                            world.control = BrowserControl::Agent;
                            world.control_epoch = world.control_epoch.saturating_add(1);
                            world.failure = None;
                            world.updated_at_ms = now_ms();
                            true
                        }
                    }
                };
                self.persist().await?;
                if needs_launch && let Err(error) = self.launch_world(world_id).await {
                    self.mark_failed(world_id, error.to_string()).await;
                    self.persist().await?;
                    return Err(error);
                }
            }
            IsolatedBrowserLifecycleAction::Pause
            | IsolatedBrowserLifecycleAction::Takeover
            | IsolatedBrowserLifecycleAction::ReturnToAgent
            | IsolatedBrowserLifecycleAction::AttachView
            | IsolatedBrowserLifecycleAction::DetachView => {
                let mut state = self.state.lock().await;
                let runtime_exists = state.runtimes.contains_key(world_id);
                let world = owned_world_mut(&mut state, owner_profile_id, world_id)?;
                match action {
                    IsolatedBrowserLifecycleAction::Pause => {
                        if !matches!(
                            world.run_state,
                            IsolatedBrowserRunState::Running | IsolatedBrowserRunState::Paused
                        ) {
                            return Err(IsolatedBrowserError::Conflict(
                                "only a running isolated browser can be paused".to_string(),
                            ));
                        }
                        self.take_authoritative_control(world)?;
                        world.run_state = IsolatedBrowserRunState::Paused;
                        world.control_epoch = world.control_epoch.saturating_add(1);
                    }
                    IsolatedBrowserLifecycleAction::Takeover => {
                        if !runtime_exists {
                            return Err(IsolatedBrowserError::Conflict(
                                "isolated browser is not running".to_string(),
                            ));
                        }
                        self.take_authoritative_control(world)?;
                        world.control = BrowserControl::User;
                        world.control_epoch = world.control_epoch.saturating_add(1);
                    }
                    IsolatedBrowserLifecycleAction::ReturnToAgent => {
                        if world.run_state != IsolatedBrowserRunState::Running {
                            return Err(IsolatedBrowserError::Conflict(
                                "resume the isolated browser before returning agent control"
                                    .to_string(),
                            ));
                        }
                        self.return_authoritative_control(world)?;
                        world.control = BrowserControl::Agent;
                        world.control_epoch = world.control_epoch.saturating_add(1);
                    }
                    IsolatedBrowserLifecycleAction::AttachView => world.view_attached = true,
                    IsolatedBrowserLifecycleAction::DetachView => world.view_attached = false,
                    IsolatedBrowserLifecycleAction::Resume
                    | IsolatedBrowserLifecycleAction::Stop => unreachable!(),
                }
                TabGroupManager::set_control(&world.tab_group_id, world.control);
                world.updated_at_ms = now_ms();
                drop(state);
                self.persist().await?;
            }
            IsolatedBrowserLifecycleAction::Stop => {
                let snapshot = self.get(owner_profile_id, world_id).await?;
                self.take_authoritative_control(&snapshot)?;
                self.stop_runtime(world_id).await?;
                let mut state = self.state.lock().await;
                let world = owned_world_mut(&mut state, owner_profile_id, world_id)?;
                world.run_state = IsolatedBrowserRunState::Stopped;
                world.control_epoch = world.control_epoch.saturating_add(1);
                world.tab_id = None;
                world.updated_at_ms = now_ms();
                drop(state);
                self.persist().await?;
            }
        }
        self.get(owner_profile_id, world_id).await
    }

    pub async fn cleanup(
        &self,
        owner_profile_id: &str,
        world_id: &str,
        delete_profile: bool,
    ) -> Result<IsolatedBrowserWorld, IsolatedBrowserError> {
        let _mutation = self.mutation.lock().await;
        self.ensure_owned(owner_profile_id, world_id).await?;
        self.stop_runtime(world_id).await?;
        let removed = self
            .state
            .lock()
            .await
            .worlds
            .remove(world_id)
            .ok_or_else(|| IsolatedBrowserError::NotFound("browser world not found".to_string()))?;
        self.persist().await?;
        TabGroupManager::remove_group(&removed.tab_group_id);
        crate::world_authority::forget_owned_browser_world(
            &removed.world_id,
            &format!("human:{}", removed.owner_profile_id),
        )
        .map_err(IsolatedBrowserError::Conflict)?;

        let should_delete_profile = matches!(removed.profile, IsolatedBrowserProfile::Ephemeral)
            || delete_profile;
        if should_delete_profile {
            let directory = self.profile_directory(&removed);
            match tokio::fs::remove_dir_all(&directory).await {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(IsolatedBrowserError::Store(format!(
                        "world was removed but profile cleanup {} failed: {error}",
                        directory.display()
                    )));
                }
            }
        }
        Ok(removed)
    }

    pub async fn context_for_driver(
        &self,
        owner_profile_id: &str,
        driver_id: &str,
    ) -> Result<IsolatedBrowserContext, IsolatedBrowserError> {
        self.context_for_driver_inner(owner_profile_id, driver_id, true)
            .await
    }

    pub async fn observation_context_for_driver(
        &self,
        owner_profile_id: &str,
        driver_id: &str,
    ) -> Result<IsolatedBrowserContext, IsolatedBrowserError> {
        self.context_for_driver_inner(owner_profile_id, driver_id, false)
            .await
    }

    async fn context_for_driver_inner(
        &self,
        owner_profile_id: &str,
        driver_id: &str,
        require_agent_control: bool,
    ) -> Result<IsolatedBrowserContext, IsolatedBrowserError> {
        self.refresh_exited_processes().await;
        let state = self.state.lock().await;
        let world = state
            .worlds
            .values()
            .find(|world| {
                world.driver.driver_id.as_str() == driver_id
                    && world.owner_profile_id == owner_profile_id
            })
            .ok_or_else(|| {
                IsolatedBrowserError::NotFound(
                    "selected isolated browser driver is not owned by this profile".to_string(),
                )
            })?;
        if world.run_state != IsolatedBrowserRunState::Running
            || (require_agent_control && world.control != BrowserControl::Agent)
        {
            return Err(IsolatedBrowserError::Conflict(match world.control {
                BrowserControl::User | BrowserControl::AwaitingOperator => {
                    "isolated browser is under human control".to_string()
                }
                BrowserControl::Agent => {
                    "isolated browser is paused or stopped; resume it before acting".to_string()
                }
            }));
        }
        if !state.runtimes.contains_key(&world.world_id) {
            return Err(IsolatedBrowserError::Unavailable(
                "isolated browser process is unavailable".to_string(),
            ));
        }
        Ok(IsolatedBrowserContext {
            world_id: world.world_id.clone(),
            driver_id: world.driver.driver_id.to_string(),
            tab_group_id: world.tab_group_id.clone(),
            tab_id: world.tab_id.clone().ok_or_else(|| {
                IsolatedBrowserError::Unavailable(
                    "isolated browser has no active page".to_string(),
                )
            })?,
            url: world.url.clone(),
            control: world.control,
        })
    }

    pub async fn is_driver_for_profile(&self, owner_profile_id: &str, driver_id: &str) -> bool {
        self.state.lock().await.worlds.values().any(|world| {
            world.driver.driver_id.as_str() == driver_id
                && world.owner_profile_id == owner_profile_id
        })
    }

    async fn ensure_owned(
        &self,
        owner_profile_id: &str,
        world_id: &str,
    ) -> Result<(), IsolatedBrowserError> {
        let state = self.state.lock().await;
        owned_world(&state, owner_profile_id, world_id).map(|_| ())
    }

    /// Navigate a daemon-owned page as an explicit human action. Taking this
    /// path fences any agent batch before dispatching the navigation.
    pub async fn navigate(
        &self,
        owner_profile_id: &str,
        world_id: &str,
        url: &str,
    ) -> Result<IsolatedBrowserWorld, IsolatedBrowserError> {
        let url = normalize_browser_url(Some(url))?;
        let snapshot = self.get(owner_profile_id, world_id).await?;
        if snapshot.run_state != IsolatedBrowserRunState::Running {
            return Err(IsolatedBrowserError::Conflict(
                "resume the isolated browser before navigating".to_string(),
            ));
        }
        self.take_authoritative_control(&snapshot)?;
        let (websocket_url, target_id, tab_group_id) = {
            let mut state = self.state.lock().await;
            let runtime = state.runtimes.get(world_id).ok_or_else(|| {
                IsolatedBrowserError::Conflict(
                    "resume the isolated browser before navigating".to_string(),
                )
            })?;
            let websocket_url = runtime.websocket_url.clone();
            let target_id = runtime.target_id.clone();
            let world = owned_world_mut(&mut state, owner_profile_id, world_id)?;
            if world.run_state != IsolatedBrowserRunState::Running {
                return Err(IsolatedBrowserError::Conflict(
                    "resume the isolated browser before navigating".to_string(),
                ));
            }
            world.control = BrowserControl::User;
            world.control_epoch = world.control_epoch.saturating_add(1);
            world.updated_at_ms = now_ms();
            TabGroupManager::set_control(&world.tab_group_id, BrowserControl::User);
            (websocket_url, target_id, world.tab_group_id.clone())
        };
        navigate_target(&websocket_url, &target_id, &url).await?;
        let (actual_url, title) = page_identity(&websocket_url, &target_id).await?;
        {
            let mut state = self.state.lock().await;
            let world = owned_world_mut(&mut state, owner_profile_id, world_id)?;
            world.url = actual_url.clone();
            world.title = title.clone();
            world.updated_at_ms = now_ms();
            TabGroupManager::navigate_active_tab(
                &tab_group_id,
                &actual_url,
                Some(&title),
                TabOpenedBy::User,
            );
        }
        self.persist().await?;
        self.get(owner_profile_id, world_id).await
    }

    pub async fn observe(
        &self,
        owner_profile_id: &str,
        world_id: &str,
        since_revision: Option<u64>,
        max_nodes: usize,
    ) -> Result<BrowserObservation, IsolatedBrowserError> {
        let (world, websocket_url, target_id) = self
            .runtime_binding(owner_profile_id, world_id, false)
            .await?;
        let tab_id = world.tab_id.as_deref().ok_or_else(|| {
            IsolatedBrowserError::Unavailable("isolated browser has no active page".to_string())
        })?;
        let capture = capture_observation(&websocket_url, &target_id, tab_id, max_nodes).await?;
        let observation = TabGroupManager::record_observation(
            &world.tab_group_id,
            capture,
            since_revision,
            max_nodes,
        )
        .map_err(IsolatedBrowserError::Driver)?;
        if self
            .update_page_identity(world_id, &observation.url, &observation.title)
            .await
        {
            self.persist().await?;
        }
        Ok(observation)
    }

    pub async fn observe_for_driver(
        &self,
        owner_profile_id: &str,
        driver_id: &str,
        since_revision: Option<u64>,
        max_nodes: usize,
    ) -> Result<BrowserObservation, IsolatedBrowserError> {
        let world_id = self
            .world_id_for_driver(owner_profile_id, driver_id)
            .await?;
        self.observe(owner_profile_id, &world_id, since_revision, max_nodes)
            .await
    }

    pub async fn screenshot(
        &self,
        owner_profile_id: &str,
        world_id: &str,
        expected_document_id: &str,
        expected_observation_revision: u64,
        max_width: u32,
    ) -> Result<BrowserScreenshotCapture, IsolatedBrowserError> {
        let (world, websocket_url, target_id) = self
            .runtime_binding(owner_profile_id, world_id, false)
            .await?;
        let tab_id = world.tab_id.as_deref().ok_or_else(|| {
            IsolatedBrowserError::Unavailable("isolated browser has no active page".to_string())
        })?;
        let observation = TabGroupManager::current_observation(&world.tab_group_id, tab_id)
            .ok_or_else(|| {
                IsolatedBrowserError::Conflict(
                    "observe the isolated browser before capturing pixels".to_string(),
                )
            })?;
        if observation.document_id != expected_document_id
            || observation.revision != expected_observation_revision
        {
            return Err(IsolatedBrowserError::Conflict(
                "pixel capture belongs to stale browser state".to_string(),
            ));
        }
        capture_screenshot(
            &websocket_url,
            &target_id,
            tab_id,
            &observation,
            max_width.clamp(320, 1600),
        )
        .await
    }

    pub async fn screenshot_for_driver(
        &self,
        owner_profile_id: &str,
        driver_id: &str,
        expected_document_id: &str,
        expected_observation_revision: u64,
        max_width: u32,
    ) -> Result<BrowserScreenshotCapture, IsolatedBrowserError> {
        let world_id = self
            .world_id_for_driver(owner_profile_id, driver_id)
            .await?;
        self.screenshot(
            owner_profile_id,
            &world_id,
            expected_document_id,
            expected_observation_revision,
            max_width,
        )
        .await
    }

    pub async fn act_for_driver(
        &self,
        owner_profile_id: &str,
        driver_id: &str,
        body: Value,
    ) -> Result<Value, IsolatedBrowserError> {
        let world_id = self
            .world_id_for_driver(owner_profile_id, driver_id)
            .await?;
        let (world, websocket_url, target_id) = self
            .runtime_binding(owner_profile_id, &world_id, true)
            .await?;
        let tab_id = world.tab_id.clone().ok_or_else(|| {
            IsolatedBrowserError::Unavailable("isolated browser has no active page".to_string())
        })?;
        let permit: WorldActionPermit = serde_json::from_value(
            body.get("world_permit")
                .cloned()
                .ok_or_else(|| {
                    IsolatedBrowserError::Invalid(
                        "isolated browser action is missing its world permit".to_string(),
                    )
                })?,
        )
        .map_err(|error| {
            IsolatedBrowserError::Invalid(format!("decode isolated browser permit: {error}"))
        })?;
        validate_action_permit(&world, &permit, &body, &tab_id)?;
        let steps = parse_action_steps(&body)?;
        let expected_epoch = world.control_epoch;
        let (mut connection, session_id) = connect_page(&websocket_url, &target_id).await?;
        let context_id = isolated_execution_context(&mut connection, &session_id).await?;
        let mut executed_steps = 0usize;

        for step in &steps {
            self.validate_action_boundary(&world_id, expected_epoch, &permit)
                .await?;
            execute_action_step(
                &mut connection,
                &session_id,
                context_id,
                step,
            )
            .await?;
            executed_steps = executed_steps.saturating_add(1);
        }

        let observation = self
            .observe(owner_profile_id, &world_id, None, 256)
            .await?;
        Ok(json!({
            "ok": true,
            "url": observation.url,
            "executed_steps": executed_steps,
            "total_steps": steps.len(),
            "observation": observation,
            "binding_used": "isolated_browser",
        }))
    }

    async fn validate_action_boundary(
        &self,
        world_id: &str,
        expected_epoch: u64,
        permit: &WorldActionPermit,
    ) -> Result<(), IsolatedBrowserError> {
        crate::world_authority::validate_browser_action_permit(permit)
            .map_err(IsolatedBrowserError::Conflict)?;
        let state = self.state.lock().await;
        let world = state.worlds.get(world_id).ok_or_else(|| {
            IsolatedBrowserError::NotFound("isolated browser world was removed".to_string())
        })?;
        if !world.agent_operable()
            || world.control_epoch != expected_epoch
            || !state.runtimes.contains_key(world_id)
        {
            return Err(IsolatedBrowserError::Conflict(
                "isolated browser control changed; remaining actions were cancelled".to_string(),
            ));
        }
        Ok(())
    }

    async fn runtime_binding(
        &self,
        owner_profile_id: &str,
        world_id: &str,
        require_agent_control: bool,
    ) -> Result<(IsolatedBrowserWorld, String, String), IsolatedBrowserError> {
        self.refresh_exited_processes().await;
        let state = self.state.lock().await;
        let world = owned_world(&state, owner_profile_id, world_id)?;
        if world.run_state != IsolatedBrowserRunState::Running {
            return Err(IsolatedBrowserError::Conflict(
                "isolated browser is paused or stopped".to_string(),
            ));
        }
        if require_agent_control && world.control != BrowserControl::Agent {
            return Err(IsolatedBrowserError::Conflict(
                "isolated browser is under human control".to_string(),
            ));
        }
        let runtime = state.runtimes.get(world_id).ok_or_else(|| {
            IsolatedBrowserError::Unavailable(
                "isolated browser process is unavailable".to_string(),
            )
        })?;
        Ok((
            world.clone(),
            runtime.websocket_url.clone(),
            runtime.target_id.clone(),
        ))
    }

    async fn world_id_for_driver(
        &self,
        owner_profile_id: &str,
        driver_id: &str,
    ) -> Result<String, IsolatedBrowserError> {
        self.state
            .lock()
            .await
            .worlds
            .values()
            .find(|world| {
                world.owner_profile_id == owner_profile_id
                    && world.driver.driver_id.as_str() == driver_id
            })
            .map(|world| world.world_id.clone())
            .ok_or_else(|| {
                IsolatedBrowserError::NotFound(
                    "selected isolated browser driver is not owned by this profile".to_string(),
                )
            })
    }

    async fn update_page_identity(&self, world_id: &str, url: &str, title: &str) -> bool {
        if let Some(world) = self.state.lock().await.worlds.get_mut(world_id) {
            if world.url == url && world.title == title {
                return false;
            }
            world.url = url.to_string();
            world.title = title.to_string();
            world.updated_at_ms = now_ms();
            TabGroupManager::navigate_active_tab(
                &world.tab_group_id,
                url,
                Some(title),
                TabOpenedBy::Agent,
            );
            return true;
        }
        false
    }

    fn take_authoritative_control(
        &self,
        world: &IsolatedBrowserWorld,
    ) -> Result<(), IsolatedBrowserError> {
        crate::world_authority::take_owned_browser_control(
            &world.authority_id,
            world.driver.driver_id.as_str(),
            &world.tab_group_id,
            &format!("human:{}", world.owner_profile_id),
        )
        .map(|_| ())
        .map_err(IsolatedBrowserError::Conflict)
    }

    fn return_authoritative_control(
        &self,
        world: &IsolatedBrowserWorld,
    ) -> Result<(), IsolatedBrowserError> {
        crate::world_authority::return_owned_browser_control(
            &world.authority_id,
            world.driver.driver_id.as_str(),
            &world.tab_group_id,
            &format!("human:{}", world.owner_profile_id),
        )
        .map(|_| ())
        .map_err(IsolatedBrowserError::Conflict)
    }

    fn profile_directory(&self, world: &IsolatedBrowserWorld) -> PathBuf {
        match &world.profile {
            IsolatedBrowserProfile::Ephemeral => self
                .root
                .join("worlds")
                .join(world_suffix(&world.world_id))
                .join("profile"),
            IsolatedBrowserProfile::Persistent { profile_id } => {
                self.root.join("profiles").join(profile_id)
            }
        }
    }

    async fn launch_world(&self, world_id: &str) -> Result<(), IsolatedBrowserError> {
        let world = {
            let state = self.state.lock().await;
            if !state.runtimes.contains_key(world_id)
                && state.runtimes.len() >= MAX_RUNNING_WORLDS
            {
                return Err(IsolatedBrowserError::Unavailable(format!(
                    "workshop is at its {MAX_RUNNING_WORLDS}-browser runtime limit"
                )));
            }
            state.worlds.get(world_id).cloned().ok_or_else(|| {
                IsolatedBrowserError::NotFound("browser world not found".to_string())
            })?
        };
        ensure_authoritative_world(&world)?;
        let binary = tokio::task::spawn_blocking(resolve_browser_binary)
            .await
            .map_err(|error| {
                IsolatedBrowserError::Unavailable(format!(
                    "isolated browser discovery task failed: {error}"
                ))
            })?
            .ok_or_else(|| {
                IsolatedBrowserError::Unavailable(
                    "Chromium-family browser not found; install Chrome, Edge, or Chromium on this workshop, or set MEDOUSA_ISOLATED_BROWSER_BIN".to_string(),
                )
            })?;
        let profile_dir = self.profile_directory(&world);
        tokio::fs::create_dir_all(&profile_dir)
            .await
            .map_err(|error| store_error("create isolated browser profile", error))?;
        let active_port_path = profile_dir.join("DevToolsActivePort");
        match tokio::fs::remove_file(&active_port_path).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(store_error("clear stale Chromium endpoint", error)),
        }

        let mut command = Command::new(&binary);
        command
            .arg(format!("--user-data-dir={}", profile_dir.display()))
            .arg("--remote-debugging-address=127.0.0.1")
            .arg("--remote-debugging-port=0")
            .arg("--no-first-run")
            .arg("--no-default-browser-check")
            .arg("--disable-component-update")
            .arg("--window-size=1280,900")
            .arg("about:blank")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        if world.headless {
            command.arg("--headless=new");
        }
        #[cfg(target_os = "linux")]
        command.arg("--disable-dev-shm-usage");
        medousa_host::hide_tokio_subprocess_window(&mut command);

        let mut child = command.spawn().map_err(|error| {
            IsolatedBrowserError::Unavailable(format!(
                "start isolated browser {}: {error}",
                binary.display()
            ))
        })?;
        let websocket_url = match wait_for_devtools_endpoint(&mut child, &active_port_path).await {
            Ok(endpoint) => endpoint,
            Err(error) => {
                let _ = child.kill().await;
                return Err(error);
            }
        };
        TabGroupManager::ensure_group(&world.tab_group_id, world.driver.driver_id.as_str());
        TabGroupManager::set_control(&world.tab_group_id, BrowserControl::Agent);
        let target_id = match create_page_target(&websocket_url, &world.url).await {
            Ok(target_id) => target_id,
            Err(error) => {
                let _ = child.kill().await;
                return Err(error);
            }
        };
        let page = TabGroupManager::navigate_active_tab(
            &world.tab_group_id,
            &world.url,
            None,
            TabOpenedBy::Agent,
        )
        .ok_or_else(|| {
            IsolatedBrowserError::Store("isolated browser tab group disappeared".to_string())
        })?;

        let mut state = self.state.lock().await;
        let current = state.worlds.get_mut(world_id).ok_or_else(|| {
            IsolatedBrowserError::NotFound("browser world was removed during startup".to_string())
        })?;
        current.tab_id = Some(page.id);
        current.run_state = IsolatedBrowserRunState::Running;
        current.control = BrowserControl::Agent;
        current.failure = None;
        current.updated_at_ms = now_ms();
        state.runtimes.insert(
            world_id.to_string(),
            BrowserRuntime {
                child,
                websocket_url,
                target_id,
            },
        );
        drop(state);
        self.persist().await?;
        tracing::info!(%world_id, driver_id = %world.driver.driver_id, "isolated browser world running");
        Ok(())
    }

    async fn stop_runtime(&self, world_id: &str) -> Result<(), IsolatedBrowserError> {
        let runtime = self.state.lock().await.runtimes.remove(world_id);
        if let Some(mut runtime) = runtime {
            runtime.child.kill().await.map_err(|error| {
                IsolatedBrowserError::Driver(format!(
                    "stop isolated browser process for {world_id}: {error}"
                ))
            })?;
            let _ = runtime.child.wait().await;
        }
        Ok(())
    }

    async fn mark_failed(&self, world_id: &str, failure: String) {
        if let Some(world) = self.state.lock().await.worlds.get_mut(world_id) {
            world.run_state = IsolatedBrowserRunState::Failed;
            world.failure = Some(failure);
            world.tab_id = None;
            world.updated_at_ms = now_ms();
        }
    }

    async fn refresh_exited_processes(&self) {
        let mut state = self.state.lock().await;
        let mut exited = Vec::new();
        for (world_id, runtime) in &mut state.runtimes {
            match runtime.child.try_wait() {
                Ok(Some(status)) => exited.push((world_id.clone(), status.to_string())),
                Ok(None) => {}
                Err(error) => exited.push((world_id.clone(), error.to_string())),
            }
        }
        let changed = !exited.is_empty();
        for (world_id, detail) in exited {
            state.runtimes.remove(&world_id);
            if let Some(world) = state.worlds.get_mut(&world_id) {
                world.run_state = IsolatedBrowserRunState::Failed;
                world.failure = Some(format!("isolated browser process exited: {detail}"));
                world.tab_id = None;
                world.updated_at_ms = now_ms();
            }
        }
        drop(state);
        if changed && let Err(error) = self.persist().await {
            tracing::warn!(%error, "failed to persist isolated browser process exit");
        }
    }

    async fn persist(&self) -> Result<(), IsolatedBrowserError> {
        let catalog = {
            let state = self.state.lock().await;
            IsolatedBrowserCatalog {
                schema_version: CATALOG_SCHEMA_VERSION,
                worlds: state.worlds.values().cloned().collect(),
            }
        };
        let bytes = serde_json::to_vec_pretty(&catalog).map_err(|error| {
            IsolatedBrowserError::Store(format!("encode isolated browser catalog: {error}"))
        })?;
        let path = self.root.join("worlds.json");
        tokio::task::spawn_blocking(move || crate::session::atomic_write(&path, &bytes))
            .await
            .map_err(|error| {
                IsolatedBrowserError::Store(format!(
                    "isolated browser catalog write task failed: {error}"
                ))
            })?
            .map_err(|error| store_error("write isolated browser catalog", error))
    }
}

pub fn register_global_host(host: Arc<IsolatedBrowserHost>) -> Result<(), String> {
    GLOBAL_HOST
        .set(host)
        .map_err(|_| "isolated browser host is already registered".to_string())
}

pub fn global_host() -> Option<Arc<IsolatedBrowserHost>> {
    GLOBAL_HOST.get().cloned()
}

pub fn is_isolated_driver_id(driver_id: &str) -> bool {
    driver_id.trim().starts_with(DRIVER_PREFIX)
}

fn ensure_authoritative_world(world: &IsolatedBrowserWorld) -> Result<(), IsolatedBrowserError> {
    let registered = crate::world_authority::register_owned_browser_world(
        &world.authority_id,
        world.driver.driver_id.as_str(),
        &world.tab_group_id,
        &format!("human:{}", world.owner_profile_id),
    )
    .map_err(IsolatedBrowserError::Conflict)?;
    if registered.as_str() != world.world_id {
        return Err(IsolatedBrowserError::Conflict(
            "isolated browser catalog identity does not match world authority".to_string(),
        ));
    }
    Ok(())
}

fn isolated_driver_registration(
    driver_id: &str,
    display_name: Option<String>,
) -> WorldDriverRegistration {
    WorldDriverRegistration {
        driver_id: WorldDriverId::new(driver_id),
        kind: WorldDriverKind::IsolatedBrowser,
        surface: WorldSurfaceKind::Browser,
        ownership: WorldOwnership::Owned,
        transport: WorldDriverTransport::LocalSidecar,
        capabilities: [
            WorldDriverCapability::SemanticObservation,
            WorldDriverCapability::PixelObservation,
            WorldDriverCapability::Navigation,
            WorldDriverCapability::Interaction,
            WorldDriverCapability::GuardedBatch,
            WorldDriverCapability::HumanTakeover,
            WorldDriverCapability::PersistentProfile,
        ]
        .into_iter()
        .collect::<BTreeSet<_>>(),
        display_name: Some(display_name.unwrap_or_else(|| "Isolated browser".to_string())),
    }
}

fn owned_world<'a>(
    state: &'a HostState,
    owner_profile_id: &str,
    world_id: &str,
) -> Result<&'a IsolatedBrowserWorld, IsolatedBrowserError> {
    let world = state
        .worlds
        .get(world_id)
        .ok_or_else(|| IsolatedBrowserError::NotFound("browser world not found".to_string()))?;
    if world.owner_profile_id != owner_profile_id {
        return Err(IsolatedBrowserError::NotFound(
            "browser world not found".to_string(),
        ));
    }
    Ok(world)
}

fn owned_world_mut<'a>(
    state: &'a mut HostState,
    owner_profile_id: &str,
    world_id: &str,
) -> Result<&'a mut IsolatedBrowserWorld, IsolatedBrowserError> {
    let world = state
        .worlds
        .get_mut(world_id)
        .ok_or_else(|| IsolatedBrowserError::NotFound("browser world not found".to_string()))?;
    if world.owner_profile_id != owner_profile_id {
        return Err(IsolatedBrowserError::NotFound(
            "browser world not found".to_string(),
        ));
    }
    Ok(world)
}

fn ensure_profile_available(
    state: &HostState,
    profile: &IsolatedBrowserProfile,
    except_world_id: Option<&str>,
) -> Result<(), IsolatedBrowserError> {
    let IsolatedBrowserProfile::Persistent { profile_id } = profile else {
        return Ok(());
    };
    if state.worlds.values().any(|world| {
        Some(world.world_id.as_str()) != except_world_id
            && matches!(
                &world.profile,
                IsolatedBrowserProfile::Persistent { profile_id: current } if current == profile_id
            )
    }) {
        return Err(IsolatedBrowserError::Conflict(format!(
            "persistent browser profile '{profile_id}' is already attached to another world"
        )));
    }
    Ok(())
}

fn ensure_world_capacity(
    state: &HostState,
    owner_profile_id: &str,
) -> Result<(), IsolatedBrowserError> {
    if state.worlds.len() >= MAX_CATALOG_WORLDS {
        return Err(IsolatedBrowserError::Unavailable(format!(
            "isolated browser catalog is at its {MAX_CATALOG_WORLDS}-world limit"
        )));
    }
    if state
        .worlds
        .values()
        .filter(|world| world.owner_profile_id == owner_profile_id)
        .count()
        >= MAX_WORLDS_PER_PROFILE
    {
        return Err(IsolatedBrowserError::Unavailable(format!(
            "profile is at its {MAX_WORLDS_PER_PROFILE}-world isolated browser limit"
        )));
    }
    if state.runtimes.len() >= MAX_RUNNING_WORLDS {
        return Err(IsolatedBrowserError::Unavailable(format!(
            "workshop is at its {MAX_RUNNING_WORLDS}-browser runtime limit"
        )));
    }
    Ok(())
}

fn validate_owner(owner_profile_id: &str) -> Result<(), IsolatedBrowserError> {
    let owner = owner_profile_id.trim();
    if owner.is_empty() || owner.len() > 256 {
        return Err(IsolatedBrowserError::Invalid(
            "browser world owner profile is invalid".to_string(),
        ));
    }
    Ok(())
}

fn validate_profile(profile: &IsolatedBrowserProfile) -> Result<(), IsolatedBrowserError> {
    let IsolatedBrowserProfile::Persistent { profile_id } = profile else {
        return Ok(());
    };
    if profile_id.is_empty()
        || profile_id.len() > MAX_PROFILE_ID_BYTES
        || !profile_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(IsolatedBrowserError::Invalid(
            "persistent profile id must use 1-80 ASCII letters, numbers, '-' or '_'".to_string(),
        ));
    }
    Ok(())
}

fn validate_loaded_world(world: &IsolatedBrowserWorld) -> Result<(), IsolatedBrowserError> {
    validate_owner(&world.owner_profile_id)?;
    validate_profile(&world.profile)?;
    if !is_isolated_driver_id(world.driver.driver_id.as_str())
        || world.driver.kind != WorldDriverKind::IsolatedBrowser
        || world.driver.ownership != WorldOwnership::Owned
        || world.driver.surface != WorldSurfaceKind::Browser
        || world.world_id.trim().is_empty()
        || world.authority_id.trim().is_empty()
        || world.tab_group_id.trim().is_empty()
    {
        return Err(IsolatedBrowserError::Store(
            "isolated browser catalog contains an invalid world identity".to_string(),
        ));
    }
    normalize_browser_url(Some(&world.url)).map(|_| ())
}

fn normalize_display_name(value: Option<&str>) -> Result<Option<String>, IsolatedBrowserError> {
    let value = value.map(str::trim).filter(|value| !value.is_empty());
    if value.is_some_and(|value| value.len() > MAX_DISPLAY_NAME_BYTES) {
        return Err(IsolatedBrowserError::Invalid(format!(
            "browser world display name exceeds {MAX_DISPLAY_NAME_BYTES} bytes"
        )));
    }
    Ok(value.map(str::to_string))
}

fn normalize_browser_url(value: Option<&str>) -> Result<String, IsolatedBrowserError> {
    let value = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("about:blank");
    if value.len() > MAX_URL_BYTES {
        return Err(IsolatedBrowserError::Invalid(
            "browser URL exceeds its byte limit".to_string(),
        ));
    }
    if value == "about:blank" {
        return Ok(value.to_string());
    }
    let parsed = reqwest::Url::parse(value)
        .map_err(|error| IsolatedBrowserError::Invalid(format!("invalid browser URL: {error}")))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(IsolatedBrowserError::Invalid(
            "isolated browser navigation allows only http, https, or about:blank".to_string(),
        ));
    }
    Ok(parsed.to_string())
}

fn world_suffix(world_id: &str) -> &str {
    world_id.rsplit(':').next().unwrap_or(world_id)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn store_error(context: &str, error: std::io::Error) -> IsolatedBrowserError {
    IsolatedBrowserError::Store(format!("{context}: {error}"))
}

fn resolve_browser_binary() -> Option<PathBuf> {
    if let Some(explicit) = std::env::var_os("MEDOUSA_ISOLATED_BROWSER_BIN") {
        let path = PathBuf::from(explicit);
        if path.is_file() {
            return Some(path);
        }
    }
    if let Ok(executable) = std::env::current_exe()
        && let Some(directory) = executable.parent()
    {
        for name in browser_binary_names() {
            let candidate = directory.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    let data_bin = crate::paths::medousa_data_dir().join("bin");
    for name in browser_binary_names() {
        let candidate = data_bin.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    for path in platform_browser_paths() {
        if path.is_file() {
            return Some(path);
        }
    }
    let path = std::env::var_os("PATH")?;
    for directory in std::env::split_paths(&path) {
        for name in browser_binary_names() {
            let candidate = directory.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn browser_binary_names() -> &'static [&'static str] {
    if cfg!(windows) {
        &["chrome.exe", "msedge.exe", "chromium.exe"]
    } else {
        &[
            "chromium",
            "chromium-browser",
            "google-chrome",
            "google-chrome-stable",
            "chrome",
            "microsoft-edge",
        ]
    }
}

fn platform_browser_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if cfg!(target_os = "macos") {
        paths.extend([
            PathBuf::from("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"),
            PathBuf::from("/Applications/Chromium.app/Contents/MacOS/Chromium"),
            PathBuf::from("/Applications/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing"),
            PathBuf::from("/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge"),
        ]);
    }
    if cfg!(windows) {
        for root in ["PROGRAMFILES", "PROGRAMFILES(X86)", "LOCALAPPDATA"] {
            let Some(root) = std::env::var_os(root) else {
                continue;
            };
            let root = PathBuf::from(root);
            paths.push(root.join("Google/Chrome/Application/chrome.exe"));
            paths.push(root.join("Microsoft/Edge/Application/msedge.exe"));
            paths.push(root.join("Chromium/Application/chrome.exe"));
        }
    }
    if cfg!(target_os = "linux") {
        paths.extend([
            PathBuf::from("/usr/bin/google-chrome"),
            PathBuf::from("/usr/bin/google-chrome-stable"),
            PathBuf::from("/usr/bin/chromium"),
            PathBuf::from("/usr/bin/chromium-browser"),
            PathBuf::from("/usr/bin/microsoft-edge"),
        ]);
    }
    paths
}

async fn wait_for_devtools_endpoint(
    child: &mut Child,
    active_port_path: &Path,
) -> Result<String, IsolatedBrowserError> {
    for _ in 0..STARTUP_ATTEMPTS {
        if let Some(status) = child.try_wait().map_err(|error| {
            IsolatedBrowserError::Driver(format!(
                "inspect isolated browser during startup: {error}"
            ))
        })? {
            return Err(IsolatedBrowserError::Unavailable(format!(
                "isolated browser exited during startup: {status}"
            )));
        }
        if let Ok(contents) = tokio::fs::read_to_string(active_port_path).await {
            let mut lines = contents.lines();
            let port = lines.next().and_then(|value| value.trim().parse::<u16>().ok());
            let websocket_path = lines.next().map(str::trim).filter(|value| !value.is_empty());
            if let (Some(port), Some(websocket_path)) = (port, websocket_path) {
                return Ok(format!("ws://127.0.0.1:{port}{websocket_path}"));
            }
        }
        tokio::time::sleep(Duration::from_millis(STARTUP_POLL_MS)).await;
    }
    Err(IsolatedBrowserError::Unavailable(
        "isolated browser did not publish a DevTools endpoint within 10 seconds".to_string(),
    ))
}

struct CdpConnection {
    socket: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    next_id: u64,
}

impl CdpConnection {
    async fn connect(websocket_url: &str) -> Result<Self, IsolatedBrowserError> {
        let (socket, _) = tokio::time::timeout(CDP_TIMEOUT, connect_async(websocket_url))
            .await
            .map_err(|_| {
                IsolatedBrowserError::Driver(
                    "timed out connecting to isolated browser DevTools".to_string(),
                )
            })?
            .map_err(|error| {
                IsolatedBrowserError::Driver(format!(
                    "connect isolated browser DevTools: {error}"
                ))
            })?;
        Ok(Self { socket, next_id: 1 })
    }

    async fn call(
        &mut self,
        session_id: Option<&str>,
        method: &str,
        params: Value,
    ) -> Result<Value, IsolatedBrowserError> {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        let mut request = json!({ "id": id, "method": method, "params": params });
        if let Some(session_id) = session_id {
            request["sessionId"] = json!(session_id);
        }
        self.socket
            .send(Message::Text(request.to_string().into()))
            .await
            .map_err(|error| {
                IsolatedBrowserError::Driver(format!("send DevTools command {method}: {error}"))
            })?;

        let response = tokio::time::timeout(CDP_TIMEOUT, async {
            while let Some(message) = self.socket.next().await {
                let message = message.map_err(|error| {
                    IsolatedBrowserError::Driver(format!(
                        "read DevTools response for {method}: {error}"
                    ))
                })?;
                match message {
                    Message::Text(text) => {
                        let value: Value = serde_json::from_str(text.as_ref()).map_err(|error| {
                            IsolatedBrowserError::Driver(format!(
                                "decode DevTools response for {method}: {error}"
                            ))
                        })?;
                        if value.get("id").and_then(Value::as_u64) != Some(id) {
                            continue;
                        }
                        if let Some(error) = value.get("error") {
                            return Err(IsolatedBrowserError::Driver(format!(
                                "DevTools {method} failed: {error}"
                            )));
                        }
                        return Ok(value.get("result").cloned().unwrap_or(Value::Null));
                    }
                    Message::Ping(payload) => {
                        self.socket.send(Message::Pong(payload)).await.map_err(|error| {
                            IsolatedBrowserError::Driver(format!(
                                "reply to DevTools ping for {method}: {error}"
                            ))
                        })?;
                    }
                    Message::Close(_) => {
                        return Err(IsolatedBrowserError::Driver(format!(
                            "DevTools closed while waiting for {method}"
                        )));
                    }
                    _ => {}
                }
            }
            Err(IsolatedBrowserError::Driver(format!(
                "DevTools disconnected while waiting for {method}"
            )))
        })
        .await
        .map_err(|_| {
            IsolatedBrowserError::Driver(format!("DevTools command {method} timed out"))
        })??;
        Ok(response)
    }
}

async fn create_page_target(
    websocket_url: &str,
    initial_url: &str,
) -> Result<String, IsolatedBrowserError> {
    let mut connection = CdpConnection::connect(websocket_url).await?;
    let targets = connection
        .call(None, "Target.getTargets", json!({}))
        .await?;
    if let Some(target_id) = targets
        .get("targetInfos")
        .and_then(Value::as_array)
        .and_then(|targets| {
            targets.iter().find_map(|target| {
                (target.get("type").and_then(Value::as_str) == Some("page")
                    && target.get("url").and_then(Value::as_str) == Some("about:blank"))
                .then(|| target.get("targetId").and_then(Value::as_str))
                .flatten()
                .map(str::to_string)
            })
        })
    {
        if initial_url != "about:blank" {
            drop(connection);
            navigate_target(websocket_url, &target_id, initial_url).await?;
        }
        return Ok(target_id);
    }
    let result = connection
        .call(
            None,
            "Target.createTarget",
            json!({ "url": initial_url, "background": true }),
        )
        .await?;
    result
        .get("targetId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            IsolatedBrowserError::Driver(
                "DevTools createTarget response omitted targetId".to_string(),
            )
        })
}

async fn connect_page(
    websocket_url: &str,
    target_id: &str,
) -> Result<(CdpConnection, String), IsolatedBrowserError> {
    let mut connection = CdpConnection::connect(websocket_url).await?;
    let attached = connection
        .call(
            None,
            "Target.attachToTarget",
            json!({ "targetId": target_id, "flatten": true }),
        )
        .await?;
    let session_id = attached
        .get("sessionId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            IsolatedBrowserError::Driver(
                "DevTools attachToTarget response omitted sessionId".to_string(),
            )
        })?;
    connection
        .call(Some(&session_id), "Runtime.enable", json!({}))
        .await?;
    connection
        .call(Some(&session_id), "Page.enable", json!({}))
        .await?;
    Ok((connection, session_id))
}

async fn evaluate(
    connection: &mut CdpConnection,
    session_id: &str,
    context_id: Option<i64>,
    expression: String,
) -> Result<Value, IsolatedBrowserError> {
    let mut params = json!({
        "expression": expression,
        "returnByValue": true,
        "awaitPromise": true,
        "userGesture": true,
    });
    if let Some(context_id) = context_id {
        params["contextId"] = json!(context_id);
    }
    let result = connection
        .call(Some(session_id), "Runtime.evaluate", params)
        .await?;
    if let Some(exception) = result.get("exceptionDetails") {
        return Err(IsolatedBrowserError::Driver(format!(
            "browser script failed: {exception}"
        )));
    }
    Ok(result
        .get("result")
        .and_then(|value| value.get("value"))
        .cloned()
        .unwrap_or(Value::Null))
}

async fn isolated_execution_context(
    connection: &mut CdpConnection,
    session_id: &str,
) -> Result<i64, IsolatedBrowserError> {
    let tree = connection
        .call(Some(session_id), "Page.getFrameTree", json!({}))
        .await?;
    let frame_id = tree
        .pointer("/frameTree/frame/id")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            IsolatedBrowserError::Driver(
                "DevTools frame tree omitted the main frame id".to_string(),
            )
        })?;
    let result = connection
        .call(
            Some(session_id),
            "Page.createIsolatedWorld",
            json!({
                "frameId": frame_id,
                "worldName": "medousa-governed-world",
                "grantUniveralAccess": false,
            }),
        )
        .await?;
    result
        .get("executionContextId")
        .and_then(Value::as_i64)
        .ok_or_else(|| {
            IsolatedBrowserError::Driver(
                "DevTools isolated world response omitted executionContextId".to_string(),
            )
        })
}

async fn navigate_target(
    websocket_url: &str,
    target_id: &str,
    url: &str,
) -> Result<(), IsolatedBrowserError> {
    let (mut connection, session_id) = connect_page(websocket_url, target_id).await?;
    let result = connection
        .call(
            Some(&session_id),
            "Page.navigate",
            json!({ "url": url }),
        )
        .await?;
    if let Some(error) = result.get("errorText").and_then(Value::as_str) {
        return Err(IsolatedBrowserError::Driver(format!(
            "isolated browser navigation failed: {error}"
        )));
    }
    let deadline = tokio::time::Instant::now() + CDP_TIMEOUT;
    loop {
        let ready = evaluate(
            &mut connection,
            &session_id,
            None,
            "document.readyState".to_string(),
        )
        .await?
        .as_str()
        .is_some_and(|state| matches!(state, "interactive" | "complete"));
        if ready {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(IsolatedBrowserError::Driver(
                "isolated browser navigation did not become interactive".to_string(),
            ));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn page_identity(
    websocket_url: &str,
    target_id: &str,
) -> Result<(String, String), IsolatedBrowserError> {
    let (mut connection, session_id) = connect_page(websocket_url, target_id).await?;
    let value = evaluate(
        &mut connection,
        &session_id,
        None,
        "({ url: location.href, title: document.title || '' })".to_string(),
    )
    .await?;
    let url = value
        .get("url")
        .and_then(Value::as_str)
        .unwrap_or("about:blank")
        .to_string();
    let title = value
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    Ok((url, title))
}

#[derive(Debug, Deserialize)]
struct RawObservation {
    url: String,
    title: String,
    document_id: String,
    viewport: BrowserObservationViewport,
    nodes: Vec<BrowserSemanticNode>,
    truncated: bool,
}

async fn capture_observation(
    websocket_url: &str,
    target_id: &str,
    tab_id: &str,
    max_nodes: usize,
) -> Result<BrowserObservationCapture, IsolatedBrowserError> {
    let max_nodes = max_nodes.clamp(1, 1_024);
    let (mut connection, session_id) = connect_page(websocket_url, target_id).await?;
    let context_id = isolated_execution_context(&mut connection, &session_id).await?;
    let value = evaluate(
        &mut connection,
        &session_id,
        Some(context_id),
        observation_script(max_nodes),
    )
    .await?;
    let raw: RawObservation = serde_json::from_value(value).map_err(|error| {
        IsolatedBrowserError::Driver(format!(
            "decode isolated browser semantic observation: {error}"
        ))
    })?;
    Ok(BrowserObservationCapture {
        tab_id: tab_id.to_string(),
        url: raw.url,
        title: raw.title,
        document_id: raw.document_id,
        viewport: raw.viewport,
        nodes: raw.nodes,
        truncated: raw.truncated,
        unchanged: false,
        captured_at_ms: now_ms(),
    })
}

fn observation_script(max_nodes: usize) -> String {
    format!(
        r#"(() => {{
  const root = globalThis;
  if (!root.__medousaWorldState || root.__medousaWorldState.document !== document) {{
    root.__medousaWorldState = {{
      document,
      documentId: `doc-${{Date.now().toString(36)}}-${{Math.random().toString(36).slice(2)}}`,
      refs: new WeakMap(),
      elements: new Map(),
      nextRef: 1,
    }};
  }}
  const state = root.__medousaWorldState;
  state.elements.clear();
  const refFor = (element) => {{
    let ref = state.refs.get(element);
    if (!ref) {{
      ref = `el-${{state.nextRef++}}`;
      state.refs.set(element, ref);
    }}
    state.elements.set(ref, element);
    return ref;
  }};
  const implicitRole = (element) => {{
    const tag = element.tagName.toLowerCase();
    if (tag === 'a' && element.hasAttribute('href')) return 'link';
    if (tag === 'button') return 'button';
    if (tag === 'textarea') return 'textbox';
    if (tag === 'select') return 'combobox';
    if (element.isContentEditable) return 'textbox';
    if (tag === 'img') return 'img';
    if (/^h[1-6]$/.test(tag)) return 'heading';
    if (tag === 'li') return 'listitem';
    if (tag === 'input') {{
      const type = (element.type || 'text').toLowerCase();
      if (['checkbox', 'radio', 'button', 'submit'].includes(type)) return type;
      return 'textbox';
    }}
    return tag === 'p' ? 'paragraph' : 'generic';
  }};
  const text = (value, limit = 1024) => String(value || '').replace(/\s+/g, ' ').trim().slice(0, limit);
  const sensitive = (element) => {{
    const type = String(element.getAttribute('type') || '').toLowerCase();
    const autocomplete = String(element.getAttribute('autocomplete') || '').toLowerCase();
    const semantics = `${{element.getAttribute('name') || ''}} ${{element.id || ''}} ${{element.getAttribute('aria-label') || ''}}`.toLowerCase();
    return type === 'password'
      || /(cc-|credit|card|cvc|cvv|security-code|one-time-code|current-password|new-password)/.test(`${{autocomplete}} ${{semantics}}`)
      || type === 'file';
  }};
  const candidates = Array.from(document.querySelectorAll(
    'a,button,input,textarea,select,summary,label,[role],[contenteditable],h1,h2,h3,h4,h5,h6,p,li'
  )).filter((element) => {{
    const style = getComputedStyle(element);
    const rect = element.getBoundingClientRect();
    return style.display !== 'none' && style.visibility !== 'hidden' && rect.width > 0 && rect.height > 0;
  }});
  const nodes = candidates.slice(0, {max_nodes}).map((element) => {{
    const rect = element.getBoundingClientRect();
    const isSensitive = sensitive(element);
    const tag = element.tagName.toLowerCase();
    const value = !isSensitive && ['input', 'textarea', 'select'].includes(tag)
      ? text(element.value)
      : undefined;
    const name = text(
      element.getAttribute('aria-label')
        || element.getAttribute('alt')
        || element.getAttribute('title')
        || element.innerText
        || element.textContent
    );
    return {{
      element_ref: refFor(element),
      parent_ref: element.parentElement ? refFor(element.parentElement) : undefined,
      role: text(element.getAttribute('role') || implicitRole(element), 64),
      name,
      tag,
      value,
      href: tag === 'a' && element.href ? String(element.href).slice(0, 8192) : undefined,
      disabled: Boolean(element.disabled || element.getAttribute('aria-disabled') === 'true'),
      checked: typeof element.checked === 'boolean' ? element.checked : undefined,
      selected: typeof element.selected === 'boolean' ? element.selected : undefined,
      bounds: {{
        x: Math.round(rect.x), y: Math.round(rect.y),
        width: Math.max(0, Math.round(rect.width)),
        height: Math.max(0, Math.round(rect.height)),
      }},
      sensitive: isSensitive,
    }};
  }});
  return {{
    url: String(location.href).slice(0, 8192),
    title: text(document.title, 512),
    document_id: state.documentId,
    viewport: {{
      width: Math.max(1, Math.round(innerWidth)),
      height: Math.max(1, Math.round(innerHeight)),
      scroll_x: Math.round(scrollX),
      scroll_y: Math.round(scrollY),
      device_scale_factor: Number(devicePixelRatio || 1),
    }},
    nodes,
    truncated: candidates.length > {max_nodes},
  }};
}})()"#
    )
}

async fn capture_screenshot(
    websocket_url: &str,
    target_id: &str,
    tab_id: &str,
    observation: &BrowserObservation,
    max_width: u32,
) -> Result<BrowserScreenshotCapture, IsolatedBrowserError> {
    let (mut connection, session_id) = connect_page(websocket_url, target_id).await?;
    let context_id = isolated_execution_context(&mut connection, &session_id).await?;
    let redacted = evaluate(
        &mut connection,
        &session_id,
        Some(context_id),
        REDACT_SENSITIVE_SCRIPT.to_string(),
    )
    .await?
    .as_u64()
    .unwrap_or_default() as usize;
    let width = observation.viewport.width.max(1);
    let height = observation.viewport.height.max(1);
    let scale = (max_width as f64 / width as f64).min(1.0);
    let captured = connection
        .call(
            Some(&session_id),
            "Page.captureScreenshot",
            json!({
                "format": "png",
                "fromSurface": true,
                "captureBeyondViewport": false,
                "clip": {
                    "x": observation.viewport.scroll_x.max(0),
                    "y": observation.viewport.scroll_y.max(0),
                    "width": width,
                    "height": height,
                    "scale": scale,
                },
            }),
        )
        .await;
    let _ = evaluate(
        &mut connection,
        &session_id,
        Some(context_id),
        CLEAR_REDACTION_SCRIPT.to_string(),
    )
    .await;
    let data = captured?
        .get("data")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            IsolatedBrowserError::Driver(
                "DevTools screenshot response omitted image data".to_string(),
            )
        })?
        .to_string();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&data)
        .map_err(|error| {
            IsolatedBrowserError::Driver(format!("decode isolated browser screenshot: {error}"))
        })?;
    if bytes.len() > MAX_SCREENSHOT_BYTES {
        return Err(IsolatedBrowserError::Driver(
            "isolated browser screenshot exceeds 12 MB".to_string(),
        ));
    }
    let image = image::load_from_memory(&bytes).map_err(|error| {
        IsolatedBrowserError::Driver(format!("decode isolated browser screenshot image: {error}"))
    })?;
    let (image_width, image_height) = image.dimensions();
    let sha256 = format!("{:x}", Sha256::digest(&bytes));
    Ok(BrowserScreenshotCapture {
        schema_version: BROWSER_SCREENSHOT_SCHEMA_VERSION,
        tab_id: tab_id.to_string(),
        url: observation.url.clone(),
        title: observation.title.clone(),
        document_id: observation.document_id.clone(),
        observation_revision: observation.revision,
        viewport: observation.viewport.clone(),
        coordinate_frame: "css_viewport".to_string(),
        mime: "image/png".to_string(),
        image_width,
        image_height,
        byte_size: bytes.len(),
        sha256,
        sensitive_regions_redacted: redacted,
        captured_at_ms: now_ms(),
        untrusted_content: true,
        image_base64: data,
    })
}

const REDACT_SENSITIVE_SCRIPT: &str = r#"(() => {
  for (const existing of document.querySelectorAll('[data-medousa-redaction]')) existing.remove();
  const candidates = Array.from(document.querySelectorAll('input,textarea,[contenteditable="true"]'));
  let count = 0;
  for (const element of candidates) {
    const type = String(element.getAttribute('type') || '').toLowerCase();
    const autocomplete = String(element.getAttribute('autocomplete') || '').toLowerCase();
    const semantics = `${element.getAttribute('name') || ''} ${element.id || ''} ${element.getAttribute('aria-label') || ''}`.toLowerCase();
    const sensitive = type === 'password'
      || type === 'file'
      || /(cc-|credit|card|cvc|cvv|security-code|one-time-code|current-password|new-password)/.test(`${autocomplete} ${semantics}`);
    if (!sensitive) continue;
    const rect = element.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) continue;
    const mask = document.createElement('div');
    mask.setAttribute('data-medousa-redaction', '');
    Object.assign(mask.style, {
      position: 'fixed', left: `${rect.left}px`, top: `${rect.top}px`,
      width: `${rect.width}px`, height: `${rect.height}px`,
      background: '#111', zIndex: '2147483647', pointerEvents: 'none',
    });
    document.documentElement.appendChild(mask);
    count += 1;
  }
  return count;
})()"#;

const CLEAR_REDACTION_SCRIPT: &str = r#"(() => {
  for (const element of document.querySelectorAll('[data-medousa-redaction]')) element.remove();
  return true;
})()"#;

#[derive(Debug, Clone, Deserialize)]
struct BrowserActionGuard {
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    value: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct BrowserActionStep {
    action: String,
    #[serde(default)]
    target_ref: Option<String>,
    #[serde(default)]
    selector: Option<String>,
    #[serde(default)]
    guard: Option<BrowserActionGuard>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    delta_y: Option<i64>,
    #[serde(default)]
    value: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    ms: Option<i64>,
}

fn parse_action_steps(body: &Value) -> Result<Vec<BrowserActionStep>, IsolatedBrowserError> {
    let raw = body
        .get("actions")
        .cloned()
        .unwrap_or_else(|| body.clone());
    let steps = if raw.is_array() {
        serde_json::from_value::<Vec<BrowserActionStep>>(raw)
    } else {
        serde_json::from_value::<BrowserActionStep>(raw).map(|step| vec![step])
    }
    .map_err(|error| {
        IsolatedBrowserError::Invalid(format!("decode isolated browser action: {error}"))
    })?;
    if steps.is_empty() || steps.len() > MAX_ACTION_STEPS {
        return Err(IsolatedBrowserError::Invalid(format!(
            "isolated browser action requires 1-{MAX_ACTION_STEPS} steps"
        )));
    }
    for step in &steps {
        if !matches!(
            step.action.as_str(),
            "click" | "type" | "press" | "scroll" | "select" | "wait" | "navigate"
        ) {
            return Err(IsolatedBrowserError::Invalid(format!(
                "unsupported isolated browser action '{}'",
                step.action
            )));
        }
        if matches!(step.action.as_str(), "click" | "type" | "press" | "select")
            && step.target_ref.as_deref().is_none_or(str::is_empty)
            && step.selector.as_deref().is_none_or(str::is_empty)
        {
            return Err(IsolatedBrowserError::Invalid(format!(
                "isolated browser {} requires target_ref or selector",
                step.action
            )));
        }
        if step.action == "navigate" {
            normalize_browser_url(step.url.as_deref())?;
        }
    }
    Ok(steps)
}

fn validate_action_permit(
    world: &IsolatedBrowserWorld,
    permit: &WorldActionPermit,
    body: &Value,
    tab_id: &str,
) -> Result<(), IsolatedBrowserError> {
    if permit.world_id.as_str() != world.world_id
        || permit.driver_id != world.driver.driver_id
        || permit.resource_id.as_str() != format!("browser-tab:{tab_id}")
    {
        return Err(IsolatedBrowserError::Conflict(
            "world action permit is bound to another browser resource".to_string(),
        ));
    }
    if permit.expires_at_ms <= now_ms() {
        return Err(IsolatedBrowserError::Conflict(
            "world action permit expired before browser dispatch".to_string(),
        ));
    }
    let expected_url = body
        .get("world_expected_url")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            IsolatedBrowserError::Invalid(
                "isolated browser action is missing its expected URL".to_string(),
            )
        })?;
    if !same_browser_url(expected_url, &world.url) {
        return Err(IsolatedBrowserError::Conflict(
            "isolated browser navigated after action admission".to_string(),
        ));
    }
    Ok(())
}

async fn execute_action_step(
    connection: &mut CdpConnection,
    session_id: &str,
    context_id: i64,
    step: &BrowserActionStep,
) -> Result<(), IsolatedBrowserError> {
    if step.action == "wait" {
        let ms = step.ms.unwrap_or(1_000).clamp(0, 30_000) as u64;
        tokio::time::sleep(Duration::from_millis(ms)).await;
        return Ok(());
    }
    if step.action == "navigate" {
        let url = normalize_browser_url(step.url.as_deref())?;
        let result = connection
            .call(Some(session_id), "Page.navigate", json!({ "url": url }))
            .await?;
        if let Some(error) = result.get("errorText").and_then(Value::as_str) {
            return Err(IsolatedBrowserError::Driver(format!(
                "isolated browser navigation failed: {error}"
            )));
        }
        return Ok(());
    }

    let payload = serde_json::to_string(&json!({
        "action": step.action,
        "targetRef": step.target_ref,
        "selector": step.selector,
        "guard": step.guard.as_ref().map(|guard| json!({
            "role": guard.role,
            "name": guard.name,
            "value": guard.value,
        })),
        "text": step.text,
        "key": step.key,
        "deltaY": step.delta_y,
        "value": step.value,
    }))
    .map_err(|error| {
        IsolatedBrowserError::Invalid(format!("encode isolated browser action: {error}"))
    })?;
    let result = evaluate(
        connection,
        session_id,
        Some(context_id),
        action_script(&payload),
    )
    .await?;
    if !result.get("ok").and_then(Value::as_bool).unwrap_or(false) {
        return Err(IsolatedBrowserError::Driver(
            result
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("isolated browser action failed")
                .to_string(),
        ));
    }
    Ok(())
}

fn action_script(payload: &str) -> String {
    format!(
        r#"(() => {{
  const command = {payload};
  const state = globalThis.__medousaWorldState;
  let element = null;
  if (command.targetRef && state && state.document === document) {{
    element = state.elements.get(command.targetRef) || null;
  }}
  if (!element && command.selector) {{
    try {{ element = document.querySelector(command.selector); }}
    catch (error) {{ return {{ ok: false, error: `invalid selector: ${{error}}` }}; }}
  }}
  if (command.action === 'scroll') {{
    scrollBy({{ top: Number(command.deltaY || 0), behavior: 'instant' }});
    return {{ ok: true }};
  }}
  if (!element || !element.isConnected) return {{ ok: false, error: 'target is stale or missing' }};
  const implicitRole = (target) => {{
    const tag = target.tagName.toLowerCase();
    if (tag === 'a' && target.hasAttribute('href')) return 'link';
    if (tag === 'button') return 'button';
    if (tag === 'textarea' || target.isContentEditable) return 'textbox';
    if (tag === 'select') return 'combobox';
    if (tag === 'input') {{
      const type = String(target.type || 'text').toLowerCase();
      if (['checkbox', 'radio', 'button', 'submit'].includes(type)) return type;
      return 'textbox';
    }}
    return tag;
  }};
  const role = String(element.getAttribute('role') || implicitRole(element)).toLowerCase();
  const name = String(element.getAttribute('aria-label') || element.innerText || element.textContent || '').replace(/\s+/g, ' ').trim();
  const value = 'value' in element ? String(element.value || '') : '';
  if (command.guard) {{
    if (command.guard.role && role !== String(command.guard.role).toLowerCase()) return {{ ok: false, error: 'role precondition failed' }};
    if (command.guard.name && name !== String(command.guard.name)) return {{ ok: false, error: 'name precondition failed' }};
    if (command.guard.value && value !== String(command.guard.value)) return {{ ok: false, error: 'value precondition failed' }};
  }}
  element.scrollIntoView({{ block: 'center', inline: 'center' }});
  element.focus();
  if (command.action === 'click') element.click();
  else if (command.action === 'type') {{
    const next = String(command.text || '');
    if (element.isContentEditable) {{
      element.textContent = next;
    }} else if (element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement) {{
      const prototype = element instanceof HTMLTextAreaElement
        ? HTMLTextAreaElement.prototype
        : HTMLInputElement.prototype;
      const setter = Object.getOwnPropertyDescriptor(prototype, 'value')?.set;
      if (setter) setter.call(element, next); else element.value = next;
    }} else {{
      return {{ ok: false, error: 'type target is not an editable control' }};
    }}
    element.dispatchEvent(new InputEvent('input', {{ bubbles: true, inputType: 'insertText', data: next }}));
    element.dispatchEvent(new Event('change', {{ bubbles: true }}));
  }} else if (command.action === 'press') {{
    const key = String(command.key || 'Enter');
    element.dispatchEvent(new KeyboardEvent('keydown', {{ key, bubbles: true }}));
    element.dispatchEvent(new KeyboardEvent('keyup', {{ key, bubbles: true }}));
  }} else if (command.action === 'select') {{
    if (!(element instanceof HTMLSelectElement)) return {{ ok: false, error: 'select target is not a select control' }};
    element.value = String(command.value || '');
    element.dispatchEvent(new Event('input', {{ bubbles: true }}));
    element.dispatchEvent(new Event('change', {{ bubbles: true }}));
  }}
  return {{ ok: true }};
}})()"#
    )
}

fn same_browser_url(left: &str, right: &str) -> bool {
    left.trim().trim_end_matches('/') == right.trim().trim_end_matches('/')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persistent_profile_ids_are_path_components_not_paths() {
        assert!(validate_profile(&IsolatedBrowserProfile::Persistent {
            profile_id: "work-profile_2".to_string(),
        })
        .is_ok());
        for value in ["../escape", "nested/profile", "", "spaces are loud"] {
            assert!(validate_profile(&IsolatedBrowserProfile::Persistent {
                profile_id: value.to_string(),
            })
            .is_err());
        }
    }

    #[test]
    fn isolated_navigation_rejects_active_content_schemes() {
        assert!(normalize_browser_url(Some("https://example.com/path")).is_ok());
        assert!(normalize_browser_url(Some("http://127.0.0.1:3000")).is_ok());
        assert!(normalize_browser_url(Some("about:blank")).is_ok());
        assert!(normalize_browser_url(Some("javascript:alert(1)")).is_err());
        assert!(normalize_browser_url(Some("file:///etc/passwd")).is_err());
    }

    #[test]
    fn isolated_driver_inventory_is_owned_and_profile_capable() {
        let registration = isolated_driver_registration("driver:isolated-browser:test", None);
        assert_eq!(registration.kind, WorldDriverKind::IsolatedBrowser);
        assert_eq!(registration.ownership, WorldOwnership::Owned);
        assert!(
            registration
                .capabilities
                .contains(&WorldDriverCapability::PersistentProfile)
        );
    }

    #[tokio::test]
    async fn catalog_recovery_preserves_owned_identity_and_requires_resume() {
        let root = std::env::temp_dir().join(format!(
            "medousa-isolated-browser-recovery-{}",
            Uuid::new_v4().simple()
        ));
        tokio::fs::create_dir_all(root.join("worlds"))
            .await
            .unwrap();
        tokio::fs::create_dir_all(root.join("profiles"))
            .await
            .unwrap();
        let owner = format!("user:test-{}", Uuid::new_v4().simple());
        let authority = format!("workshop:test-{}", Uuid::new_v4().simple());
        let driver_id = format!("{DRIVER_PREFIX}{}", Uuid::new_v4().simple());
        let tab_group = TabGroupManager::create_group(&driver_id, None, None);
        let world_id = crate::world_authority::register_owned_browser_world(
            &authority,
            &driver_id,
            &tab_group.id,
            &format!("human:{owner}"),
        )
        .unwrap();
        let world = IsolatedBrowserWorld {
            world_id: world_id.to_string(),
            owner_profile_id: owner.clone(),
            authority_id: authority,
            driver: isolated_driver_registration(&driver_id, None),
            tab_group_id: tab_group.id,
            tab_id: Some("tab-before-restart".to_string()),
            profile: IsolatedBrowserProfile::Persistent {
                profile_id: "work".to_string(),
            },
            run_state: IsolatedBrowserRunState::Running,
            control: BrowserControl::Agent,
            control_epoch: 0,
            view_attached: false,
            headless: true,
            url: "https://example.com/".to_string(),
            title: "Example".to_string(),
            failure: None,
            created_at_ms: now_ms(),
            updated_at_ms: now_ms(),
        };
        let catalog = IsolatedBrowserCatalog {
            schema_version: CATALOG_SCHEMA_VERSION,
            worlds: vec![world],
        };
        tokio::fs::write(
            root.join("worlds.json"),
            serde_json::to_vec_pretty(&catalog).unwrap(),
        )
        .await
        .unwrap();

        let host = IsolatedBrowserHost::open(root.clone()).await.unwrap();
        let recovered = host.list(&owner).await.pop().unwrap();
        assert_eq!(recovered.run_state, IsolatedBrowserRunState::Stopped);
        assert_eq!(recovered.control_epoch, 1);
        assert!(recovered.tab_id.is_none());
        assert!(recovered.failure.as_deref().unwrap().contains("restarted"));

        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    #[ignore = "requires an installed Chromium-family browser"]
    async fn installed_chromium_runs_the_governed_contract() {
        if resolve_browser_binary().is_none() {
            return;
        }
        let root = std::env::temp_dir().join(format!(
            "medousa-isolated-browser-smoke-{}",
            Uuid::new_v4().simple()
        ));
        let host = IsolatedBrowserHost::open(root.clone()).await.unwrap();
        let owner = format!("user:test-{}", Uuid::new_v4().simple());
        let authority = format!("workshop:test-{}", Uuid::new_v4().simple());
        let world = host
            .create(
                &owner,
                &authority,
                CreateIsolatedBrowserWorldRequest {
                    display_name: Some("Smoke browser".to_string()),
                    profile: Some(IsolatedBrowserProfile::Ephemeral),
                    initial_url: Some("about:blank".to_string()),
                    headless: true,
                },
            )
            .await
            .unwrap();
        assert_eq!(world.run_state, IsolatedBrowserRunState::Running);
        let (_, websocket_url, target_id) = host
            .runtime_binding(&owner, &world.world_id, true)
            .await
            .unwrap();
        let (mut connection, session_id) = connect_page(&websocket_url, &target_id).await.unwrap();
        let context_id = isolated_execution_context(&mut connection, &session_id)
            .await
            .unwrap();
        evaluate(
            &mut connection,
            &session_id,
            Some(context_id),
            "document.body.innerHTML = '<input aria-label=\"Name\"><button aria-label=\"Save\">Save</button>'; document.querySelector('button').onclick = () => { document.title = 'saved'; }; true"
                .to_string(),
        )
        .await
        .unwrap();
        let observation = host.observe(&owner, &world.world_id, None, 16).await.unwrap();
        assert_eq!(observation.url, "about:blank");
        let input_ref = observation
            .nodes
            .iter()
            .find(|node| node.name == "Name")
            .map(|node| node.element_ref.clone())
            .unwrap();
        let button_ref = observation
            .nodes
            .iter()
            .find(|node| node.name == "Save")
            .map(|node| node.element_ref.clone())
            .unwrap();
        crate::world_authority::record_browser_observation(
            &crate::world_authority::admit_browser_observation(
                &authority,
                world.driver.driver_id.as_str(),
                &world.tab_group_id,
                world.tab_id.as_deref().unwrap(),
                "isolated-smoke-observe",
                "observe smoke controls",
            )
            .unwrap(),
            observation.clone(),
        )
        .unwrap();
        let action_admission = crate::world_authority::admit_browser_action(
            &authority,
            world.driver.driver_id.as_str(),
            &world.tab_group_id,
            world.tab_id.as_deref().unwrap(),
            "isolated-smoke-action",
            "type a name and save",
            medousa_world::WorldEffectClass::LocalMutation,
        )
        .unwrap();
        let action = host
            .act_for_driver(
                &owner,
                world.driver.driver_id.as_str(),
                json!({
                    "world_permit": action_admission.permit,
                    "world_expected_url": "about:blank",
                    "actions": [
                        { "action": "type", "target_ref": input_ref, "text": "Medousa" },
                        { "action": "click", "target_ref": button_ref }
                    ]
                }),
            )
            .await
            .unwrap();
        assert_eq!(action["executed_steps"], 2);
        assert_eq!(action["observation"]["title"], "saved");
        assert!(action["observation"]["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|node| node["value"] == "Medousa"));
        let action_observation: BrowserObservation =
            serde_json::from_value(action["observation"].clone()).unwrap();
        crate::world_authority::complete_browser_action(
            &action_admission,
            "smoke action completed",
        )
        .unwrap();
        let screenshot = host
            .screenshot(
                &owner,
                &world.world_id,
                &action_observation.document_id,
                action_observation.revision,
                640,
            )
            .await
            .unwrap();
        assert_eq!(screenshot.mime, "image/png");
        assert!(screenshot.byte_size > 0);
        let detached = host
            .lifecycle(
                &owner,
                &world.world_id,
                IsolatedBrowserLifecycleAction::DetachView,
            )
            .await
            .unwrap();
        assert!(!detached.view_attached);
        assert_eq!(detached.run_state, IsolatedBrowserRunState::Running);
        host.cleanup(&owner, &world.world_id, false).await.unwrap();
        let _ = tokio::fs::remove_dir_all(root).await;
    }
}
