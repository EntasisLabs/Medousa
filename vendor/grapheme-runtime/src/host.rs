use serde_json::Value as JsonValue;

#[derive(Debug, Clone)]
pub struct CapabilityCall {
    pub module: Option<String>,
    pub op: String,
    pub capability: String,
    pub arg_count: u16,
    pub args: JsonValue,
    pub step_index: usize,
}

#[derive(Debug, Clone)]
pub enum HostCallError {
    Retryable(String),
    Fatal(String),
}

pub trait CapabilityHost {
    fn call(&mut self, call: &CapabilityCall) -> Result<JsonValue, HostCallError>;
}
