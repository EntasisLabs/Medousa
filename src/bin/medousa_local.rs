//! Standalone local inference server (`medousa_local`).
//!
//! OpenAI-compatible loopback server on `:7421` with model download/load handled
//! via the main daemon's `/v1/local/*` APIs. Built with `embedded-inference*`
//! features (Metal / CUDA / CPU).

use std::env;

use crate::local_inference::{
    builtin_catalog, compiled_backends, config_from_catalog_entry, load_recommended_engine,
    LocalEngineConfig, LOCAL_ENGINE, DEFAULT_LOCAL_ENGINE_BIND,
};

#[cfg(not(feature = "embedded-inference"))]
fn main() {
    eprintln!(
        "medousa_local requires embedded-inference at build time.\n\
         Rebuild with: cargo build -p medousa --bin medousa_local --features embedded-inference-metal"
    );
    std::process::exit(1);
}

#[cfg(feature = "embedded-inference")]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--print-backends") {
        for backend in compiled_backends() {
            println!("{backend}");
        }
        return Ok(());
    }

    let bind = flag_value(&args, "--bind")
        .unwrap_or_else(|| DEFAULT_LOCAL_ENGINE_BIND.to_string());
    let load_recommended = args.iter().any(|arg| arg == "--load-recommended");
    let model_id = flag_value(&args, "--model-id");
    let model_repo = flag_value(&args, "--model-repo");
    let model_alias = flag_value(&args, "--model-alias");
    let from_uqff = flag_value(&args, "--from-uqff");
    let in_situ_quant = flag_value(&args, "--in-situ-quant");

    let status = if load_recommended || (model_id.is_none() && model_repo.is_none()) {
        eprintln!("medousa_local loading tier-recommended model (this may take several minutes)…");
        load_recommended_engine(Some(bind)).await?
    } else if let (Some(repo), Some(alias)) = (model_repo, model_alias) {
        let config = LocalEngineConfig {
            bind: bind.clone(),
            model_repo: repo,
            model_alias: alias,
            from_uqff,
            in_situ_quant,
            cpu_only: env::var("MEDOUSA_LOCAL_ENGINE_CPU")
                .ok()
                .is_some_and(|value| matches!(value.trim(), "1" | "true" | "yes")),
        };
        LOCAL_ENGINE.as_ref().load(config).await?
    } else if let Some(model_id) = model_id {
        let catalog = builtin_catalog();
        let entry = catalog
            .models
            .iter()
            .find(|entry| entry.id == model_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknown catalog model id: {model_id}"))?;
        let config = config_from_catalog_entry(&entry, Some(bind));
        LOCAL_ENGINE.as_ref().load(config).await?
    } else {
        anyhow::bail!("provide --load-recommended, --model-id, or --model-repo + --model-alias");
    };

    println!(
        "medousa_local ready at {} ({})",
        status.base_url,
        status
            .model_alias
            .as_deref()
            .unwrap_or("gemma")
    );

    tokio::signal::ctrl_c().await?;
    LOCAL_ENGINE.as_ref().unload().await?;
    println!("medousa_local stopped");
    Ok(())
}

#[cfg(feature = "embedded-inference")]
fn flag_value(args: &[String], key: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == key)
        .and_then(|index| args.get(index + 1))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(feature = "embedded-inference")]
fn print_help() {
    println!(
        r#"medousa_local — Medousa offline brain (OpenAI-compatible :7421)

usage:
  medousa_local [options]

options:
  --bind <host:port>         Bind address (default: 127.0.0.1:7421)
  --load-recommended         Load tier-recommended Gemma model
  --model-id <id>            Load catalog model by id (must be installed)
  --model-repo <repo>        HuggingFace repo (with --model-alias)
  --model-alias <alias>      Model alias (with --model-repo)
  --from-uqff <file>         Load from UQFF file in model dir
  --in-situ-quant <level>     In-situ quant level (default: 4)
  --print-backends           Print compiled inference backends and exit
  -h, --help                 Show this help

environment:
  MEDOUSA_DATA_DIR           Model weights directory
  MEDOUSA_LOCAL_ENGINE_CPU=1 Force CPU inference
"#
    );
}
