//! Reviewed policy metadata for daemon route assembly and inventory export.

use axum::body::Body;
use axum::extract::{Extension, State};
use axum::http::{Method, Request};
use axum::middleware::Next;
use axum::response::Response;
use axum::routing::MethodRouter;
use axum::{Router, extract::DefaultBodyLimit};
use serde::Serialize;

use crate::peer_scope::AccessDenial;
use crate::request_principal::{Capability, RequestPrincipal};

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorizationClass {
    Public,
    Capability,
    PreviewToken,
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
    pub authorization: AuthorizationClass,
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
            authorization: policy.authorization(),
            bootstrap_public: policy.bootstrap_public,
            browser_policy: policy.browser_policy,
            body_limit: policy.body_limit,
            rate_limit_class: policy.rate_limit_class,
        }
    }
}

impl RoutePolicy {
    const fn authorization(&self) -> AuthorizationClass {
        if self.bootstrap_public {
            AuthorizationClass::Public
        } else if matches!(self.group, RouteGroup::Preview) {
            AuthorizationClass::PreviewToken
        } else {
            AuthorizationClass::Capability
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
    pub fn route(self, policy: RoutePolicy, handler: MethodRouter<S>) -> Self {
        self.methods([(policy, handler)])
    }

    pub fn methods<const N: usize>(mut self, routes: [(RoutePolicy, MethodRouter<S>); N]) -> Self {
        let mut routes = routes.into_iter();
        let (first_policy, first_handler) = routes.next().expect("route set cannot be empty");
        let path = first_policy.path;
        let first_limit = first_policy.body_limit;
        let first_group = first_policy.group;
        let first_capability = first_policy.required_capability;
        let first_browser_policy = first_policy.browser_policy;
        self.inventory
            .declare(first_policy)
            .expect("invalid daemon route policy");
        let mut handler = declared_method(
            first_handler,
            first_limit,
            first_group,
            first_capability,
            first_browser_policy,
        );
        for (policy, method_handler) in routes {
            assert_eq!(policy.path, path, "route set must share one path");
            let body_limit = policy.body_limit;
            let group = policy.group;
            let required_capability = policy.required_capability;
            let browser_policy = policy.browser_policy;
            self.inventory
                .declare(policy)
                .expect("invalid daemon route policy");
            handler = handler.merge(declared_method(
                method_handler,
                body_limit,
                group,
                required_capability,
                browser_policy,
            ));
        }
        self.router = self.router.route(path, handler);
        self
    }

    pub fn inventory(&self) -> &RouteInventory {
        &self.inventory
    }

    pub fn merge(mut self, other: Self) -> Self {
        self.inventory
            .extend(&other.inventory)
            .expect("duplicate declared route policy");
        self.router = self.router.merge(other.router);
        self
    }

    pub fn with_state<S2>(self, state: S) -> DeclaredRouter<S2>
    where
        S2: Clone + Send + Sync + 'static,
    {
        DeclaredRouter {
            router: self.router.with_state(state),
            inventory: self.inventory,
        }
    }

    pub fn into_router(self) -> Router<S> {
        self.router
    }
}

fn declared_method<S>(
    handler: MethodRouter<S>,
    body_limit: usize,
    group: RouteGroup,
    required_capability: Option<Capability>,
    browser_policy: BrowserPolicy,
) -> MethodRouter<S>
where
    S: Clone + Send + Sync + 'static,
{
    let handler = handler.layer(DefaultBodyLimit::max(body_limit)).layer(
        axum::middleware::from_fn_with_state(
            browser_policy,
            crate::daemon::request_boundary::enforce_declared_browser_policy,
        ),
    );
    if matches!(group, RouteGroup::Preview) {
        return handler.layer(axum::middleware::from_fn(
            crate::daemon::forge_preview::enforce_preview_grant,
        ));
    }
    match required_capability {
        Some(required) => handler.layer(axum::middleware::from_fn_with_state(
            required,
            enforce_declared_capability,
        )),
        None => handler,
    }
}

async fn enforce_declared_capability(
    State(required): State<Capability>,
    Extension(principal): Extension<RequestPrincipal>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if principal.capabilities().contains(required) {
        next.run(request).await
    } else {
        AccessDenial::Forbidden.into_response()
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
        if !policy.bootstrap_public
            && !matches!(policy.group, RouteGroup::Preview)
            && policy.required_capability.is_none()
        {
            return Err("protected route requires a capability");
        }
        if matches!(policy.group, RouteGroup::Preview) && policy.required_capability.is_some() {
            return Err("preview route authorization is token-owned");
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

    pub fn extend(&mut self, other: &Self) -> Result<(), &'static str> {
        for policy in &other.entries {
            self.declare(policy.clone())?;
        }
        Ok(())
    }

    pub fn to_pretty_json(&self) -> Result<String, serde_json::Error> {
        let mut entries = self.entries().collect::<Vec<_>>();
        entries.sort_by(|left, right| {
            left.path
                .cmp(right.path)
                .then_with(|| left.method.cmp(&right.method))
        });
        serde_json::to_string_pretty(&entries)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use axum::http::header::ORIGIN;
    use axum::http::{HeaderValue, StatusCode};
    use axum::routing::get;
    use tower::ServiceExt;

    use crate::request_principal::TransportClass;

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

    fn preview() -> RoutePolicy {
        RoutePolicy {
            method: Method::GET,
            path: "/v1/forge/preview/{token}",
            group: RouteGroup::Preview,
            required_capability: None,
            bootstrap_public: false,
            browser_policy: BrowserPolicy::ExactOrigin,
            body_limit: 2 * 1024 * 1024,
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
    fn preview_routes_are_token_owned_not_capability_owned() {
        let mut inventory = RouteInventory::default();
        inventory.declare(preview()).unwrap();
        let entry = inventory.entries().next().unwrap();
        assert_eq!(entry.authorization, AuthorizationClass::PreviewToken);
        assert_eq!(entry.required_capability, None);

        let mut invalid = preview();
        invalid.required_capability = Some(Capability::AdminExecute);
        assert_eq!(
            inventory.declare(invalid),
            Err("preview route authorization is token-owned")
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
    fn inventory_merge_rejects_cross_surface_duplicates() {
        let mut left = RouteInventory::default();
        left.declare(protected()).unwrap();
        let mut right = RouteInventory::default();
        right.declare(protected()).unwrap();
        assert_eq!(left.extend(&right), Err("duplicate route method and path"));
    }

    #[test]
    fn inventory_exports_stable_wire_names() {
        let mut inventory = RouteInventory::default();
        inventory.declare(protected()).unwrap();
        let entry = inventory.entries().next().unwrap();
        assert_eq!(entry.method, "GET");
        assert_eq!(entry.required_capability, Some("workshop.read"));
        assert_eq!(entry.authorization, AuthorizationClass::Capability);
        assert_eq!(serde_json::to_value(entry).unwrap()["group"], "portal");
    }

    #[tokio::test]
    async fn declared_capability_denies_before_handler() {
        let router = DeclaredRouter::default()
            .route(protected(), get(|| async { StatusCode::NO_CONTENT }))
            .into_router()
            .layer(Extension(RequestPrincipal::anonymous(
                TransportClass::Direct,
            )));
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn declared_capability_allows_matching_principal() {
        let router = DeclaredRouter::default()
            .route(protected(), get(|| async { StatusCode::NO_CONTENT }))
            .into_router()
            .layer(Extension(RequestPrincipal::local_app(
                Arc::from("test-local"),
                TransportClass::Loopback,
            )));
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn native_only_route_rejects_browser_origin_before_handler() {
        let hits = Arc::new(AtomicUsize::new(0));
        let handler_hits = hits.clone();
        let router = DeclaredRouter::default()
            .route(
                protected(),
                get(move || {
                    let handler_hits = handler_hits.clone();
                    async move {
                        handler_hits.fetch_add(1, Ordering::Relaxed);
                        StatusCode::NO_CONTENT
                    }
                }),
            )
            .into_router()
            .layer(Extension(RequestPrincipal::local_app(
                Arc::from("test-local"),
                TransportClass::Loopback,
            )));
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/health")
                    .header(ORIGIN, HeaderValue::from_static("https://attacker.example"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(hits.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn native_only_route_allows_originless_native_client() {
        let router = DeclaredRouter::default()
            .route(protected(), get(|| async { StatusCode::NO_CONTENT }))
            .into_router()
            .layer(Extension(RequestPrincipal::local_app(
                Arc::from("test-local"),
                TransportClass::Loopback,
            )));
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }
}
