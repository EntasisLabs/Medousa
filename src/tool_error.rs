//! Medousa-owned error context kept until the Stasis adapter boundary.

use std::fmt::{Display, Formatter};

use stasis::domain::errors::StasisError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolErrorKind {
    Input,
    NotFound,
    Policy,
    Dependency,
    Conflict,
    External,
    Canceled,
}

impl Display for ToolErrorKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Input => "input",
            Self::NotFound => "not_found",
            Self::Policy => "policy",
            Self::Dependency => "dependency",
            Self::Conflict => "conflict",
            Self::External => "external",
            Self::Canceled => "canceled",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolError {
    kind: ToolErrorKind,
    tool_id: Option<String>,
    operation: Option<String>,
    message: String,
}

impl ToolError {
    pub fn new(kind: ToolErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            tool_id: None,
            operation: None,
            message: message.into(),
        }
    }

    pub fn input(tool_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(ToolErrorKind::Input, message).for_tool(tool_id)
    }

    pub fn not_found(tool_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(ToolErrorKind::NotFound, message).for_tool(tool_id)
    }

    pub fn policy(tool_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(ToolErrorKind::Policy, message).for_tool(tool_id)
    }

    pub fn dependency(tool_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(ToolErrorKind::Dependency, message).for_tool(tool_id)
    }

    pub fn external(tool_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(ToolErrorKind::External, message).for_tool(tool_id)
    }

    pub fn kind(&self) -> ToolErrorKind {
        self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn tool_id(&self) -> Option<&str> {
        self.tool_id.as_deref()
    }

    pub fn operation(&self) -> Option<&str> {
        self.operation.as_deref()
    }

    pub fn for_tool(mut self, tool_id: impl Into<String>) -> Self {
        self.tool_id = Some(tool_id.into());
        self
    }

    pub fn with_operation(mut self, operation: impl Into<String>) -> Self {
        self.operation = Some(operation.into());
        self
    }

    pub fn into_stasis(self) -> StasisError {
        StasisError::PortFailure(self.to_string())
    }
}

impl Display for ToolError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.kind)?;
        if let Some(tool_id) = self.tool_id() {
            write!(f, " tool={tool_id}")?;
        }
        if let Some(operation) = self.operation() {
            write!(f, " operation={operation}")?;
        }
        write!(f, ": {}", self.message)
    }
}

impl std::error::Error for ToolError {}

impl From<ToolError> for StasisError {
    fn from(error: ToolError) -> Self {
        error.into_stasis()
    }
}

/// Add operation context to an existing Medousa tool error without changing
/// its category.
pub trait ToolResultExt<T> {
    fn tool_context(
        self,
        tool_id: impl Into<String>,
        operation: impl Into<String>,
    ) -> Result<T, ToolError>;
}

impl<T> ToolResultExt<T> for Result<T, ToolError> {
    fn tool_context(
        self,
        tool_id: impl Into<String>,
        operation: impl Into<String>,
    ) -> Result<T, ToolError> {
        self.map_err(|error| error.for_tool(tool_id).with_operation(operation))
    }
}

/// Convert a dependency error into a dependency-category tool error while
/// adding the tool and operation that exposed it.
pub trait DependencyResultExt<T> {
    fn dependency_context(
        self,
        tool_id: impl Into<String>,
        operation: impl Into<String>,
    ) -> Result<T, ToolError>;
}

impl<T, E: Display> DependencyResultExt<T> for Result<T, E> {
    fn dependency_context(
        self,
        tool_id: impl Into<String>,
        operation: impl Into<String>,
    ) -> Result<T, ToolError> {
        self.map_err(|error| {
            ToolError::dependency(tool_id, error.to_string()).with_operation(operation)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{DependencyResultExt, ToolError, ToolErrorKind, ToolResultExt};

    #[test]
    fn context_preserves_tool_error_category() {
        let error = Err::<(), _>(ToolError::policy("tool.test", "approval required"))
            .tool_context("tool.test", "invoke")
            .unwrap_err();
        assert_eq!(error.kind(), ToolErrorKind::Policy);
        assert_eq!(error.tool_id(), Some("tool.test"));
        assert_eq!(error.operation(), Some("invoke"));
        assert!(error.to_string().contains("policy"));
    }

    #[test]
    fn dependency_context_is_distinct_from_input_errors() {
        let error = Err::<(), _>("backend unavailable")
            .dependency_context("tool.test", "load state")
            .unwrap_err();
        assert_eq!(error.kind(), ToolErrorKind::Dependency);
        assert!(error.to_string().contains("backend unavailable"));
    }
}
