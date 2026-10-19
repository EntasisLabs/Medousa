//! Hosted review publication; local Git pushes remain the caller's responsibility.
use super::*;

pub fn repository(host: &str, remote: &str) -> Result<String, ReviewProviderError> {
    if remote.lines().count() != 1 {
        return Err(invalid("Origin must have one SSH or HTTPS push URL."));
    }
    let normalized = if let Some(scp) = remote.strip_prefix("git@") {
        format!("ssh://git@{}", scp.replacen(':', "/", 1))
    } else {
        remote.to_owned()
    };
    let url = Url::parse(&normalized).map_err(|_| invalid("Origin must use SSH or HTTPS."))?;
    if !matches!(url.scheme(), "ssh" | "https")
        || url.host_str() != Some(host)
        || url.query().is_some()
        || url.fragment().is_some()
        || url.password().is_some()
        || (url.scheme() == "https" && !url.username().is_empty())
    {
        return Err(invalid(
            "Origin must use the selected GitHub host over SSH or HTTPS.",
        ));
    }
    let repository = url.path().trim_matches('/').trim_end_matches(".git");
    validate_repository(repository)?;
    Ok(repository.into())
}

pub async fn invoke(
    input: ReviewProviderWriteInvocation,
) -> Result<ReviewProviderResult, ReviewProviderError> {
    if input.protocol_version != REVIEW_PROVIDER_PROTOCOL_VERSION
        || input.mutation.provider_id != "github"
    {
        return Err(invalid("Unsupported provider protocol."));
    }
    validate_host(&input.mutation.host)?;
    let token = credential_token(input.credential.as_deref())?;
    let program = locate_cli().await?;
    let github = GitHub {
        program,
        host: &input.mutation.host,
        credential: token.as_deref(),
    };
    let ReviewProviderWrite::Publish {
        repository,
        head,
        base,
        title,
        body,
        draft,
        update_existing,
    } = input.mutation.request
    else {
        return super::manage::mutate(&github, input.mutation.request).await;
    };
    validate_repository(&repository)?;
    if title.trim().is_empty()
        || title.len() > 1024
        || body.len() > 60_000
        || head.is_empty()
        || base.is_empty()
        || head == base
        || head.len() > 1024
        || base.len() > 1024
    {
        return Err(invalid(
            "Choose different head and base branches and enter a title.",
        ));
    }
    let mut endpoint = Url::parse("https://placeholder.invalid/").expect("static URL");
    endpoint.set_path(&format!("repos/{repository}/pulls"));
    endpoint
        .query_pairs_mut()
        .append_pair("state", "open")
        .append_pair(
            "head",
            &format!(
                "{}:{head}",
                repository.split('/').next().unwrap_or_default()
            ),
        )
        .append_pair("base", &base)
        .append_pair("per_page", "100");
    let existing = array(
        github
            .api(&format!(
                "{}?{}",
                endpoint.path().trim_start_matches('/'),
                endpoint.query().unwrap_or_default()
            ))
            .await?,
    )?;
    let (raw, existing) = if let Some(existing) = existing.first() {
        if update_existing {
            let id = number(existing, "number");
            if id == 0 {
                return Err(invalid("GitHub returned an invalid review identity."));
            }
            (
                github
                    .request(
                        "PATCH",
                        &format!("repos/{repository}/pulls/{id}"),
                        Some(serde_json::json!({"title":title,"body":body})),
                    )
                    .await?,
                true,
            )
        } else {
            (existing.clone(), true)
        }
    } else {
        let result = github.request("POST", &format!("repos/{repository}/pulls"), Some(serde_json::json!({"head":head,"base":base,"title":title,"body":body,"draft":draft}))).await;
        // No blind retry of POST: a lost response may have created the review.
        (
            result.map_err(|cause| {
                error(
                    cause.code,
                    format!(
                        "{} Retry will look for an existing review first.",
                        cause.message
                    ),
                )
            })?,
            false,
        )
    };
    if number(&raw, "number") == 0 || text(&raw, "html_url").is_empty() {
        return Err(error(
            ReviewProviderErrorCode::ProviderError,
            "GitHub did not confirm the review identity. Retry to look it up.",
        ));
    }
    Ok(ReviewProviderResult::Published {
        review: Box::new(project_review(github.host, &repository, &raw)),
        existing,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn remote_identity_uses_the_push_host_and_rejects_multiple_destinations() {
        assert_eq!(
            repository("code.example.com", "git@code.example.com:team/repo.git").unwrap(),
            "team/repo"
        );
        assert!(repository("github.com", "git@gitlab.com:team/repo.git").is_err());
        assert!(
            repository(
                "github.com",
                "https://github.com/team/repo.git\nhttps://github.com/other/repo"
            )
            .is_err()
        );
        assert!(repository("github.com", "https://github.com/team/../repo").is_err());
    }
}
