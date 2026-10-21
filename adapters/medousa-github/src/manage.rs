//! GitHub inbox, review decisions, PR lifecycle and CI. No GitHub semantics leak
//! into the daemon dispatcher or the native shell.
use super::*;
use serde_json::json;

pub(super) async fn project_details(
    github: &GitHub<'_>,
    repository: &str,
    raw: &Value,
) -> Result<HostedReview, ReviewProviderError> {
    let mut review = project_review(github.host, repository, raw);
    let user = github.api("user").await?;
    let repo = github.api(&format!("repos/{repository}")).await?;
    let owner = text(&user, "login") == review.author;
    let manage = ["push", "maintain", "admin"]
        .iter()
        .any(|key| repo["permissions"][key] == true);
    review.actions.push("comment".into());
    if manage || repo["permissions"]["triage"] == true {
        review
            .actions
            .extend(["add_labels".into(), "remove_label".into()]);
    }
    if review.state != "merged" && (owner || manage) {
        review.actions.push("edit".into());
        review.actions.push(
            if review.state == "closed" {
                "reopen"
            } else {
                "close"
            }
            .into(),
        );
        if review.state != "closed" {
            review.actions.push(
                if review.state == "draft" {
                    "ready"
                } else {
                    "draft"
                }
                .into(),
            );
            review.actions.push("request_reviewers".into());
            review.actions.push("remove_reviewers".into());
        }
    }
    if review.state == "open" || review.state == "draft" {
        review.actions.push("submit_review".into());
        if !owner && review.state == "open" {
            review
                .actions
                .extend(["approve".into(), "request_changes".into()]);
        }
    }
    if manage {
        review
            .actions
            .extend(["rerun_check".into(), "cancel_check".into()]);
        if review.state == "open" {
            for (key, method) in [
                ("allow_merge_commit", "merge"),
                ("allow_squash_merge", "squash"),
                ("allow_rebase_merge", "rebase"),
            ] {
                if repo[key] == true {
                    review.merge_methods.push(method.into());
                }
            }
            if !review.merge_methods.is_empty() {
                review.actions.push("merge".into());
            }
        }
    }
    Ok(review)
}

pub(super) async fn inbox(
    github: &GitHub<'_>,
    folder: ReviewInboxFolder,
    cursor: Option<String>,
) -> Result<ReviewProviderResult, ReviewProviderError> {
    let login = text(&github.api("user").await?, "login");
    if login.is_empty()
        || !login
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-')
    {
        return Err(invalid("GitHub returned an invalid account."));
    }
    let qualifier = match folder {
        ReviewInboxFolder::Authored => format!("author:{login}"),
        ReviewInboxFolder::ReviewRequested => format!("review-requested:{login}"),
        ReviewInboxFolder::Participating => format!("involves:{login}"),
    };
    let page = page(&cursor)?;
    if page > 10 {
        return Err(invalid(
            "GitHub search is limited to the first 1,000 results.",
        ));
    }
    let mut url = Url::parse("https://placeholder.invalid/search/issues").expect("static URL");
    url.query_pairs_mut()
        .append_pair(
            "q",
            &format!(
                "is:pr {qualifier}{}",
                if matches!(folder, ReviewInboxFolder::ReviewRequested) {
                    " is:open"
                } else {
                    ""
                }
            ),
        )
        .append_pair("sort", "updated")
        .append_pair("order", "desc")
        .append_pair("per_page", "100")
        .append_pair("page", &page.to_string());
    let raw = github
        .api(&format!(
            "search/issues?{}",
            url.query().unwrap_or_default()
        ))
        .await?;
    if raw["incomplete_results"] == true {
        return Err(error(
            ReviewProviderErrorCode::Unavailable,
            "GitHub returned an incomplete inbox. Refresh to try again.",
        ));
    }
    let rows = array(raw["items"].clone())?;
    let mut reviews = Vec::new();
    for row in &rows {
        let reference = resolve_url(github.host, &text(row, "html_url"))?;
        reviews.push(project_review(github.host, &reference.repository, row));
    }
    Ok(ReviewProviderResult::Reviews {
        reviews,
        next_cursor: if page < 10 {
            next_page(page, rows.len())
        } else {
            None
        },
    })
}
fn numeric_id(value: &str, prefix: &str) -> Result<u64, ReviewProviderError> {
    value
        .strip_prefix(prefix)
        .and_then(|id| id.parse::<u64>().ok())
        .filter(|id| *id > 0)
        .ok_or_else(|| invalid("Invalid provider resource identity."))
}
fn body_valid(body: &str) -> Result<(), ReviewProviderError> {
    if body.trim().is_empty() || body.len() > 60_000 {
        return Err(invalid("Enter a comment of at most 60,000 bytes."));
    }
    Ok(())
}
async fn workflow(
    github: &GitHub<'_>,
    review: &HostedReviewRef,
    revision: &str,
    id: u64,
) -> Result<Value, ReviewProviderError> {
    github.require_revision(review, revision).await?;
    let run = github
        .api(&format!("repos/{}/actions/runs/{id}", review.repository))
        .await?;
    if run["head_sha"] != revision {
        return Err(invalid("This workflow belongs to another revision."));
    }
    Ok(run)
}
pub(super) async fn check_details(
    github: &GitHub<'_>,
    review: &HostedReviewRef,
    revision: &str,
    id: &str,
) -> Result<ReviewProviderResult, ReviewProviderError> {
    github.require_revision(review, revision).await?;
    if id.starts_with("workflows:") {
        let id = numeric_id(id, "workflows:")?;
        let run = workflow(github, review, revision, id).await?;
        let raw = github
            .api(&format!(
                "repos/{}/actions/runs/{id}/jobs?per_page=100",
                review.repository
            ))
            .await?;
        let jobs = array(raw["jobs"].clone())?;
        let mut annotations = Vec::new();
        for job in &jobs {
            annotations.push(format!(
                "{}: {}",
                text(job, "name"),
                job["conclusion"]
                    .as_str()
                    .unwrap_or(job["status"].as_str().unwrap_or("unknown"))
            ));
            for step in job["steps"].as_array().into_iter().flatten() {
                annotations.push(format!(
                    "  {}: {}",
                    text(step, "name"),
                    step["conclusion"]
                        .as_str()
                        .unwrap_or(step["status"].as_str().unwrap_or("unknown"))
                ));
            }
        }
        if number(&raw, "total_count") > jobs.len() as u64 {
            annotations.push("More jobs are available on the provider page.".into());
        }
        return Ok(ReviewProviderResult::CheckDetails {
            title: text(&run, "name"),
            summary: run["conclusion"]
                .as_str()
                .unwrap_or(run["status"].as_str().unwrap_or("unknown"))
                .into(),
            text: "Job steps are shown below. Open the workflow for full logs and artifacts."
                .into(),
            annotations,
            url: run["html_url"].as_str().map(str::to_owned),
        });
    }
    let id = numeric_id(id, "runs:")?;
    let raw = github
        .api(&format!("repos/{}/check-runs/{id}", review.repository))
        .await?;
    if raw["head_sha"] != revision {
        return Err(invalid("This check belongs to another revision."));
    }
    let rows = array(
        github
            .api(&format!(
                "repos/{}/check-runs/{id}/annotations?per_page=100",
                review.repository
            ))
            .await?,
    )?;
    let mut annotations: Vec<_> = rows
        .iter()
        .map(|v| {
            format!(
                "{}:{} · {}\n{}",
                text(v, "path"),
                number(v, "start_line"),
                text(v, "annotation_level"),
                text(v, "message")
            )
        })
        .collect();
    if number(&raw["output"], "annotations_count") > rows.len() as u64 {
        annotations.push("More annotations are available on the provider page.".into());
    }
    Ok(ReviewProviderResult::CheckDetails {
        title: text(&raw["output"], "title"),
        summary: text(&raw["output"], "summary"),
        text: text(&raw["output"], "text"),
        annotations,
        url: raw["html_url"].as_str().map(str::to_owned),
    })
}

pub(super) async fn mutate(
    github: &GitHub<'_>,
    request: ReviewProviderWrite,
) -> Result<ReviewProviderResult, ReviewProviderError> {
    let review = request
        .review_ref()
        .ok_or_else(|| invalid("Choose a review."))?
        .clone();
    validate_ref(github.host, &review)?;
    let pr = format!("repos/{}/pulls/{}", review.repository, review.native_id);
    let message = match request {
        ReviewProviderWrite::AddLabels { labels, .. } => {
            metadata::validate_labels(&labels)?;
            github
                .request(
                    "POST",
                    &format!(
                        "repos/{}/issues/{}/labels",
                        review.repository, review.native_id
                    ),
                    Some(json!({"labels": labels})),
                )
                .await?;
            "Labels added."
        }
        ReviewProviderWrite::RemoveLabel { label, .. } => {
            metadata::validate_labels(std::slice::from_ref(&label))?;
            if label == "." || label == ".." {
                return Err(invalid("This label must be removed on GitHub."));
            }
            let encoded: String = url::form_urlencoded::byte_serialize(label.as_bytes()).collect();
            github
                .request(
                    "DELETE",
                    &format!(
                        "repos/{}/issues/{}/labels/{}",
                        review.repository,
                        review.native_id,
                        encoded.replace('+', "%20")
                    ),
                    None,
                )
                .await?;
            "Label removed."
        }
        ReviewProviderWrite::RemoveReviewers { reviewers, .. } => {
            metadata::validate_reviewers(&reviewers)?;
            github
                .request(
                    "DELETE",
                    &format!("{pr}/requested_reviewers"),
                    Some(metadata::reviewer_body(&reviewers)),
                )
                .await?;
            "Review requests removed."
        }
        ReviewProviderWrite::Comment { body, reply_to, .. } => {
            body_valid(&body)?;
            let endpoint = if let Some(id) = reply_to {
                format!("{pr}/comments/{}/replies", numeric_id(&id, "inline:")?)
            } else {
                format!(
                    "repos/{}/issues/{}/comments",
                    review.repository, review.native_id
                )
            };
            github
                .request("POST", &endpoint, Some(json!({"body":body})))
                .await?;
            "Comment posted."
        }
        ReviewProviderWrite::SubmitReview {
            revision,
            decision,
            body,
            comments,
            ..
        } => {
            github.require_revision(&review, &revision).await?;
            if body.len() > 60_000 || comments.len() > 100 {
                return Err(invalid("Review exceeds the submission limit."));
            }
            if !matches!(decision, ReviewDecision::Approve) {
                body_valid(&body)?;
            }
            let mut inline = Vec::new();
            for comment in comments {
                body_valid(&comment.body)?;
                if comment.line == 0 || comment.path.is_empty() || comment.path.len() > 4096 {
                    return Err(invalid("Choose a valid diff line."));
                }
                inline.push(json!({"path":comment.path,"line":comment.line,"side":match comment.side { ReviewLineSide::Old => "LEFT", ReviewLineSide::New => "RIGHT" },"body":comment.body}));
            }
            let event = match decision {
                ReviewDecision::Approve => "APPROVE",
                ReviewDecision::RequestChanges => "REQUEST_CHANGES",
                ReviewDecision::Comment => "COMMENT",
            };
            github
                .request(
                    "POST",
                    &format!("{pr}/reviews"),
                    Some(json!({"commit_id":revision,"event":event,"body":body,"comments":inline})),
                )
                .await?;
            "Review submitted."
        }
        ReviewProviderWrite::Edit { title, body, .. } => {
            if title.trim().is_empty() || title.len() > 1024 || body.len() > 60_000 {
                return Err(invalid(
                    "Enter a title and a description within the size limit.",
                ));
            }
            github
                .request("PATCH", &pr, Some(json!({"title":title,"body":body})))
                .await?;
            "Pull request updated."
        }
        ReviewProviderWrite::SetState { state, .. } => {
            match state {
                ReviewLifecycle::Close | ReviewLifecycle::Reopen => {
                    github.request("PATCH", &pr, Some(json!({"state":if matches!(state, ReviewLifecycle::Close) { "closed" } else { "open" }}))).await?;
                }
                ReviewLifecycle::Ready | ReviewLifecycle::Draft => {
                    let raw = github.api(&pr).await?;
                    let mutation = if matches!(state, ReviewLifecycle::Ready) {
                        "markPullRequestReadyForReview"
                    } else {
                        "convertPullRequestToDraft"
                    };
                    let query = format!(
                        "mutation($id:ID!) {{ {mutation}(input:{{pullRequestId:$id}}) {{ pullRequest {{ id }} }} }}"
                    );
                    let response = github
                        .request(
                            "POST",
                            "graphql",
                            Some(json!({"query":query,"variables":{"id":raw["node_id"]}})),
                        )
                        .await?;
                    if response["data"][mutation]["pullRequest"]["id"] != raw["node_id"]
                        || raw["node_id"].as_str().is_none()
                        || response["errors"]
                            .as_array()
                            .is_some_and(|errors| !errors.is_empty())
                    {
                        return Err(error(
                            ReviewProviderErrorCode::ProviderError,
                            "GitHub could not change the draft state. Check repository permissions and refresh.",
                        ));
                    }
                }
            }
            "Pull request state updated."
        }
        ReviewProviderWrite::RequestReviewers { reviewers, .. } => {
            metadata::validate_reviewers(&reviewers)?;
            github
                .request(
                    "POST",
                    &format!("{pr}/requested_reviewers"),
                    Some(metadata::reviewer_body(&reviewers)),
                )
                .await?;
            "Reviewers requested."
        }
        ReviewProviderWrite::Merge {
            revision, method, ..
        } => {
            github.require_revision(&review, &revision).await?;
            let method = match method {
                ReviewMergeMethod::Merge => "merge",
                ReviewMergeMethod::Squash => "squash",
                ReviewMergeMethod::Rebase => "rebase",
            };
            let result = github
                .request(
                    "PUT",
                    &format!("{pr}/merge"),
                    Some(json!({"sha":revision,"merge_method":method})),
                )
                .await?;
            if result["merged"] != true {
                return Err(error(
                    ReviewProviderErrorCode::ProviderError,
                    "GitHub did not merge this pull request. Check branch requirements and refresh.",
                ));
            }
            "Pull request merged."
        }
        ReviewProviderWrite::RerunCheck {
            revision,
            check_id,
            failed_only,
            ..
        } => {
            let id = numeric_id(&check_id, "workflows:")?;
            workflow(github, &review, &revision, id).await?;
            let action = if failed_only {
                "rerun-failed-jobs"
            } else {
                "rerun"
            };
            github
                .request(
                    "POST",
                    &format!("repos/{}/actions/runs/{id}/{action}", review.repository),
                    None,
                )
                .await?;
            "Workflow rerun requested."
        }
        ReviewProviderWrite::CancelCheck {
            revision, check_id, ..
        } => {
            let id = numeric_id(&check_id, "workflows:")?;
            workflow(github, &review, &revision, id).await?;
            github
                .request(
                    "POST",
                    &format!("repos/{}/actions/runs/{id}/cancel", review.repository),
                    None,
                )
                .await?;
            "Workflow cancellation requested."
        }
        ReviewProviderWrite::Publish { .. } => {
            return Err(invalid("Unexpected publication request."));
        }
    };
    Ok(ReviewProviderResult::Updated {
        message: message.into(),
    })
}
