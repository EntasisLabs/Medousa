//! Standalone local inference server (`medousa_local`).
//!
//! OpenAI-compatible loopback server on `:7421`. Model download/catalog is
//! handled via the daemon's `/v1/local/*` APIs; this binary only runs mistralrs.

#[cfg(feature = "embedded-inference")]
use std::env;
#[cfg(feature = "embedded-inference")]
use std::sync::Arc;

#[cfg(feature = "embedded-inference")]
use once_cell::sync::Lazy;

#[cfg(feature = "embedded-inference")]
use medousa::local_inference::{
    builtin_catalog, compiled_backends, config_from_catalog_entry, recommended_engine_config,
    DEFAULT_LOCAL_ENGINE_BIND,
};
#[cfg(feature = "embedded-inference")]
use medousa_local_engine::{LocalEngineConfig as EngineConfig, LocalEngineRuntime};

#[cfg(feature = "embedded-inference")]
static RUNTIME: Lazy<Arc<LocalEngineRuntime>> =
    Lazy::new(|| Arc::new(LocalEngineRuntime::new()));

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

    let medousa_config = if load_recommended || (model_id.is_none() && model_repo.is_none()) {
        eprintln!("medousa_local loading tier-recommended model (this may take several minutes)…");
        recommended_engine_config(Some(bind.clone())).map_err(anyhow::Error::msg)?
    } else if let (Some(repo), Some(alias)) = (model_repo, model_alias) {
        medousa::local_inference::LocalEngineConfig {
            bind: bind.clone(),
            model_repo: repo,
            model_alias: alias,
            from_uqff,
            in_situ_quant,
            cpu_only: env::var("MEDOUSA_LOCAL_ENGINE_CPU")
                .ok()
                .is_some_and(|value| matches!(value.trim(), "1" | "true" | "yes")),
        }
    } else if let Some(model_id) = model_id {
        let catalog = builtin_catalog();
        let entry = catalog
            .models
            .iter()
            .find(|entry| entry.id == model_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknown catalog model id: {model_id}"))?;
        config_from_catalog_entry(&entry, Some(bind.clone()))
    } else {
        anyhow::bail!("provide --load-recommended, --model-id, or --model-repo + --model-alias");
    };

    let status = RUNTIME
        .load(to_engine_config(medousa_config))
        .await
        .map_err(anyhow::Error::msg)?;

    println!(
        "medousa_local ready at {} ({})",
        status.base_url,
        status
            .model_alias
            .as_deref()
            .unwrap_or("gemma")
    );

    tokio::signal::ctrl_c().await?;
    RUNTIME.unload().await.map_err(anyhow::Error::msg)?;
    println!("medousa_local stopped");
    Ok(())
}

#[cfg(feature = "embedded-inference")]
fn to_engine_config(config: medousa::local_inference::LocalEngineConfig) -> EngineConfig {
    EngineConfig {
        bind: config.bind,
        model_repo: config.model_repo,
        model_alias: config.model_alias,
        from_uqff: config.from_uqff,
        in_situ_quant: config.in_situ_quant,
        cpu_only: config.cpu_only,
    }
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
  --in-situ-quant <level>    In-situ quant level (default: 4)
  --print-backends           Print compiled inference backends and exit
  -h, --help                 Show this help

environment:
  MEDOUSA_DATA_DIR           Model weights directory
  MEDOUSA_LOCAL_ENGINE_CPU=1 Force CPU inference
"#
    );
}
