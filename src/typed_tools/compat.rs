//! Boundary-only compatibility values for preserving permissive legacy wire
//! inputs while handlers migrate to typed DTOs.

use schemars::{JsonSchema, SchemaGenerator, schema::Schema};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

/// A schema-transparent optional value that treats invalid legacy JSON as
/// absent.
///
/// The wrapper is intentionally a boundary type. Internal commands should
/// consume it with [`CompatOption::into_option`] and operate on the resulting
/// validated value instead of carrying compatibility semantics into domain
/// code. Missing fields still require `#[serde(default)]` at the containing
/// field, just like `Option<T>` does when a custom deserializer is used.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompatOption<T>(Option<T>);

impl<T> CompatOption<T> {
    pub fn into_option(self) -> Option<T> {
        self.0
    }

    pub fn as_ref(&self) -> Option<&T> {
        self.0.as_ref()
    }

    pub fn is_none(&self) -> bool {
        self.0.is_none()
    }
}

impl<T> From<Option<T>> for CompatOption<T> {
    fn from(value: Option<T>) -> Self {
        Self(value)
    }
}

impl<'de, T: DeserializeOwned> Deserialize<'de> for CompatOption<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        Ok(Self(
            (!value.is_null())
                .then(|| serde_json::from_value(value).ok())
                .flatten(),
        ))
    }
}

impl<T: Serialize> Serialize for CompatOption<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<T: JsonSchema> JsonSchema for CompatOption<T> {
    fn schema_name() -> String {
        <Option<T>>::schema_name()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        <Option<T>>::json_schema(generator)
    }
}

/// A schema-transparent compatibility list.
///
/// Valid array entries are retained and entries of the wrong type are
/// ignored, matching the legacy string-list adapters. A non-array value is
/// treated as absent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompatList<T>(Option<Vec<T>>);

impl<T> CompatList<T> {
    pub fn into_option(self) -> Option<Vec<T>> {
        self.0
    }

    pub fn as_ref(&self) -> Option<&[T]> {
        self.0.as_deref()
    }

    pub fn is_none(&self) -> bool {
        self.0.is_none()
    }
}

impl<T> From<Option<Vec<T>>> for CompatList<T> {
    fn from(value: Option<Vec<T>>) -> Self {
        Self(value)
    }
}

impl<'de, T: DeserializeOwned> Deserialize<'de> for CompatList<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        Ok(Self(value.as_array().map(|items| {
            items
                .iter()
                .filter_map(|item| serde_json::from_value(item.clone()).ok())
                .collect()
        })))
    }
}

impl<T: Serialize> Serialize for CompatList<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<T: JsonSchema> JsonSchema for CompatList<T> {
    fn schema_name() -> String {
        <Option<Vec<T>>>::schema_name()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        <Option<Vec<T>>>::json_schema(generator)
    }
}

pub fn deserialize_lenient_optional_string<'de, D>(
    deserializer: D,
) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    CompatOption::<String>::deserialize(deserializer).map(CompatOption::into_option)
}

pub fn deserialize_lenient_optional_usize<'de, D>(
    deserializer: D,
) -> Result<Option<usize>, D::Error>
where
    D: Deserializer<'de>,
{
    CompatOption::<usize>::deserialize(deserializer).map(CompatOption::into_option)
}

pub fn deserialize_lenient_optional_bool<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: Deserializer<'de>,
{
    CompatOption::<bool>::deserialize(deserializer).map(CompatOption::into_option)
}

pub fn deserialize_lenient_optional_u64<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: Deserializer<'de>,
{
    CompatOption::<u64>::deserialize(deserializer).map(CompatOption::into_option)
}

pub fn deserialize_lenient_optional_i64<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: Deserializer<'de>,
{
    CompatOption::<i64>::deserialize(deserializer).map(CompatOption::into_option)
}

pub fn deserialize_lenient_optional_f64<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: Deserializer<'de>,
{
    CompatOption::<f64>::deserialize(deserializer).map(CompatOption::into_option)
}

pub fn deserialize_lenient_optional_string_list<'de, D>(
    deserializer: D,
) -> Result<Option<Vec<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    CompatList::<String>::deserialize(deserializer).map(CompatList::into_option)
}

#[cfg(test)]
mod tests {
    use schemars::schema_for;
    use serde::Deserialize;
    use serde_json::json;

    use super::{CompatList, CompatOption};

    #[derive(Debug, Deserialize, schemars::JsonSchema)]
    struct Fixture {
        #[serde(default)]
        #[schemars(with = "Option<String>")]
        text: CompatOption<String>,
        #[serde(default)]
        #[schemars(with = "Option<bool>")]
        flag: CompatOption<bool>,
        #[serde(default)]
        #[schemars(with = "Option<Vec<String>>")]
        values: CompatList<String>,
    }

    #[test]
    fn compatibility_values_absorb_missing_null_and_wrong_types() {
        let fixture: Fixture = serde_json::from_value(json!({
            "text": 42,
            "flag": null,
            "values": ["kept", 9, null, "also-kept"]
        }))
        .expect("compatibility input should deserialize");

        assert_eq!(fixture.text.into_option(), None);
        assert_eq!(fixture.flag.into_option(), None);
        assert_eq!(
            fixture.values.into_option(),
            Some(vec!["kept".to_string(), "also-kept".to_string()])
        );

        let missing: Fixture = serde_json::from_value(json!({})).expect("missing is absent");
        assert_eq!(missing.text.into_option(), None);
        assert_eq!(missing.flag.into_option(), None);
        assert_eq!(missing.values.into_option(), None);
    }

    #[test]
    fn compatibility_schemas_are_underlying_optional_schemas() {
        let schema = schema_for!(Fixture);
        let properties = schema
            .schema
            .object
            .as_ref()
            .expect("object schema")
            .properties
            .clone();

        let text = serde_json::to_value(&properties["text"]).expect("text schema");
        let flag = serde_json::to_value(&properties["flag"]).expect("flag schema");
        let values = serde_json::to_value(&properties["values"]).expect("values schema");
        assert_eq!(text.get("type"), Some(&json!(["string", "null"])));
        assert_eq!(flag.get("type"), Some(&json!(["boolean", "null"])));
        assert_eq!(values.get("type"), Some(&json!(["array", "null"])));
    }
}
