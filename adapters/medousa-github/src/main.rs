use medousa_types::review_provider::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let output = if args.as_slice() == ["--describe"] {
        serde_json::to_vec(&medousa_github::descriptor())?
    } else if matches!(args.as_slice(), [arg] if matches!(arg.as_str(), "--invoke" | "--mutate" | "--auth"))
    {
        let mut bytes = vec![];
        tokio::io::stdin()
            .take(256 * 1024 + 1)
            .read_to_end(&mut bytes)
            .await?;
        if args[0] == "--auth" {
            let result = match serde_json::from_slice::<ReviewProviderAuthInvocation>(&bytes) {
                Ok(input) if bytes.len() <= 256 * 1024 => medousa_github::auth::invoke(input).await,
                _ => Err(ReviewProviderError {
                    code: ReviewProviderErrorCode::InvalidParameter,
                    message: "Invalid sign-in request.".into(),
                }),
            };
            let output = serde_json::to_vec(&ReviewProviderAuthPackageResponse {
                protocol_version: REVIEW_PROVIDER_PROTOCOL_VERSION,
                result,
            })?;
            let mut stdout = tokio::io::stdout();
            stdout.write_all(&output).await?;
            stdout.flush().await?;
            return Ok(());
        }
        let result = if bytes.len() > 256 * 1024 {
            Err(ReviewProviderError {
                code: ReviewProviderErrorCode::InvalidParameter,
                message: "Provider input exceeded the size limit.".into(),
            })
        } else {
            if args[0] == "--mutate" {
                match serde_json::from_slice::<ReviewProviderWriteInvocation>(&bytes) {
                    Ok(input) => medousa_github::invoke_write(input).await,
                    Err(_) => Err(ReviewProviderError {
                        code: ReviewProviderErrorCode::InvalidParameter,
                        message: "Invalid review write request.".into(),
                    }),
                }
            } else {
                match serde_json::from_slice::<ReviewProviderInvocation>(&bytes) {
                    Ok(input) => medousa_github::invoke(input).await,
                    Err(_) => Err(ReviewProviderError {
                        code: ReviewProviderErrorCode::InvalidParameter,
                        message: "Invalid review-provider request.".into(),
                    }),
                }
            }
        };
        serde_json::to_vec(&ReviewProviderResponse::from_result(result))?
    } else {
        eprintln!(
            "medousa_review_github --describe | --invoke | --mutate | --auth\nGitHub review package. Sign in through Medousa Settings → Accounts. Existing workshop CLI accounts are also supported."
        );
        return Ok(());
    };
    let mut stdout = tokio::io::stdout();
    stdout.write_all(&output).await?;
    stdout.flush().await?;
    Ok(())
}
