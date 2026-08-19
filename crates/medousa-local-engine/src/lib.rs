//! Embedded mistralrs runtime for `medousa_local`.

pub mod engine;

pub use engine::{
    DEFAULT_LOCAL_ENGINE_BIND, LoadedEngineHandle, LocalEngineConfig, LocalEngineRuntime,
    load_embedded_engine,
};
