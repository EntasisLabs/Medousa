use std::fmt::{Display, Formatter};

use serde::{Serialize, Serializer};

/// Validated identity for a statically known first-party tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ToolId(&'static str);

impl ToolId {
    /// Creates a tool id and fails compilation when a typed constant is invalid.
    pub const fn new(value: &'static str) -> Self {
        match Self::try_new(value) {
            Ok(tool_id) => tool_id,
            Err(_) => panic!("invalid tool id"),
        }
    }

    pub const fn try_new(value: &'static str) -> Result<Self, ToolIdError> {
        if is_valid_tool_id(value.as_bytes()) {
            Ok(Self(value))
        } else {
            Err(ToolIdError { value })
        }
    }

    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl AsRef<str> for ToolId {
    fn as_ref(&self) -> &str {
        self.0
    }
}

impl Display for ToolId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.0)
    }
}

impl Serialize for ToolId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolIdError {
    value: &'static str,
}

impl ToolIdError {
    pub const fn value(self) -> &'static str {
        self.value
    }
}

impl Display for ToolIdError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "tool id `{}` must start with an ASCII letter and contain only ASCII letters, digits, `_`, or `-`",
            self.value
        )
    }
}

impl std::error::Error for ToolIdError {}

/// Compatibility input accepted by the macro while legacy name constants are
/// progressively converted to [`ToolId`].
pub trait ToolIdSource {
    fn resolve_tool_id(self) -> ToolId;
}

impl ToolIdSource for ToolId {
    fn resolve_tool_id(self) -> ToolId {
        self
    }
}

impl ToolIdSource for &'static str {
    fn resolve_tool_id(self) -> ToolId {
        ToolId::try_new(self).unwrap_or_else(|error| panic!("{error}"))
    }
}

pub fn resolve_tool_id(source: impl ToolIdSource) -> ToolId {
    source.resolve_tool_id()
}

const fn is_valid_tool_id(value: &[u8]) -> bool {
    if value.is_empty() || !value[0].is_ascii_alphabetic() {
        return false;
    }

    let mut index = 1;
    while index < value.len() {
        let byte = value[index];
        if !(byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-') {
            return false;
        }
        index += 1;
    }
    true
}
