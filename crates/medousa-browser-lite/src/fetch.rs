use serde::{Deserialize, Serialize};

use crate::DEFAULT_USER_AGENT;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FetchResult {
    pub url: String,
    pub title: String,
    pub markdown: String,
}

pub fn fetch_url_markdown(url: &str, max_chars: usize) -> Result<FetchResult, String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err("url is required".to_string());
    }
    let max_chars = max_chars.clamp(256, 32_000);
    let client = reqwest::blocking::Client::builder()
        .user_agent(DEFAULT_USER_AGENT)
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|err| err.to_string())?;
    let response = client.get(trimmed).send().map_err(|err| err.to_string())?;
    let final_url = response.url().to_string();
    let body = response.text().map_err(|err| err.to_string())?;
    let title = extract_title(&body).unwrap_or_else(|| final_url.clone());
    let markdown = html_to_markdown_lite(&body, max_chars);
    Ok(FetchResult {
        url: final_url,
        title,
        markdown,
    })
}

pub fn markdown_from_html(html: &str, url: &str, max_chars: usize) -> FetchResult {
    let max_chars = max_chars.clamp(256, 32_000);
    let title = extract_title(html).unwrap_or_else(|| url.to_string());
    let markdown = html_to_markdown_lite(html, max_chars);
    FetchResult {
        url: url.to_string(),
        title,
        markdown,
    }
}

fn extract_title(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let start = lower.find("<title>")? + 7;
    let end = lower[start..].find("</title>")? + start;
    Some(
        html[start..end]
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
    )
}

fn html_to_markdown_lite(html: &str, max_chars: usize) -> String {
    let mut text = html.to_string();
    for tag in ["script", "style", "nav", "footer", "header"] {
        let pattern = format!(r"(?is)<{tag}[^>]*>.*?</{tag}>");
        text = regex::Regex::new(&pattern)
            .ok()
            .map(|re| re.replace_all(&text, " ").to_string())
            .unwrap_or(text);
    }
    text = regex::Regex::new(r"(?is)<br\s*/?>")
        .ok()
        .map(|re| re.replace_all(&text, "\n").to_string())
        .unwrap_or(text);
    text = regex::Regex::new(r"(?is)<p[^>]*>")
        .ok()
        .map(|re| re.replace_all(&text, "\n\n").to_string())
        .unwrap_or(text);
    text = regex::Regex::new(r"<[^>]+>")
        .ok()
        .map(|re| re.replace_all(&text, " ").to_string())
        .unwrap_or(text);
    let collapsed = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if collapsed.len() <= max_chars {
        collapsed
    } else {
        format!("{}…", &collapsed[..max_chars])
    }
}
