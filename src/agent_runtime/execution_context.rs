//! Immutable per-turn execution identity and exact live-owner lifecycle.

use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::request_principal::RequestPrincipal;
use crate::session_storage::SessionId;
use crate::turn_continuation::TurnContinuationScope;

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
#[derive(Clone, Debug)]
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
            tasks,
        }
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

tokio::task_local! {
    static ACTIVE_TURN_CONTEXT: Arc<TurnExecutionContext>;
    static ACTIVE_LEGACY_TURN_SCOPE: Arc<TurnContinuationScope>;
}

pub async fn with_turn_execution_context<F>(
    context: Arc<TurnExecutionContext>,
    future: F,
) -> F::Output
where
    F: Future,
{
    ACTIVE_TURN_CONTEXT.scope(context, future).await
}

pub fn active_turn_execution_context() -> Option<Arc<TurnExecutionContext>> {
    ACTIVE_TURN_CONTEXT.try_with(Arc::clone).ok()
}

pub async fn with_legacy_turn_scope<F>(scope: Arc<TurnContinuationScope>, future: F) -> F::Output
where
    F: Future,
{
    ACTIVE_LEGACY_TURN_SCOPE.scope(scope, future).await
}

/// Compatibility read for tools whose upstream trait cannot yet accept context.
/// Admitted daemon turns never consult the shared fallback.
pub async fn turn_continuation_scope(
    fallback: &tokio::sync::RwLock<Option<TurnContinuationScope>>,
) -> Option<TurnContinuationScope> {
    if let Some(context) = active_turn_execution_context() {
        return Some(context.legacy_scope().clone());
    }
    if let Ok(scope) = ACTIVE_LEGACY_TURN_SCOPE.try_with(Arc::clone) {
        return Some(scope.as_ref().clone());
    }
    fallback.read().await.clone()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::request_principal::TransportClass;

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
    async fn legacy_worker_scopes_are_isolated_without_shared_writes() {
        let fallback = Arc::new(tokio::sync::RwLock::new(None));
        let barrier = Arc::new(tokio::sync::Barrier::new(2));
        let run = |session: &'static str,
                   fallback: Arc<tokio::sync::RwLock<Option<TurnContinuationScope>>>,
                   barrier: Arc<tokio::sync::Barrier>| async move {
            let scope = context(session, "provider").legacy_scope().clone();
            with_legacy_turn_scope(Arc::new(scope), async move {
                barrier.wait().await;
                tokio::task::yield_now().await;
                turn_continuation_scope(&fallback).await.unwrap().session_id
            })
            .await
        };

        let (first, second) = tokio::join!(
            run("session-a", fallback.clone(), barrier.clone()),
            run("session-b", fallback.clone(), barrier.clone())
        );
        assert_eq!(first, "session-a");
        assert_eq!(second, "session-b");
        assert!(fallback.read().await.is_none());
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
}
