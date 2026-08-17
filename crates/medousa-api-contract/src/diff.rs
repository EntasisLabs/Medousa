use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::registry::ContractRegistry;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompatibilityClass {
    Identical,
    Additive,
    Breaking,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractDiff {
    pub class: CompatibilityClass,
    pub added_operations: Vec<String>,
    pub removed_operations: Vec<String>,
    pub breaking_changes: Vec<String>,
}

pub fn diff_contracts(baseline: &ContractRegistry, candidate: &ContractRegistry) -> ContractDiff {
    let baseline_ids: BTreeSet<_> = baseline.operations().map(|op| op.operation_id.clone()).collect();
    let candidate_ids: BTreeSet<_> = candidate
        .operations()
        .map(|op| op.operation_id.clone())
        .collect();
    let added_operations: Vec<String> = candidate_ids.difference(&baseline_ids).cloned().collect();
    let removed_operations: Vec<String> = baseline_ids.difference(&candidate_ids).cloned().collect();

    let mut breaking_changes = Vec::new();
    for id in baseline_ids.intersection(&candidate_ids) {
        let left = baseline.get(id).expect("id");
        let right = candidate.get(id).expect("id");
        if left.method != right.method {
            breaking_changes.push(format!("{id}: method changed"));
        }
        if left.path != right.path {
            breaking_changes.push(format!("{id}: path changed"));
        }
        if left.trust_group != right.trust_group
            || left.capabilities != right.capabilities
            || left.bootstrap_public != right.bootstrap_public
        {
            breaking_changes.push(format!("{id}: auth/capability changed"));
        }
        let left_required: BTreeSet<_> = left
            .parameters
            .iter()
            .filter(|parameter| parameter.required)
            .map(|parameter| (parameter.location, parameter.name.clone()))
            .collect();
        let right_required: BTreeSet<_> = right
            .parameters
            .iter()
            .filter(|parameter| parameter.required)
            .map(|parameter| (parameter.location, parameter.name.clone()))
            .collect();
        if !right_required.is_subset(&left_required) && left_required != right_required {
            breaking_changes.push(format!("{id}: required parameter added"));
        }
    }
    breaking_changes.extend(removed_operations.iter().map(|id| format!("{id}: removed")));

    let class = if !breaking_changes.is_empty() {
        CompatibilityClass::Breaking
    } else if added_operations.is_empty() {
        CompatibilityClass::Identical
    } else {
        CompatibilityClass::Additive
    };

    ContractDiff {
        class,
        added_operations,
        removed_operations,
        breaking_changes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::ContractRegistry;
    use crate::spec::{
        Audience, FeatureProfile, HttpMethod, OperationSpec, ResponseSpec, SchemaRef, Stability,
    };

    fn op(id: &str, path: &str) -> OperationSpec {
        OperationSpec {
            operation_id: id.into(),
            stability: Stability::Stable,
            feature_profile: FeatureProfile::Core,
            audience: Audience::PublicSdk,
            method: HttpMethod::Get,
            path: path.into(),
            parameters: Vec::new(),
            request_body: None,
            responses: vec![ResponseSpec {
                status: 200,
                media_type: "application/json".into(),
                schema: SchemaRef::named("Json"),
            }],
            error_codes: vec!["internal_failure".into()],
            trust_group: "portal".into(),
            credential_scheme: "bearer".into(),
            capabilities: vec!["workshop.read".into()],
            browser_policy: "native_only".into(),
            body_limit: 1024,
            rate_limit_class: "read".into(),
            bootstrap_public: false,
            stream: None,
            deprecation: None,
        }
    }

    #[test]
    fn mutation_sentinel_detects_verb_change() {
        let mut baseline = ContractRegistry::new();
        baseline.register(op("health.get", "/v1/health")).unwrap();
        let mut mutated = ContractRegistry::new();
        let mut changed = op("health.get", "/v1/health");
        changed.method = HttpMethod::Post;
        mutated.register(changed).unwrap();
        let diff = diff_contracts(&baseline, &mutated);
        assert_eq!(diff.class, CompatibilityClass::Breaking);
        assert!(diff.breaking_changes.iter().any(|row| row.contains("method")));
    }
}
