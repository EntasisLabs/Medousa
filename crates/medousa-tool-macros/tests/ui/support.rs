#![allow(dead_code)]

pub mod stasis {
    pub mod domain {
        pub mod errors {
            use std::fmt::{Display, Formatter};

            pub type Result<T> = std::result::Result<T, StasisError>;

            #[derive(Debug)]
            pub enum StasisError {
                PortFailure(String),
            }

            impl Display for StasisError {
                fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
                    match self {
                        Self::PortFailure(message) => formatter.write_str(message),
                    }
                }
            }

            impl std::error::Error for StasisError {}
        }
    }

    pub mod prelude {
        pub use super::domain::errors::{Result, StasisError};
    }

    pub mod application {
        pub mod orchestration {
            pub mod tool_registry {
                use async_trait::async_trait;
                use serde_json::Value;

                use crate::stasis::domain::errors::Result;

                #[async_trait]
                pub trait StasisTool: Send + Sync {
                    fn name(&self) -> &'static str;
                    fn description(&self) -> Option<&'static str>;
                    fn input_schema(&self) -> Option<Value>;
                    fn output_schema(&self) -> Option<Value>;
                    async fn invoke(&self, input: Value) -> Result<Value>;
                }
            }
        }
    }
}

pub mod typed_tools {
    use schemars::JsonSchema;
    use serde::Serialize;
    use serde::de::DeserializeOwned;
    use serde_json::Value;

    #[derive(Clone, Copy)]
    pub struct ToolId(&'static str);

    impl ToolId {
        pub const fn new(value: &'static str) -> Self {
            Self(value)
        }

        pub const fn as_str(self) -> &'static str {
            self.0
        }
    }

    pub trait ToolIdSource {
        fn resolve(self) -> ToolId;
    }

    impl ToolIdSource for ToolId {
        fn resolve(self) -> ToolId {
            self
        }
    }

    impl ToolIdSource for &'static str {
        fn resolve(self) -> ToolId {
            ToolId::new(self)
        }
    }

    pub fn resolve_tool_id(source: impl ToolIdSource) -> ToolId {
        source.resolve()
    }

    pub struct ToolContract {
        pub input_schema: Value,
        pub output_schema: Value,
    }

    pub trait TypedTool: Send + Sync + 'static {
        type Input: DeserializeOwned + JsonSchema + Send + 'static;
        type Output: Serialize + JsonSchema + Send + 'static;

        fn tool_id() -> ToolId;
        fn description() -> &'static str;
        fn contract() -> &'static ToolContract;
    }

    pub fn build_contract<T: TypedTool>() -> Result<ToolContract, String> {
        let input_schema = serde_json::to_value(schemars::schema_for!(T::Input))
            .map_err(|error| error.to_string())?;
        let output_schema = serde_json::to_value(schemars::schema_for!(T::Output))
            .map_err(|error| error.to_string())?;
        Ok(ToolContract {
            input_schema,
            output_schema,
        })
    }

    pub fn deserialize_input<T: DeserializeOwned>(
        tool_id: ToolId,
        input: Value,
    ) -> crate::stasis::prelude::Result<T> {
        serde_json::from_value(input).map_err(|error| {
            crate::stasis::prelude::StasisError::PortFailure(format!(
                "invalid input for {}: {error}",
                tool_id.as_str()
            ))
        })
    }

    pub fn serialize_output<T: Serialize>(
        tool_id: ToolId,
        output: T,
    ) -> crate::stasis::prelude::Result<Value> {
        serde_json::to_value(output).map_err(|error| {
            crate::stasis::prelude::StasisError::PortFailure(format!(
                "invalid output for {}: {error}",
                tool_id.as_str()
            ))
        })
    }

    pub mod __private {
        pub use crate::stasis::application::orchestration::tool_registry::StasisTool;
        pub use crate::stasis::domain::errors::Result as StasisResult;
        pub use async_trait;
        pub use serde_json::Value;
        pub use std::sync::OnceLock;
    }
}
