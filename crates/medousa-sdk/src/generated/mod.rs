//! Generated low-level daemon operations. Do not edit `ops.rs`.

pub mod ops;

pub fn expand_path(template: &str, params: &[(&str, &str)]) -> Result<String, String> {
    medousa_api_contract::expand_path(template, params).map_err(|error| error.to_string())
}
