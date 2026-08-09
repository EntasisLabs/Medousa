//! Boundary-only deserializers for preserving permissive legacy wire inputs
//! while handlers migrate to typed DTOs.

use serde::{Deserialize, Deserializer};
use serde_json::Value;

pub fn deserialize_lenient_optional_string<'de, D>(
    deserializer: D,
) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(value.as_str().map(str::to_string))
}

pub fn deserialize_lenient_optional_usize<'de, D>(
    deserializer: D,
) -> Result<Option<usize>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(value.as_u64().and_then(|value| usize::try_from(value).ok()))
}
