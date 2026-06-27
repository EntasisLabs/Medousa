use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::cache::SearchCache;
use crate::challenge::{detect_challenge, ChallengeReason};
use crate::DEFAULT_USER_AGENT;

static RESULT_LINK: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(?is)<a[^>]+class="[^"]*result__a[^"]*"[^>]+href="([^"]+)"[^>]*>(.*?)</a>"#,
    )
    .expect("result link regex")
});
static RESULT_SNIPPET: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?is)<a[^>]+class="[^"]*result__snippet[^"]*"[^>]*>(.*?)</a>"#)
        .expect("snippet regex")
});
static TAGS: Lazy<Regex> = Lazy::new(|| Regex::new(r"<[^>]+>").expect("tag regex"));

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchHit {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchResponse {
    pub query: String,
    pub provider: String,
    pub results: Vec<SearchHit>,
    pub cached: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub challenge: Option<String>,
}

static GLOBAL_CACHE: Lazy<SearchCache> = Lazy::new(SearchCache::default);

pub fn search_ddg_html_cached(query: &str, max_results: usize) -> Result<SearchResponse, String> {
    if let Some(cached) = GLOBAL_CACHE.get(query, max_results) {
        return Ok(SearchResponse {
            cached: true,
            ..cached
        });
    }
    let response = search_ddg_html(query, max_results)?;
    GLOBAL_CACHE.put(query, max_results, response.clone());
    Ok(response)
}

pub async fn search_ddg_html_async(query: &str, max_results: usize) -> Result<SearchResponse, String> {
    let query = query.to_string();
    tokio::task::spawn_blocking(move || search_ddg_html(&query, max_results))
        .await
        .map_err(|err| err.to_string())?
}

pub async fn search_ddg_html_cached_async(
    query: &str,
    max_results: usize,
) -> Result<SearchResponse, String> {
    if let Some(cached) = GLOBAL_CACHE.get(query, max_results) {
        return Ok(SearchResponse {
            cached: true,
            ..cached
        });
    }
    let response = search_ddg_html_async(query, max_results).await?;
    GLOBAL_CACHE.put(query, max_results, response.clone());
    Ok(response)
}

pub fn search_ddg_html(query: &str, max_results: usize) -> Result<SearchResponse, String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Err("query is required".to_string());
    }
    let max_results = max_results.clamp(1, 20);
    let url = format!(
        "https://html.duckduckgo.com/html/?q={}",
        urlencoding_encode(trimmed)
    );
    let client = reqwest::blocking::Client::builder()
        .user_agent(DEFAULT_USER_AGENT)
        .timeout(std::time::Duration::from_secs(12))
        .build()
        .map_err(|err| err.to_string())?;
    let response = client.get(&url).send().map_err(|err| err.to_string())?;
    let status = response.status().as_u16();
    let final_url = response.url().to_string();
    let body = response.text().map_err(|err| err.to_string())?;
    let results = parse_ddg_results(&body, max_results);
    if let Some(reason) = detect_challenge(&final_url, status, &body, results.len()) {
        return Ok(SearchResponse {
            query: trimmed.to_string(),
            provider: "duckduckgo_html".to_string(),
            results,
            cached: false,
            challenge: Some(challenge_label(reason)),
        });
    }
    Ok(SearchResponse {
        query: trimmed.to_string(),
        provider: "duckduckgo_html".to_string(),
        results,
        cached: false,
        challenge: None,
    })
}

fn challenge_label(reason: ChallengeReason) -> String {
    match reason {
        ChallengeReason::CaptchaUrl => "captcha_url".to_string(),
        ChallengeReason::CaptchaBody => "captcha_body".to_string(),
        ChallengeReason::RateLimited => "rate_limited".to_string(),
        ChallengeReason::EmptyResults => "empty_results".to_string(),
    }
}

fn parse_ddg_results(html: &str, max_results: usize) -> Vec<SearchHit> {
    let links: Vec<(String, String)> = RESULT_LINK
        .captures_iter(html)
        .filter_map(|cap| {
            let url = decode_ddg_redirect(cap.get(1)?.as_str());
            let title = strip_tags(cap.get(2)?.as_str());
            if url.is_empty() || title.is_empty() {
                None
            } else {
                Some((url, title))
            }
        })
        .collect();
    let snippets: Vec<String> = RESULT_SNIPPET
        .captures_iter(html)
        .map(|cap| strip_tags(cap.get(1).map(|m| m.as_str()).unwrap_or("")))
        .collect();
    links
        .into_iter()
        .zip(snippets.into_iter().chain(std::iter::repeat(String::new())))
        .take(max_results)
        .map(|((url, title), snippet)| SearchHit {
            title,
            url,
            snippet,
        })
        .collect()
}

fn decode_ddg_redirect(href: &str) -> String {
    if let Some(rest) = href.strip_prefix("//duckduckgo.com/l/?uddg=") {
        if let Ok(decoded) = urlencoding_decode(rest.split('&').next().unwrap_or(rest)) {
            return decoded;
        }
    }
    href.trim().to_string()
}

fn strip_tags(raw: &str) -> String {
    TAGS.replace_all(raw, "")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn urlencoding_encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn urlencoding_decode(value: &str) -> Result<String, String> {
    let mut out = Vec::new();
    let bytes = value.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).map_err(|e| e.to_string())?;
            out.push(u8::from_str_radix(hex, 16).map_err(|e| e.to_string())?);
            i += 3;
        } else if bytes[i] == b'+' {
            out.push(b' ');
            i += 1;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fixture_hits() {
        let html = include_str!("../testdata/ddg_sample.html");
        let hits = parse_ddg_results(html, 3);
        assert_eq!(hits.len(), 2);
        assert!(hits[0].title.contains("Rust"));
        assert!(hits[0].url.starts_with("http"));
    }
}
