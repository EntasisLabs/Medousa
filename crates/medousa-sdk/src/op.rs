//! Expand generated operations into request paths.

use crate::SdkError;
use crate::generated::expand_path;
use crate::generated::ops::Operation;
use crate::transport::path_with_query;

pub fn op_path(op: &Operation, params: &[(&str, &str)]) -> Result<String, SdkError> {
    expand_path(op.path, params).map_err(SdkError::Transport)
}

pub fn op_path_query(
    op: &Operation,
    params: &[(&str, &str)],
    query: &[(&str, String)],
) -> Result<String, SdkError> {
    Ok(path_with_query(&op_path(op, params)?, query))
}
