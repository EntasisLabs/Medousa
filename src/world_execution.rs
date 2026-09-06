//! Exact, task-local binding for one governed world-tool invocation.
//!
//! The model selects only an opaque world id. A destination registry resolves
//! that id against its local authority and scopes the concrete driver and
//! principal to the duration of one tool call. No transport or host address is
//! available through this context.

use std::future::Future;

use medousa_world::{
    WorldDriverId, WorldId, WorldPrincipal, WorldPrincipalId, WorldPrincipalKind,
    WorldSurfaceKind,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldExecutionBinding {
    world_id: WorldId,
    authority_id: String,
    driver_id: WorldDriverId,
    surface: WorldSurfaceKind,
    principal: WorldPrincipal,
    expires_at_ms: Option<u64>,
}

impl WorldExecutionBinding {
    #[cfg_attr(not(feature = "full-daemon"), allow(dead_code))]
    pub(crate) async fn resolve_local(
        world_id: &str,
        principal_id: &str,
        expires_at_ms: Option<u64>,
    ) -> Result<Self, String> {
        let local_authority = crate::workshop_authority::current()?.to_string();
        match crate::world_authority::resolve_world_session(world_id) {
            Ok(session) => {
                if session.authority_id.as_str() != local_authority {
                    return Err(
                        "selected world is not owned by this destination workshop".to_string(),
                    );
                }
                Ok(Self {
                    world_id: session.world_id,
                    authority_id: local_authority,
                    driver_id: session.driver_id,
                    surface: session.surface,
                    principal: worker_principal(principal_id),
                    expires_at_ms,
                })
            }
            Err(error) => {
                #[cfg(feature = "full-daemon")]
                if let Some(broker) = crate::daemon::computer_driver_host::global_computer_broker()
                    && let Some(driver_id) = broker
                        .resolve_world_driver(world_id, &local_authority)
                        .await?
                {
                    return Ok(Self {
                        world_id: WorldId::new(world_id),
                        authority_id: local_authority,
                        driver_id,
                        surface: WorldSurfaceKind::Desktop,
                        principal: worker_principal(principal_id),
                        expires_at_ms,
                    });
                }
                Err(error)
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn for_test(world_id: &str, driver_id: &str, surface: WorldSurfaceKind) -> Self {
        Self {
            world_id: WorldId::new(world_id),
            authority_id: "workshop:test".to_string(),
            driver_id: WorldDriverId::new(driver_id),
            surface,
            principal: WorldPrincipal::new("worker:test", WorldPrincipalKind::Worker),
            expires_at_ms: None,
        }
    }

    pub fn world_id(&self) -> &WorldId {
        &self.world_id
    }

    pub fn authority_id(&self) -> &str {
        &self.authority_id
    }

    pub fn driver_id(&self) -> &WorldDriverId {
        &self.driver_id
    }

    pub fn surface(&self) -> WorldSurfaceKind {
        self.surface
    }

    pub fn principal(&self) -> &WorldPrincipal {
        &self.principal
    }

    pub fn expires_at_ms(&self) -> Option<u64> {
        self.expires_at_ms
    }
}

#[cfg_attr(not(feature = "full-daemon"), allow(dead_code))]
fn worker_principal(principal_id: &str) -> WorldPrincipal {
    WorldPrincipal::new(
        WorldPrincipalId::new(principal_id),
        WorldPrincipalKind::Worker,
    )
}

tokio::task_local! {
    static ACTIVE_WORLD_EXECUTION: WorldExecutionBinding;
}

pub async fn with_world_execution<F>(binding: WorldExecutionBinding, future: F) -> F::Output
where
    F: Future,
{
    ACTIVE_WORLD_EXECUTION.scope(binding, future).await
}

pub fn active_world_execution() -> Option<WorldExecutionBinding> {
    ACTIVE_WORLD_EXECUTION.try_with(Clone::clone).ok()
}

pub fn active_world_for(surface: WorldSurfaceKind) -> Option<WorldExecutionBinding> {
    active_world_execution().filter(|binding| binding.surface == surface)
}

pub fn require_active_world(
    world_id: &str,
    surface: WorldSurfaceKind,
) -> Result<WorldExecutionBinding, String> {
    let binding = active_world_for(surface)
        .ok_or_else(|| "governed world execution context is unavailable".to_string())?;
    if binding.world_id.as_str() != world_id {
        return Err("governed world execution context does not match the requested world".to_string());
    }
    Ok(binding)
}

/// Reconcile destination-owned runtime state immediately before a governed
/// action. Only the isolated-browser adapter currently has restart state to
/// rebuild; all other worlds are already live registrations.
pub async fn prepare_active_world_for_action() -> Result<(), String> {
    #[cfg(feature = "full-daemon")]
    if let Some(binding) = active_world_for(WorldSurfaceKind::Browser)
        && crate::browser_tools::is_isolated_browser_driver_id(binding.driver_id().as_str())
    {
        let host = crate::daemon::isolated_browser_host::global_host()
            .ok_or_else(|| "isolated browser host is unavailable".to_string())?;
        host.prepare_authorized_persistent_world(binding.world_id().as_str())
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn binding_is_visible_only_inside_its_tool_boundary() {
        let binding = WorldExecutionBinding::for_test(
            "world:browser:test",
            "driver:test",
            WorldSurfaceKind::Browser,
        );
        assert!(active_world_execution().is_none());
        with_world_execution(binding.clone(), async {
            assert_eq!(active_world_execution(), Some(binding));
        })
        .await;
        assert!(active_world_execution().is_none());
    }
}
