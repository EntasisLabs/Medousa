use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::id::{path_parameters, stable_operation_id};

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ContractError {
    #[error("{0}")]
    Invalid(String),
}

impl ContractError {
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
}

impl HttpMethod {
    pub fn parse(raw: &str) -> Result<Self, ContractError> {
        match raw {
            "GET" => Ok(Self::Get),
            "POST" => Ok(Self::Post),
            "PUT" => Ok(Self::Put),
            "PATCH" => Ok(Self::Patch),
            "DELETE" => Ok(Self::Delete),
            "HEAD" => Ok(Self::Head),
            "OPTIONS" => Ok(Self::Options),
            "SSE" => Err(ContractError::invalid(
                "SSE is not an HTTP method; declare GET with stream metadata",
            )),
            other => Err(ContractError::invalid(format!(
                "unsupported HTTP method {other}"
            ))),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
            Self::Head => "HEAD",
            Self::Options => "OPTIONS",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureProfile {
    Core,
    Pairing,
    Preview,
    OptionalWorkload,
    Development,
    TestOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Audience {
    PublicSdk,
    NativeOnly,
    Internal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stability {
    Stable,
    Experimental,
    Deprecated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParameterLocation {
    Path,
    Query,
    Header,
    Cookie,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaRef {
    pub name: String,
    #[serde(default)]
    pub opaque: bool,
}

impl SchemaRef {
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            opaque: false,
        }
    }

    pub fn deferred(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            opaque: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParameterSpec {
    pub name: String,
    pub location: ParameterLocation,
    pub required: bool,
    pub schema: SchemaRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestBodySpec {
    pub media_type: String,
    pub schema: SchemaRef,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseSpec {
    pub status: u16,
    pub media_type: String,
    pub schema: SchemaRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamSpec {
    pub item_schema: SchemaRef,
    #[serde(default)]
    pub last_event_id: bool,
    #[serde(default)]
    pub heartbeat_comments: bool,
    #[serde(default)]
    pub replay: bool,
}

impl StreamSpec {
    pub fn json_events(item_schema: SchemaRef) -> Self {
        Self {
            item_schema,
            last_event_id: true,
            heartbeat_comments: true,
            replay: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationSpec {
    pub operation_id: String,
    pub stability: Stability,
    pub feature_profile: FeatureProfile,
    pub audience: Audience,
    pub method: HttpMethod,
    pub path: String,
    #[serde(default)]
    pub parameters: Vec<ParameterSpec>,
    #[serde(default)]
    pub request_body: Option<RequestBodySpec>,
    pub responses: Vec<ResponseSpec>,
    #[serde(default)]
    pub error_codes: Vec<String>,
    pub trust_group: String,
    pub credential_scheme: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    pub browser_policy: String,
    pub body_limit: usize,
    pub rate_limit_class: String,
    pub bootstrap_public: bool,
    #[serde(default)]
    pub stream: Option<StreamSpec>,
    #[serde(default)]
    pub deprecation: Option<String>,
}

impl OperationSpec {
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.operation_id.trim().is_empty() {
            return Err(ContractError::invalid("operation_id is required"));
        }
        if !self.path.starts_with('/') {
            return Err(ContractError::invalid("path must be absolute"));
        }
        if self.path.contains('?') {
            return Err(ContractError::invalid(
                "query text must not be embedded in a path template",
            ));
        }
        if self.body_limit == 0 {
            return Err(ContractError::invalid("body_limit must be non-zero"));
        }
        if self.trust_group.trim().is_empty() {
            return Err(ContractError::invalid("trust_group is required"));
        }
        if self.browser_policy.trim().is_empty() {
            return Err(ContractError::invalid("browser_policy is required"));
        }
        if self.rate_limit_class.trim().is_empty() {
            return Err(ContractError::invalid("rate_limit_class is required"));
        }
        if self.responses.is_empty() {
            return Err(ContractError::invalid(
                "every operation requires at least one success response",
            ));
        }
        if !self.bootstrap_public
            && self.trust_group != "preview"
            && self.capabilities.is_empty()
        {
            return Err(ContractError::invalid(
                "protected route requires a capability",
            ));
        }
        if self.bootstrap_public && !self.capabilities.is_empty() {
            return Err(ContractError::invalid(
                "bootstrap route cannot require an application capability",
            ));
        }
        if self.stream.is_some() && self.method != HttpMethod::Get {
            return Err(ContractError::invalid(
                "streams must be HTTP GET operations",
            ));
        }
        for schema in self.schema_refs() {
            forbid_untyped_schema(schema)?;
        }
        Ok(())
    }

    fn schema_refs(&self) -> Vec<&SchemaRef> {
        let mut refs = Vec::new();
        if let Some(body) = &self.request_body {
            refs.push(&body.schema);
        }
        for response in &self.responses {
            refs.push(&response.schema);
        }
        for parameter in &self.parameters {
            refs.push(&parameter.schema);
        }
        if let Some(stream) = &self.stream {
            refs.push(&stream.item_schema);
        }
        refs
    }

    pub fn with_path_parameters(mut self) -> Self {
        if self.parameters.iter().any(|p| p.location == ParameterLocation::Path) {
            return self;
        }
        for name in path_parameters(&self.path) {
            self.parameters.push(ParameterSpec {
                name: name.clone(),
                location: ParameterLocation::Path,
                required: true,
                schema: SchemaRef::named("string"),
            });
        }
        self
    }
}

#[allow(dead_code)]
pub fn inferred_operation_id(method: HttpMethod, path: &str) -> String {
    stable_operation_id(method.as_str(), path)
}

fn forbid_untyped_schema(schema: &SchemaRef) -> Result<(), ContractError> {
    if schema.name.trim().is_empty() {
        return Err(ContractError::invalid("schema name is required"));
    }
    let lower = schema.name.to_ascii_lowercase();
    if lower == "value"
        || lower == "jsonvalue"
        || lower == "any"
        || lower.contains("serde_json")
        || lower.contains("untagged")
    {
        return Err(ContractError::invalid(
            "serde_json::Value and untagged-ambiguous types cannot be contract schemas",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> OperationSpec {
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
    fn omitted_policy_is_a_build_error() {
        let mut spec = sample();
        spec.trust_group.clear();
        assert!(spec.validate().is_err());
        spec = sample();
        spec.body_limit = 0;
        assert!(spec.validate().is_err());
        spec = sample();
        spec.responses.clear();
        assert!(spec.validate().is_err());
        spec = sample();
        spec.bootstrap_public = false;
        spec.trust_group = "portal".into();
        spec.capabilities.clear();
        assert!(spec.validate().unwrap_err().to_string().contains("capability"));
    }

    #[test]
    fn query_in_path_is_rejected() {
        let mut spec = sample();
        spec.path = "/v1/sessions?limit={limit}".into();
        assert!(spec.validate().unwrap_err().to_string().contains("query"));
    }

    #[test]
    fn json_value_and_untagged_schemas_are_rejected() {
        let mut spec = sample();
        spec.responses[0].schema = SchemaRef::named("serde_json::Value");
        assert!(spec.validate().unwrap_err().to_string().contains("Value"));
        spec = sample();
        spec.responses[0].schema = SchemaRef::named("UntaggedEvent");
        assert!(
            spec.validate()
                .unwrap_err()
                .to_string()
                .contains("untagged")
        );
    }
}
