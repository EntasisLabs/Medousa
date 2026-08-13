//! Reviewed policy metadata for daemon route assembly and inventory export.

use axum::http::Method;
use axum::routing::MethodRouter;
use axum::{Router, extract::DefaultBodyLimit};
use serde::Serialize;

use crate::request_principal::Capability;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteGroup {
    Liveness,
    PairingCeremony,
    Portal,
    PeerExchange,
    Administration,
    Preview,
}

impl RouteGroup {
    const fn permits_bootstrap(self) -> bool {
        matches!(self, Self::Liveness | Self::PairingCeremony)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserPolicy {
    Public,
    NativeOnly,
    ExactOrigin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RateLimitClass {
    Liveness,
    PairingCeremony,
    Read,
    Mutation,
    PeerExchange,
    Administration,
    Stream,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoutePolicy {
    pub method: Method,
    pub path: &'static str,
    pub group: RouteGroup,
    pub required_capability: Option<Capability>,
    pub bootstrap_public: bool,
    pub browser_policy: BrowserPolicy,
    pub body_limit: usize,
    pub rate_limit_class: RateLimitClass,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RouteInventoryEntry {
    pub method: String,
    pub path: &'static str,
    pub group: RouteGroup,
    pub required_capability: Option<&'static str>,
    pub bootstrap_public: bool,
    pub browser_policy: BrowserPolicy,
    pub body_limit: usize,
    pub rate_limit_class: RateLimitClass,
}

impl From<&RoutePolicy> for RouteInventoryEntry {
    fn from(policy: &RoutePolicy) -> Self {
        Self {
            method: policy.method.as_str().to_string(),
            path: policy.path,
            group: policy.group,
            required_capability: policy.required_capability.map(Capability::as_str),
            bootstrap_public: policy.bootstrap_public,
            browser_policy: policy.browser_policy,
            body_limit: policy.body_limit,
            rate_limit_class: policy.rate_limit_class,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct RouteInventory {
    entries: Vec<RoutePolicy>,
}

pub struct DeclaredRouter<S = ()> {
    router: Router<S>,
    inventory: RouteInventory,
}

impl<S> Default for DeclaredRouter<S>
where
    S: Clone + Send + Sync + 'static,
{
    fn default() -> Self {
        Self {
            router: Router::new(),
            inventory: RouteInventory::default(),
        }
    }
}

impl<S> DeclaredRouter<S>
where
    S: Clone + Send + Sync + 'static,
{
    pub fn route(mut self, policy: RoutePolicy, handler: MethodRouter<S>) -> Self {
        let path = policy.path;
        let body_limit = policy.body_limit;
        self.inventory
            .declare(policy)
            .expect("invalid daemon route policy");
        self.router = self
            .router
            .route(path, handler.layer(DefaultBodyLimit::max(body_limit)));
        self
    }

    pub fn inventory(&self) -> &RouteInventory {
        &self.inventory
    }

    pub fn into_router(self) -> Router<S> {
        self.router
    }
}

impl RouteInventory {
    pub fn declare(&mut self, policy: RoutePolicy) -> Result<(), &'static str> {
        if !policy.path.starts_with('/') {
            return Err("route path must be absolute");
        }
        if policy.body_limit == 0 {
            return Err("route body limit must be non-zero");
        }
        if policy.bootstrap_public != policy.group.permits_bootstrap() {
            return Err("bootstrap visibility does not match route group");
        }
        if !policy.bootstrap_public && policy.required_capability.is_none() {
            return Err("protected route requires a capability");
        }
        if policy.bootstrap_public && policy.required_capability.is_some() {
            return Err("bootstrap route cannot require an application capability");
        }
        if self
            .entries
            .iter()
            .any(|entry| entry.method == policy.method && entry.path == policy.path)
        {
            return Err("duplicate route method and path");
        }
        self.entries.push(policy);
        Ok(())
    }

    pub fn entries(&self) -> impl ExactSizeIterator<Item = RouteInventoryEntry> + '_ {
        self.entries.iter().map(RouteInventoryEntry::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn protected() -> RoutePolicy {
        RoutePolicy {
            method: Method::GET,
            path: "/v1/health",
            group: RouteGroup::Portal,
            required_capability: Some(Capability::WorkshopRead),
            bootstrap_public: false,
            browser_policy: BrowserPolicy::NativeOnly,
            body_limit: 1024,
            rate_limit_class: RateLimitClass::Read,
        }
    }

    #[test]
    fn protected_routes_require_capabilities() {
        let mut inventory = RouteInventory::default();
        let mut policy = protected();
        policy.required_capability = None;
        assert_eq!(
            inventory.declare(policy),
            Err("protected route requires a capability")
        );
    }

    #[test]
    fn bootstrap_visibility_is_group_owned() {
        let mut inventory = RouteInventory::default();
        let mut policy = protected();
        policy.bootstrap_public = true;
        assert_eq!(
            inventory.declare(policy),
            Err("bootstrap visibility does not match route group")
        );
    }

    #[test]
    fn duplicate_method_and_path_is_rejected() {
        let mut inventory = RouteInventory::default();
        inventory.declare(protected()).unwrap();
        assert_eq!(
            inventory.declare(protected()),
            Err("duplicate route method and path")
        );
    }

    #[test]
    fn inventory_exports_stable_wire_names() {
        let mut inventory = RouteInventory::default();
        inventory.declare(protected()).unwrap();
        let entry = inventory.entries().next().unwrap();
        assert_eq!(entry.method, "GET");
        assert_eq!(entry.required_capability, Some("workshop.read"));
        assert_eq!(serde_json::to_value(entry).unwrap()["group"], "portal");
    }
}
