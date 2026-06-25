//! Embedded mistralrs runtime for `medousa_local`.

pub mod engine;

pub use engine::{
    load_embedded_engine, LocalEngineConfig, LocalEngineRuntime, LoadedEngineHandle,
    DEFAULT_LOCAL_ENGINE_BIND,
};
