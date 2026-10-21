//! Repository labels, eligible people and submitted review states stay in the package.
use super::*;

pub(super) fn label(raw: &Value) -> ReviewLabel {
    let name = text(raw, "name");
    ReviewLabel {
        id: name.clone(),
        name,
        description: text(raw, "description"),
        color: raw["color"]
            .as_str()
            .filter(|v| v.len() == 6 && v.bytes().all(|c| c.is_ascii_hexdigit()))
            .map(str::to_owned),
    }
}

pub(super) fn validate_labels(labels: &[String]) -> Result<(), ReviewProviderError> {
    if labels.is_empty()
        || labels.len() > 100
        || labels
            .iter()
            .any(|v| v.trim().is_empty() || v.len() > 200 || v.chars().any(char::is_control))
    {
        return Err(invalid("Choose up to 100 repository labels."));
    }
    Ok(())
}
pub(super) fn validate_reviewers(reviewers: &[String]) -> Result<(), ReviewProviderError> {
    if reviewers.is_empty()
        || reviewers.len() > 15
        || reviewers.iter().any(|v| {
            let v = v.strip_prefix("team:").unwrap_or(v);
            v.is_empty()
                || v.len() > 100
                || !v.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
        })
    {
        return Err(invalid("Choose up to 15 reviewer usernames."));
    }
    Ok(())
}

pub(super) fn reviewer_body(reviewers: &[String]) -> Value {
    let (teams, users): (Vec<_>, Vec<_>) = reviewers.iter().partition(|id| id.starts_with("team:"));
    serde_json::json!({"reviewers": users, "team_reviewers": teams.iter().map(|id| id.trim_start_matches("team:")).collect::<Vec<_>>()})
}

pub(super) async fn labels(
    github: &GitHub<'_>,
    review: &HostedReviewRef,
    cursor: Option<String>,
) -> Result<ReviewProviderResult, ReviewProviderError> {
    let n = page(&cursor)?;
    let rows = array(
        github
            .api(&format!(
                "repos/{}/labels?per_page=100&page={n}",
                review.repository
            ))
            .await?,
    )?;
    Ok(ReviewProviderResult::Labels {
        next_cursor: next_page(n, rows.len()),
        labels: rows.iter().map(label).collect(),
    })
}

pub(super) async fn candidates(
    github: &GitHub<'_>,
    review: &HostedReviewRef,
    cursor: Option<String>,
) -> Result<ReviewProviderResult, ReviewProviderError> {
    let n = page(&cursor)?;
    let rows = array(
        github
            .api(&format!(
                "repos/{}/collaborators?per_page=100&page={n}",
                review.repository
            ))
            .await?,
    )?;
    Ok(ReviewProviderResult::People {
        next_cursor: next_page(n, rows.len()),
        people: rows
            .iter()
            .map(|row| ReviewPerson {
                id: text(row, "login"),
                name: text(row, "login"),
                state: None,
            })
            .collect(),
    })
}

pub(super) async fn reviewers(
    github: &GitHub<'_>,
    review: &HostedReviewRef,
    cursor: Option<String>,
) -> Result<ReviewProviderResult, ReviewProviderError> {
    let n = page(&cursor)?;
    let rows = array(
        github
            .api(&format!(
                "repos/{}/pulls/{}/reviews?per_page=100&page={n}",
                review.repository, review.native_id
            ))
            .await?,
    )?;
    let people = rows
        .iter()
        .filter_map(|row| {
            let state = match row["state"].as_str()? {
                "APPROVED" => "approved",
                "CHANGES_REQUESTED" => "changes_requested",
                "COMMENTED" => "commented",
                "DISMISSED" => "dismissed",
                _ => return None,
            };
            Some(ReviewPerson {
                id: text(&row["user"], "login"),
                name: text(&row["user"], "login"),
                state: Some(state.into()),
            })
        })
        .collect();
    Ok(ReviewProviderResult::People {
        people,
        next_cursor: next_page(n, rows.len()),
    })
}
