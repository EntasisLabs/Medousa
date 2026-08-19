#![allow(dead_code)]

use serde::{Deserialize, Serialize};

use crate::registry::ContractRegistry;
use crate::spec::HttpMethod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscrepancyKind {
    MissingFromManifest,
    ExtraInManifest,
    MissingFromParity,
    ExtraInParity,
    PathPlaceholderDrift,
    FakeSseVerb,
    QueryEmbeddedInPath,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscrepancyRow {
    pub kind: DiscrepancyKind,
    pub method: String,
    pub path: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscrepancyReport {
    pub declared_count: usize,
    pub manifest_count: usize,
    pub parity_count: usize,
    pub rows: Vec<DiscrepancyRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedRoute {
    pub method: String,
    pub path: String,
}

pub fn discrepancy_report(
    registry: &ContractRegistry,
    manifest: &[ListedRoute],
    parity: &[ListedRoute],
) -> DiscrepancyReport {
    let declared: Vec<ListedRoute> = registry
        .operations()
        .map(|spec| ListedRoute {
            method: spec.method.as_str().to_string(),
            path: spec.path.clone(),
        })
        .collect();
    let mut rows = Vec::new();

    for route in &declared {
        if !manifest.iter().any(|item| same_route(item, route)) {
            rows.push(DiscrepancyRow {
                kind: DiscrepancyKind::MissingFromManifest,
                method: route.method.clone(),
                path: route.path.clone(),
                detail: "declared production route is absent from manifest.yaml".into(),
            });
        }
        if !parity.iter().any(|item| same_route(item, route)) {
            rows.push(DiscrepancyRow {
                kind: DiscrepancyKind::MissingFromParity,
                method: route.method.clone(),
                path: route.path.clone(),
                detail: "declared production route is absent from PARITY_ROUTES".into(),
            });
        }
    }
    for route in manifest {
        if !declared.iter().any(|item| same_route(item, route)) {
            rows.push(DiscrepancyRow {
                kind: DiscrepancyKind::ExtraInManifest,
                method: route.method.clone(),
                path: route.path.clone(),
                detail: "manifest.yaml lists a route the declared router does not mount".into(),
            });
        }
        if route.path.contains('?') {
            rows.push(DiscrepancyRow {
                kind: DiscrepancyKind::QueryEmbeddedInPath,
                method: route.method.clone(),
                path: route.path.clone(),
                detail: "query text is embedded in a path template".into(),
            });
        }
    }
    for route in parity {
        if route.method == "SSE" {
            rows.push(DiscrepancyRow {
                kind: DiscrepancyKind::FakeSseVerb,
                method: route.method.clone(),
                path: route.path.clone(),
                detail: "PARITY_ROUTES uses SSE as a fake HTTP verb".into(),
            });
        }
        if route.path.contains('?') {
            rows.push(DiscrepancyRow {
                kind: DiscrepancyKind::QueryEmbeddedInPath,
                method: route.method.clone(),
                path: route.path.clone(),
                detail: "query text is embedded in a path template".into(),
            });
        }
        if !declared.iter().any(|item| same_route(item, route)) && route.method != "SSE" {
            rows.push(DiscrepancyRow {
                kind: DiscrepancyKind::ExtraInParity,
                method: route.method.clone(),
                path: route.path.clone(),
                detail: "PARITY_ROUTES lists a route the declared router does not mount".into(),
            });
        }
    }

    for declared_route in &declared {
        for listed in manifest.iter().chain(parity.iter()) {
            if normalize_path(&declared_route.path) == normalize_path(&listed.path)
                && declared_route.path != listed.path.split('?').next().unwrap_or(&listed.path)
            {
                rows.push(DiscrepancyRow {
                    kind: DiscrepancyKind::PathPlaceholderDrift,
                    method: declared_route.method.clone(),
                    path: format!("{} vs {}", declared_route.path, listed.path),
                    detail: "path templates disagree on placeholders or prefixes".into(),
                });
            }
        }
    }

    rows.sort_by(|left, right| {
        left.kind
            .to_string()
            .cmp(&right.kind.to_string())
            .then_with(|| left.path.cmp(&right.path))
    });
    rows.dedup();
    DiscrepancyReport {
        declared_count: declared.len(),
        manifest_count: manifest.len(),
        parity_count: parity.len(),
        rows,
    }
}

impl std::fmt::Display for DiscrepancyKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::MissingFromManifest => "missing_from_manifest",
            Self::ExtraInManifest => "extra_in_manifest",
            Self::MissingFromParity => "missing_from_parity",
            Self::ExtraInParity => "extra_in_parity",
            Self::PathPlaceholderDrift => "path_placeholder_drift",
            Self::FakeSseVerb => "fake_sse_verb",
            Self::QueryEmbeddedInPath => "query_embedded_in_path",
        })
    }
}

fn same_route(left: &ListedRoute, right: &ListedRoute) -> bool {
    left.method == right.method && strip_query(&left.path) == strip_query(&right.path)
}

fn strip_query(path: &str) -> &str {
    path.split('?').next().unwrap_or(path)
}

fn normalize_path(path: &str) -> String {
    strip_query(path)
        .replace("{id}", "{_}")
        .replace("{path}", "{_}")
        .replace("{uid}", "{_}")
        .replace("{job_id}", "{_}")
        .replace("{proposal_id}", "{_}")
        .replace("{component_id}", "{_}")
        .replace("{feed_id}", "{_}")
        .replace("{probe_id}", "{_}")
        .replace("{key}", "{_}")
        .replace("{name}", "{_}")
}

pub fn parse_parity_table(source: &str) -> Vec<ListedRoute> {
    let mut routes = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('(') {
            continue;
        }
        let parts: Vec<&str> = trimmed
            .trim_start_matches('(')
            .trim_end_matches(',')
            .trim_end_matches(')')
            .split("\", \"")
            .collect();
        if parts.len() < 4 {
            continue;
        }
        let method = parts[2].trim_matches('"');
        let path = parts[3].trim_matches('"').trim_end_matches("\")");
        if HttpMethod::parse(method).is_ok() || method == "SSE" {
            routes.push(ListedRoute {
                method: method.to_string(),
                path: path.to_string(),
            });
        }
    }
    routes
}

#[derive(Debug, Deserialize)]
struct ManifestFile {
    methods: Vec<ManifestMethod>,
}

#[derive(Debug, Deserialize)]
struct ManifestMethod {
    http: String,
    path: String,
}

pub fn parse_manifest_yaml(yaml: &str) -> Result<Vec<ListedRoute>, serde_yaml::Error> {
    let parsed: ManifestFile = serde_yaml::from_str(yaml)?;
    Ok(parsed
        .methods
        .into_iter()
        .map(|method| ListedRoute {
            method: method.http,
            path: method.path,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::ContractRegistry;
    use crate::spec::{
        Audience, FeatureProfile, HttpMethod, OperationSpec, ResponseSpec, SchemaRef, Stability,
    };

    fn op(method: HttpMethod, path: &str, id: &str, bootstrap: bool) -> OperationSpec {
        OperationSpec {
            operation_id: id.into(),
            stability: Stability::Stable,
            feature_profile: FeatureProfile::Core,
            audience: Audience::PublicSdk,
            method,
            path: path.into(),
            parameters: Vec::new(),
            request_body: None,
            responses: vec![ResponseSpec {
                status: 200,
                media_type: "application/json".into(),
                schema: SchemaRef::named("Json"),
            }],
            error_codes: vec!["internal_failure".into()],
            trust_group: if bootstrap { "liveness" } else { "portal" }.into(),
            credential_scheme: if bootstrap { "none" } else { "bearer" }.into(),
            capabilities: if bootstrap {
                Vec::new()
            } else {
                vec!["workshop.read".into()]
            },
            browser_policy: if bootstrap { "public" } else { "native_only" }.into(),
            body_limit: 1024,
            rate_limit_class: "read".into(),
            bootstrap_public: bootstrap,
            stream: None,
            deprecation: None,
        }
    }

    #[test]
    fn health_path_drift_and_fake_sse_appear() {
        let mut registry = ContractRegistry::new();
        registry
            .register(op(HttpMethod::Get, "/health", "liveness.get", true))
            .unwrap();
        let manifest = vec![ListedRoute {
            method: "GET".into(),
            path: "/v1/health".into(),
        }];
        let parity = vec![
            ListedRoute {
                method: "GET".into(),
                path: "/v1/health".into(),
            },
            ListedRoute {
                method: "SSE".into(),
                path: "/v1/interactive/turn/{id}/stream".into(),
            },
            ListedRoute {
                method: "GET".into(),
                path: "/v1/sessions?limit={limit}".into(),
            },
        ];
        let report = discrepancy_report(&registry, &manifest, &parity);
        assert!(
            report.rows.iter().any(
                |row| row.kind == DiscrepancyKind::MissingFromManifest && row.path == "/health"
            )
        );
        assert!(
            report
                .rows
                .iter()
                .any(|row| row.kind == DiscrepancyKind::FakeSseVerb)
        );
        assert!(
            report
                .rows
                .iter()
                .any(|row| row.kind == DiscrepancyKind::QueryEmbeddedInPath)
        );
    }
}
