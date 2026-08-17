use std::collections::BTreeMap;

use crate::spec::{ContractError, OperationSpec};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContractRegistry {
    operations: BTreeMap<String, OperationSpec>,
}

impl ContractRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, spec: OperationSpec) -> Result<(), ContractError> {
        spec.validate()?;
        if self.operations.contains_key(&spec.operation_id) {
            return Err(ContractError::invalid(format!(
                "duplicate operation id {}",
                spec.operation_id
            )));
        }
        if self.operations.values().any(|existing| {
            existing.method == spec.method && existing.path == spec.path
        }) {
            return Err(ContractError::invalid(format!(
                "duplicate method and path {} {}",
                spec.method.as_str(),
                spec.path
            )));
        }
        self.operations.insert(spec.operation_id.clone(), spec);
        Ok(())
    }

    pub fn get(&self, operation_id: &str) -> Option<&OperationSpec> {
        self.operations.get(operation_id)
    }

    pub fn operations(&self) -> impl Iterator<Item = &OperationSpec> {
        self.operations.values()
    }

    pub fn into_operations(self) -> Vec<OperationSpec> {
        self.operations.into_values().collect()
    }

    pub fn len(&self) -> usize {
        self.operations.len()
    }

    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }

    pub fn extend(&mut self, other: &ContractRegistry) -> Result<(), ContractError> {
        for spec in other.operations() {
            self.register(spec.clone())?;
        }
        Ok(())
    }
}

#[allow(dead_code)]
pub fn registry_from_operations(
    operations: impl IntoIterator<Item = OperationSpec>,
) -> Result<ContractRegistry, ContractError> {
    let mut registry = ContractRegistry::new();
    for spec in operations {
        registry.register(spec)?;
    }
    Ok(registry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{
        Audience, FeatureProfile, HttpMethod, ResponseSpec, SchemaRef, Stability,
    };

    fn liveness() -> OperationSpec {
        OperationSpec {
            operation_id: "liveness.get".into(),
            stability: Stability::Stable,
            feature_profile: FeatureProfile::Core,
            audience: Audience::PublicSdk,
            method: HttpMethod::Get,
            path: "/health".into(),
            parameters: Vec::new(),
            request_body: None,
            responses: vec![ResponseSpec {
                status: 200,
                media_type: "application/json".into(),
                schema: SchemaRef::named("HealthLiveness"),
            }],
            error_codes: vec!["internal_failure".into()],
            trust_group: "liveness".into(),
            credential_scheme: "none".into(),
            capabilities: Vec::new(),
            browser_policy: "public".into(),
            body_limit: 1024,
            rate_limit_class: "liveness".into(),
            bootstrap_public: true,
            stream: None,
            deprecation: None,
        }
    }

    #[test]
    fn duplicate_id_and_path_fail() {
        let mut registry = ContractRegistry::new();
        registry.register(liveness()).unwrap();
        assert!(registry.register(liveness()).is_err());
        let mut other = liveness();
        other.operation_id = "health.alias".into();
        assert!(registry.register(other).unwrap_err().to_string().contains("duplicate method"));
    }
}
