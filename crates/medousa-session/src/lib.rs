//! Medousa workshop shell session host.
//!
//! One OS PTY per `session_id`. Home (Terminal tabs) and agent coding tools are
//! peers on the same session — no tmux-style multiplexer, no pane ownership.

pub mod server;
pub mod session;

pub use server::{SessionHostConfig, SessionHostState, serve};
pub use session::{Session, SessionId, SessionManager};

pub const ENGINE_NAME: &str = "medousa-session";
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const ENGINE_API_REVISION: u32 = 1;
