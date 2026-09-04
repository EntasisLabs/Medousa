//! Immutable per-turn execution identity and exact live-owner lifecycle.

use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::request_principal::RequestPrincipal;
use crate::session_storage::SessionId;
use crate::turn_scope::TurnContinuationScope;

/// Daemon-resolved Bot snapshot captured once at turn admission.
///
/// This is context, never authority: tool and execution access continues to
/// come from the request principal, mode, Specialist, and runtime policy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BotTurnIdentity {
    bot_id: medousa_types::BotId,
    profile_revision: u64,
    memory_scope_id: Arc<str>,
    display_name: Arc<str>,
    role_description: Option<Arc<str>>,
    default_mode: Option<medousa_types::AgentModeId>,
}

impl BotTurnIdentity {
    pub fn from_profile(profile: &medousa_types::BotProfile) -> Self {
        Self {
            bot_id: profile.bot_id.clone(),
            profile_revision: profile.revision,
            memory_scope_id: Arc::from(profile.memory_scope_id.as_str()),
            display_name: Arc::from(profile.display_name.as_str()),
            role_description: profile.role_description.as_deref().map(Arc::<str>::from),
            default_mode: profile.default_mode,
        }
    }

    pub fn bot_id(&self) -> &medousa_types::BotId {
        &self.bot_id
    }

    pub fn profile_revision(&self) -> u64 {
        self.profile_revision
    }

    pub fn memory_scope_id(&self) -> &str {
        &self.memory_scope_id
    }

    pub fn default_mode(&self) -> Option<medousa_types::AgentModeId> {
        self.default_mode
    }

    pub fn prompt_appendix(&self) -> String {
        let display_name = prompt_line(&self.display_name, 80);
        let role_description = self
            .role_description
            .as_deref()
            .map(|value| prompt_line(value, 500))
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "none".to_string());
        format!(
            "[MEDOUSA_BOT_PROFILE]\n\
             bot_id={}\n\
             profile_revision={}\n\
             display_name={}\n\
             role_description={}\n\
             authority=none (identity and relationship context only)",
            self.bot_id, self.profile_revision, display_name, role_description,
        )
    }
}

fn prompt_line(value: &str, max_chars: usize) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(max_chars)
        .collect()
}

/// Daemon-issued identity for one live turn generation.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct TurnHandle(Uuid);

impl TurnHandle {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for TurnHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for TurnHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("TurnHandle")
            .field(&self.0.simple().to_string())
            .finish()
    }
}

impl fmt::Display for TurnHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0.simple())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderRoute {
    provider: Arc<str>,
    model: Arc<str>,
}

impl ProviderRoute {
    pub fn new(provider: impl Into<Arc<str>>, model: impl Into<Arc<str>>) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
        }
    }

    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn model(&self) -> &str {
        &self.model
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SurfaceCapabilities {
    pub ui_artifacts: bool,
    pub liquid_markdown: bool,
    pub browser_host: bool,
}

/// All values selected at admission and immutable for the execution lifetime.
#[derive(Debug)]
pub struct TurnExecutionContext {
    handle: TurnHandle,
    turn_id: Arc<str>,
    correlation_id: Arc<str>,
    session_id: SessionId,
    principal: RequestPrincipal,
    route: ProviderRoute,
    surface: SurfaceCapabilities,
    cancellation: CancellationToken,
    deadline: Instant,
    legacy_scope: Arc<TurnContinuationScope>,
    bot: Option<BotTurnIdentity>,
    work_environment: Option<medousa_runtime::WorkEnvironmentBinding>,
    work_environment_operation_sequence: AtomicU64,
    last_grapheme_source: Mutex<Option<Arc<str>>>,
    tasks: Arc<TurnTaskGroup>,
}

impl TurnExecutionContext {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        turn_id: impl Into<Arc<str>>,
        correlation_id: impl Into<Arc<str>>,
        session_id: SessionId,
        principal: RequestPrincipal,
        route: ProviderRoute,
        surface: SurfaceCapabilities,
        cancellation: CancellationToken,
        deadline: Instant,
        legacy_scope: TurnContinuationScope,
    ) -> Self {
        let tasks = Arc::new(TurnTaskGroup::new(cancellation.clone(), 64));
        Self {
            handle: TurnHandle::new(),
            turn_id: turn_id.into(),
            correlation_id: correlation_id.into(),
            session_id,
            principal,
            route,
            surface,
            cancellation,
            deadline,
            legacy_scope: Arc::new(legacy_scope),
            bot: None,
            work_environment: None,
            work_environment_operation_sequence: AtomicU64::new(0),
            last_grapheme_source: Mutex::new(None),
            tasks,
        }
    }

    /// Build an execution owner from the compatibility scope at an admission
    /// boundary. Keeping this conversion here makes route, surface, and
    /// authority derivation identical for every non-HTTP turn source.
    pub fn from_scope(
        turn_id: impl Into<Arc<str>>,
        principal: RequestPrincipal,
        cancellation: CancellationToken,
        deadline: Instant,
        scope: TurnContinuationScope,
    ) -> Result<Self, crate::session_storage::InvalidSessionId> {
        let session_id = SessionId::parse(&scope.session_id)?;
        let route = ProviderRoute::new(scope.provider.clone(), scope.model.clone());
        let surface = SurfaceCapabilities {
            ui_artifacts: scope.supports_ui_artifacts,
            liquid_markdown: scope.supports_liquid_markdown,
            browser_host: scope.supports_browser_host,
        };
        let correlation_id: Arc<str> = Arc::from(scope.turn_correlation_id.as_str());
        Ok(Self::new(
            turn_id,
            correlation_id,
            session_id,
            principal,
            route,
            surface,
            cancellation,
            deadline,
            scope,
        ))
    }

    pub fn handle(&self) -> TurnHandle {
        self.handle
    }

    pub fn turn_id(&self) -> &str {
        &self.turn_id
    }

    pub fn correlation_id(&self) -> &str {
        &self.correlation_id
    }

    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    pub fn principal(&self) -> &RequestPrincipal {
        &self.principal
    }

    pub fn route(&self) -> &ProviderRoute {
        &self.route
    }

    pub fn surface(&self) -> SurfaceCapabilities {
        self.surface
    }

    pub fn cancellation(&self) -> &CancellationToken {
        &self.cancellation
    }

    pub fn deadline(&self) -> Instant {
        self.deadline
    }

    pub fn legacy_scope(&self) -> &TurnContinuationScope {
        &self.legacy_scope
    }

    pub fn with_bot_identity(mut self, bot: BotTurnIdentity) -> Self {
        self.bot = Some(bot);
        self
    }

    pub fn bot_identity(&self) -> Option<&BotTurnIdentity> {
        self.bot.as_ref()
    }

    /// Memory tools use Bot continuity when present while transcript, artifact,
    /// and conversation operations retain the actual session identity.
    pub fn memory_session_id(&self) -> &str {
        self.bot
            .as_ref()
            .map(BotTurnIdentity::memory_scope_id)
            .unwrap_or_else(|| self.session_id.as_str())
    }

    pub fn with_work_environment(
        mut self,
        binding: medousa_runtime::WorkEnvironmentBinding,
    ) -> Self {
        self.work_environment = Some(binding);
        self
    }

    pub fn work_environment(&self) -> Option<&medousa_runtime::WorkEnvironmentBinding> {
        self.work_environment.as_ref()
    }

    pub fn next_work_environment_operation_id(&self, tool_name: &str) -> String {
        let sequence = self
            .work_environment_operation_sequence
            .fetch_add(1, Ordering::Relaxed)
            .saturating_add(1);
        format!("{}:{tool_name}:{sequence}", self.handle)
    }

    pub fn remember_grapheme_source(&self, source: &str) {
        *self
            .last_grapheme_source
            .lock()
            .expect("turn grapheme source poisoned") = Some(Arc::from(source));
    }

    pub fn last_grapheme_source(&self) -> Option<Arc<str>> {
        self.last_grapheme_source
            .lock()
            .expect("turn grapheme source poisoned")
            .clone()
    }

    pub fn tasks(&self) -> &Arc<TurnTaskGroup> {
        &self.tasks
    }

    /// Spawn a context-retaining child owned by this execution.
    pub fn spawn<F>(self: &Arc<Self>, future: F) -> Result<(), TurnTaskSpawnError>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.tasks.spawn(self.clone(), future)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TurnTaskSpawnError {
    pub limit: usize,
}

impl fmt::Display for TurnTaskSpawnError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "turn task capacity reached (limit {})",
            self.limit
        )
    }
}

impl std::error::Error for TurnTaskSpawnError {}

/// Bounded owner for child tasks that retain turn context.
#[derive(Debug)]
pub struct TurnTaskGroup {
    cancellation: CancellationToken,
    permits: Arc<tokio::sync::Semaphore>,
    limit: usize,
    tasks: Mutex<Vec<tokio::task::AbortHandle>>,
}

impl TurnTaskGroup {
    fn new(cancellation: CancellationToken, limit: usize) -> Self {
        Self {
            cancellation,
            permits: Arc::new(tokio::sync::Semaphore::new(limit)),
            limit,
            tasks: Mutex::new(Vec::with_capacity(limit)),
        }
    }

    fn spawn<F>(
        &self,
        context: Arc<TurnExecutionContext>,
        future: F,
    ) -> Result<(), TurnTaskSpawnError>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let permit = self
            .permits
            .clone()
            .try_acquire_owned()
            .map_err(|_| TurnTaskSpawnError { limit: self.limit })?;
        let cancellation = self.cancellation.clone();
        let task = tokio::spawn(with_turn_execution_context(context, async move {
            let _permit = permit;
            tokio::select! {
                () = cancellation.cancelled() => {}
                () = future => {}
            }
        }));
        let mut tasks = self.tasks.lock().expect("turn task group poisoned");
        tasks.retain(|task| !task.is_finished());
        tasks.push(task.abort_handle());
        Ok(())
    }

    pub fn active_count(&self) -> usize {
        self.limit - self.permits.available_permits()
    }

    pub fn shutdown(&self) {
        self.cancellation.cancel();
        for task in self
            .tasks
            .lock()
            .expect("turn task group poisoned")
            .drain(..)
        {
            task.abort();
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum TurnAdmissionError {
    AtCapacity { limit: usize },
    DuplicateHandle(TurnHandle),
}

impl fmt::Display for TurnAdmissionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AtCapacity { limit } => {
                write!(formatter, "live turn capacity reached (limit {limit})")
            }
            Self::DuplicateHandle(handle) => {
                write!(formatter, "turn handle '{handle}' is already live")
            }
        }
    }
}

impl std::error::Error for TurnAdmissionError {}

struct RegistryState {
    live: HashMap<TurnHandle, Arc<TurnExecutionContext>>,
    high_water: usize,
}

struct RegistryInner {
    max_live: usize,
    state: Mutex<RegistryState>,
}

/// Bounded registry for live execution owners. Locks never cross an await.
#[derive(Clone)]
pub struct TurnExecutionRegistry {
    inner: Arc<RegistryInner>,
}

impl TurnExecutionRegistry {
    pub fn new(max_live: usize) -> Self {
        assert!(max_live > 0, "live turn capacity must be non-zero");
        Self {
            inner: Arc::new(RegistryInner {
                max_live,
                state: Mutex::new(RegistryState {
                    live: HashMap::with_capacity(max_live),
                    high_water: 0,
                }),
            }),
        }
    }

    pub fn admit(
        &self,
        context: TurnExecutionContext,
    ) -> Result<TurnExecutionLease, TurnAdmissionError> {
        let handle = context.handle();
        let context = Arc::new(context);
        let mut state = self.inner.state.lock().expect("turn registry poisoned");
        if state.live.contains_key(&handle) {
            return Err(TurnAdmissionError::DuplicateHandle(handle));
        }
        if state.live.len() >= self.inner.max_live {
            return Err(TurnAdmissionError::AtCapacity {
                limit: self.inner.max_live,
            });
        }
        state.live.insert(handle, context.clone());
        state.high_water = state.high_water.max(state.live.len());
        drop(state);
        Ok(TurnExecutionLease {
            registry: self.clone(),
            context,
        })
    }

    pub fn get(&self, handle: TurnHandle) -> Option<Arc<TurnExecutionContext>> {
        self.inner
            .state
            .lock()
            .expect("turn registry poisoned")
            .live
            .get(&handle)
            .cloned()
    }

    pub fn live_count(&self) -> usize {
        self.inner
            .state
            .lock()
            .expect("turn registry poisoned")
            .live
            .len()
    }

    pub fn high_water(&self) -> usize {
        self.inner
            .state
            .lock()
            .expect("turn registry poisoned")
            .high_water
    }

    /// Transitional bridge for the session cancellation endpoint, which has
    /// not yet moved from public turn IDs to opaque execution handles.
    pub fn cancel_matching_turn(&self, session_id: &SessionId, turn_id: &str) -> bool {
        let context = self
            .inner
            .state
            .lock()
            .expect("turn registry poisoned")
            .live
            .values()
            .find(|context| context.session_id() == session_id && context.turn_id() == turn_id)
            .cloned();
        if let Some(context) = context {
            context.cancellation().cancel();
            true
        } else {
            false
        }
    }

    fn remove_exact(&self, handle: TurnHandle, expected: &Arc<TurnExecutionContext>) {
        let mut state = self.inner.state.lock().expect("turn registry poisoned");
        if state
            .live
            .get(&handle)
            .is_some_and(|current| Arc::ptr_eq(current, expected))
        {
            state.live.remove(&handle);
        }
    }
}

impl Default for TurnExecutionRegistry {
    fn default() -> Self {
        Self::new(256)
    }
}

/// Owns one exact registry entry and removes only that entry on drop.
pub struct TurnExecutionLease {
    registry: TurnExecutionRegistry,
    context: Arc<TurnExecutionContext>,
}

impl TurnExecutionLease {
    pub fn context(&self) -> &Arc<TurnExecutionContext> {
        &self.context
    }
}

impl Drop for TurnExecutionLease {
    fn drop(&mut self) {
        self.context.tasks().shutdown();
        self.registry
            .remove_exact(self.context.handle(), &self.context);
    }
}

/// Zero-sized compatibility token for tools whose upstream trait cannot yet
/// accept a typed invocation context. It carries no mutable fallback state.
#[derive(Clone, Debug, Default)]
pub struct TurnScopeAccess {
    #[cfg(test)]
    test_scope: Option<Arc<TurnContinuationScope>>,
}

#[cfg(test)]
impl TurnScopeAccess {
    pub(crate) fn for_test(scope: TurnContinuationScope) -> Self {
        Self {
            test_scope: Some(Arc::new(scope)),
        }
    }
}

pub async fn with_turn_execution_context<F>(
    context: Arc<TurnExecutionContext>,
    future: F,
) -> F::Output
where
    F: Future,
{
    let boundary = Arc::new(
        medousa_runtime::TurnExecutionBoundary::new(
            context.cancellation().clone(),
            context.deadline(),
        )
        .with_host_context(context),
    );
    medousa_runtime::with_turn_execution_boundary(boundary, future).await
}

pub fn active_turn_execution_context() -> Option<Arc<TurnExecutionContext>> {
    medousa_runtime::active_turn_execution_boundary()?.host_context::<TurnExecutionContext>()
}

pub fn missing_turn_context_invocations() -> u64 {
    medousa_runtime::missing_turn_execution_boundary_invocations()
}

pub use medousa_runtime::{TurnExecutionBoundaryError, await_turn_boundary};

/// Compatibility read for tools whose upstream trait cannot yet accept context.
/// The marker carries no fallback: an unscoped invocation fails closed.
#[allow(unused_variables)]
pub async fn turn_continuation_scope(access: &TurnScopeAccess) -> Option<TurnContinuationScope> {
    if let Some(context) = active_turn_execution_context() {
        return Some(context.legacy_scope().clone());
    }
    #[cfg(test)]
    if let Some(scope) = access.test_scope.as_ref() {
        return Some(scope.as_ref().clone());
    }
    None
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    use super::*;
    use crate::request_principal::TransportClass;
    use chrono::Utc;

    fn context(session: &str, provider: &str) -> TurnExecutionContext {
        let turn_id = format!("turn-{session}-{provider}");
        let scope = TurnContinuationScope {
            turn_correlation_id: turn_id.clone(),
            session_id: session.to_string(),
            identity_user_id: None,
            original_prompt: "canary".to_string(),
            delivery_target: None,
            provider: provider.to_string(),
            model: "model".to_string(),
            response_depth_mode: "standard".to_string(),
            supports_ui_artifacts: false,
            supports_liquid_markdown: false,
            supports_browser_host: false,
            channel_surface: None,
        };
        TurnExecutionContext::new(
            turn_id.clone(),
            turn_id,
            SessionId::parse(session).unwrap(),
            RequestPrincipal::local_app(Arc::from("test-credential"), TransportClass::Loopback),
            ProviderRoute::new(provider, "model"),
            SurfaceCapabilities::default(),
            CancellationToken::new(),
            Instant::now() + Duration::from_secs(60),
            scope,
        )
    }

    fn bot_profile() -> medousa_types::BotProfile {
        medousa_types::BotProfile {
            schema_version: medousa_types::BOT_PROFILE_SCHEMA_VERSION,
            bot_id: medousa_types::BotId::parse("bot_0123456789abcdef0123456789abcdef").unwrap(),
            owner_profile_id: "user:test".to_string(),
            display_name: "Ada".to_string(),
            role_description: Some("Teach systems through connected concepts".to_string()),
            avatar_ref: None,
            primary_manuscript_id: "specialist-mentor".to_string(),
            additional_manuscript_ids: vec![],
            memory_scope_id: "bot_0123456789abcdef0123456789abcdef".to_string(),
            default_mode: Some(medousa_types::AgentModeId::Teacher),
            primary_session_id: Some("session-a".to_string()),
            archived: false,
            revision: 7,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn scoped_admission_derives_route_and_surface_without_shared_state() {
        let mut scope = context("session-a", "provider-a").legacy_scope().clone();
        scope.supports_ui_artifacts = true;
        scope.supports_browser_host = true;
        let admitted = TurnExecutionContext::from_scope(
            "turn-a",
            RequestPrincipal::anonymous(TransportClass::Loopback),
            CancellationToken::new(),
            Instant::now() + Duration::from_secs(30),
            scope,
        )
        .unwrap();

        assert_eq!(admitted.session_id().as_str(), "session-a");
        assert_eq!(admitted.route().provider(), "provider-a");
        assert!(admitted.surface().ui_artifacts);
        assert!(admitted.surface().browser_host);
    }

    #[test]
    fn scoped_admission_rejects_invalid_session_authority() {
        let mut scope = context("session-a", "provider-a").legacy_scope().clone();
        scope.session_id = "../outside".to_string();

        assert!(
            TurnExecutionContext::from_scope(
                "turn-a",
                RequestPrincipal::anonymous(TransportClass::Loopback),
                CancellationToken::new(),
                Instant::now() + Duration::from_secs(30),
                scope,
            )
            .is_err()
        );
    }

    #[test]
    fn admission_carries_optional_work_environment_without_changing_turn_identity() {
        let original = context("session-a", "provider-a");
        assert!(original.work_environment().is_none());
        let expected_turn = original.turn_id().to_string();
        let expected_session = original.session_id().clone();
        let environment_id = medousa_runtime::WorkEnvironmentId::parse("env-1").unwrap();
        let binding = medousa_runtime::WorkEnvironmentBinding {
            port: Arc::new(medousa_runtime::InMemoryWorkEnvironmentPort::new()),
            handle: medousa_runtime::WorkEnvironmentHandle::new_local(
                environment_id.clone(),
                "test-adapter-token",
            ),
            fence: medousa_runtime::WorkEnvironmentFence {
                stasis_attempt: stasis::domain::runtime::resource_lease::FencingToken(1),
                forge_environment_generation: None,
                forge_execution_generation: None,
            },
        };

        let admitted = original.with_work_environment(binding);

        assert_eq!(admitted.turn_id(), expected_turn);
        assert_eq!(admitted.session_id(), &expected_session);
        assert_eq!(
            admitted.work_environment().unwrap().handle.environment_id(),
            &environment_id
        );
    }

    #[test]
    fn bot_identity_is_an_immutable_memory_and_prompt_snapshot() {
        let mut profile = bot_profile();
        let identity = BotTurnIdentity::from_profile(&profile);
        profile.display_name = "Changed after admission".to_string();
        profile.revision = 8;

        let admitted = context("session-a", "provider-a").with_bot_identity(identity);
        let bot = admitted.bot_identity().expect("Bot identity");
        assert_eq!(admitted.memory_session_id(), bot_profile().memory_scope_id);
        assert_eq!(
            bot.default_mode(),
            Some(medousa_types::AgentModeId::Teacher)
        );
        assert_eq!(bot.profile_revision(), 7);
        let appendix = bot.prompt_appendix();
        assert!(appendix.contains("display_name=Ada"));
        assert!(!appendix.contains("Changed after admission"));
        assert!(appendix.contains("authority=none"));
    }

    #[test]
    fn invocation_scratch_is_owned_by_one_execution_generation() {
        let first = context("session-a", "provider-a");
        let second = context("session-a", "provider-a");

        first.remember_grapheme_source("first-source");
        second.remember_grapheme_source("second-source");

        assert_eq!(
            first.last_grapheme_source().as_deref(),
            Some("first-source")
        );
        assert_eq!(
            second.last_grapheme_source().as_deref(),
            Some("second-source")
        );
    }

    #[test]
    fn bounded_registry_rejects_before_insertion_and_releases_on_drop() {
        let registry = TurnExecutionRegistry::new(1);
        let first = registry.admit(context("session-a", "provider-a")).unwrap();
        let error = registry
            .admit(context("session-b", "provider-b"))
            .err()
            .expect("second admission must fail");
        assert_eq!(error, TurnAdmissionError::AtCapacity { limit: 1 });
        assert_eq!(registry.live_count(), 1);
        assert_eq!(registry.high_water(), 1);

        drop(first);
        assert_eq!(registry.live_count(), 0);
        assert!(registry.admit(context("session-b", "provider-b")).is_ok());
    }

    #[test]
    fn same_session_turns_have_distinct_immutable_contexts() {
        let registry = TurnExecutionRegistry::new(2);
        let first = registry
            .admit(context("shared-session", "provider-a"))
            .unwrap();
        let second = registry
            .admit(context("shared-session", "provider-b"))
            .unwrap();

        assert_ne!(first.context().handle(), second.context().handle());
        assert_eq!(first.context().session_id(), second.context().session_id());
        assert_eq!(first.context().route().provider(), "provider-a");
        assert_eq!(second.context().route().provider(), "provider-b");
        assert_eq!(registry.live_count(), 2);
    }

    #[tokio::test]
    async fn task_local_context_isolated_across_concurrent_turns() {
        let first = Arc::new(context("session-a", "provider-a"));
        let second = Arc::new(context("session-b", "provider-b"));
        let barrier = Arc::new(tokio::sync::Barrier::new(2));

        let run = |context: Arc<TurnExecutionContext>, barrier: Arc<tokio::sync::Barrier>| async move {
            let expected = context.handle();
            with_turn_execution_context(context, async move {
                barrier.wait().await;
                tokio::task::yield_now().await;
                active_turn_execution_context().unwrap().handle()
            })
            .await
                == expected
        };
        let (first_ok, second_ok) =
            tokio::join!(run(first, barrier.clone()), run(second, barrier.clone()));

        assert!(first_ok && second_ok);
        assert!(active_turn_execution_context().is_none());
    }

    #[tokio::test]
    async fn owned_child_inherits_context_and_releases_its_permit() {
        let context = Arc::new(context("session-a", "provider-a"));
        let expected = context.handle();
        let (tx, rx) = tokio::sync::oneshot::channel();
        context
            .spawn(async move {
                tx.send(active_turn_execution_context().unwrap().handle())
                    .unwrap();
            })
            .unwrap();

        assert_eq!(rx.await.unwrap(), expected);
        tokio::task::yield_now().await;
        assert_eq!(context.tasks().active_count(), 0);
    }

    #[tokio::test]
    async fn invocation_scopes_are_isolated_without_shared_writes() {
        let access = TurnScopeAccess::default();
        let barrier = Arc::new(tokio::sync::Barrier::new(2));
        let run = |session: &'static str,
                   access: TurnScopeAccess,
                   barrier: Arc<tokio::sync::Barrier>| async move {
            let context = Arc::new(context(session, "provider"));
            with_turn_execution_context(context, async move {
                barrier.wait().await;
                tokio::task::yield_now().await;
                turn_continuation_scope(&access).await.unwrap().session_id
            })
            .await
        };

        let (first, second) = tokio::join!(
            run("session-a", access.clone(), barrier.clone()),
            run("session-b", access, barrier.clone())
        );
        assert_eq!(first, "session-a");
        assert_eq!(second, "session-b");
        assert!(active_turn_execution_context().is_none());
    }

    #[test]
    fn cancellation_is_owned_by_one_execution() {
        let first = context("session-a", "provider-a");
        let second = context("session-b", "provider-b");
        first.cancellation().cancel();
        assert!(first.cancellation().is_cancelled());
        assert!(!second.cancellation().is_cancelled());
    }

    #[test]
    fn stale_lease_cannot_remove_a_replacement_generation() {
        let registry = TurnExecutionRegistry::new(2);
        let stale = registry.admit(context("session-a", "provider-a")).unwrap();
        let handle = stale.context().handle();
        let mut replacement = context("session-a", "provider-b");
        replacement.handle = handle;
        let replacement = Arc::new(replacement);
        registry
            .inner
            .state
            .lock()
            .unwrap()
            .live
            .insert(handle, replacement.clone());

        drop(stale);

        let current = registry.get(handle).expect("replacement must remain live");
        assert!(Arc::ptr_eq(&current, &replacement));
        assert_eq!(current.route().provider(), "provider-b");
    }

    #[test]
    fn cancellation_matches_session_and_turn_together() {
        let registry = TurnExecutionRegistry::new(2);
        let first = registry.admit(context("session-a", "provider-a")).unwrap();
        let second = registry.admit(context("session-b", "provider-b")).unwrap();

        assert!(
            !registry
                .cancel_matching_turn(first.context().session_id(), second.context().turn_id())
        );
        assert!(!first.context().cancellation().is_cancelled());
        assert!(!second.context().cancellation().is_cancelled());
        assert!(
            registry.cancel_matching_turn(first.context().session_id(), first.context().turn_id())
        );
        assert!(first.context().cancellation().is_cancelled());
        assert!(!second.context().cancellation().is_cancelled());
    }

    #[test]
    fn cancellation_tree_flows_from_parent_only() {
        let parent = CancellationToken::new();
        let first_child = parent.child_token();
        let second_child = parent.child_token();

        first_child.cancel();
        assert!(first_child.is_cancelled());
        assert!(!parent.is_cancelled());
        assert!(!second_child.is_cancelled());

        parent.cancel();
        assert!(second_child.is_cancelled());
    }

    #[tokio::test]
    async fn leaf_boundary_stops_on_exact_execution_cancellation() {
        let context = Arc::new(context("session-a", "provider-a"));
        let cancellation = context.cancellation().clone();
        let waiting = with_turn_execution_context(context, async {
            await_turn_boundary(std::future::pending::<()>()).await
        });
        tokio::pin!(waiting);
        tokio::task::yield_now().await;
        cancellation.cancel();

        assert_eq!(waiting.await, Err(TurnExecutionBoundaryError::Cancelled));
    }

    #[tokio::test]
    async fn leaf_boundary_enforces_absolute_deadline() {
        let mut expired = context("session-a", "provider-a");
        expired.deadline = Instant::now();
        let cancellation = expired.cancellation().clone();
        let result = with_turn_execution_context(Arc::new(expired), async {
            await_turn_boundary(std::future::pending::<()>()).await
        })
        .await;

        assert_eq!(result, Err(TurnExecutionBoundaryError::DeadlineExceeded));
        assert!(cancellation.is_cancelled());
    }

    #[tokio::test]
    async fn spawned_leaf_reinstalls_context_before_waiting() {
        let context = Arc::new(context("session-a", "provider-a"));
        let expected = context.handle();
        let cancellation = context.cancellation().clone();
        let child_context = context.clone();
        let child = tokio::spawn(with_turn_execution_context(child_context, async move {
            let observed = active_turn_execution_context().unwrap().handle();
            let result = await_turn_boundary(std::future::pending::<()>()).await;
            (observed, result)
        }));
        tokio::task::yield_now().await;
        cancellation.cancel();
        let (observed, result) = child.await.unwrap();

        assert_eq!(observed, expected);
        assert_eq!(result, Err(TurnExecutionBoundaryError::Cancelled));
    }

    #[tokio::test]
    async fn unscoped_leaf_fails_closed_without_polling() {
        let before = missing_turn_context_invocations();
        let polled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let observed = polled.clone();
        let result = await_turn_boundary(async move {
            observed.store(true, Ordering::Relaxed);
        })
        .await;

        assert_eq!(result, Err(TurnExecutionBoundaryError::MissingContext));
        assert!(!polled.load(Ordering::Relaxed));
        assert!(missing_turn_context_invocations() > before);
    }
}
