//! DuckDuckGo HTML lite search, challenge heuristics, and in-memory cache.

mod cache;
mod challenge;
mod fetch;
mod search;

pub use cache::SearchCache;
pub use challenge::{ChallengeReason, detect_challenge};
pub use fetch::{fetch_url_markdown, markdown_from_html, FetchResult};
pub use search::{
    search_ddg_html, search_ddg_html_async, search_ddg_html_cached, search_ddg_html_cached_async,
    search_response_from_ddg_html, SearchHit, SearchResponse,
};

pub const DEFAULT_USER_AGENT: &str =
    "Mozilla/5.0 (compatible; MedousaBrowserLite/1.0; +https://medousa.dev)";
