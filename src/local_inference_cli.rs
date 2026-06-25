use std::env;
use std::net::{TcpStream, ToSocketAddrs};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use reqwest::blocking::Client;
use serde_json::json;

use crate::daemon_api::resolve_daemon_url;
use crate::local_inference::DEFAULT_LOCAL_ENGINE_BIND;
use crate::DEFAULT_MEDOUSA_LOCAL_BASE_URL;
use crate::local_inference_handlers::{
    LocalCatalogResponse, LocalHardwareResponse, LocalModelDownloadRequest,
    LocalModelDownloadResponse, LocalModelsResponse,
};
use crate::local_inference::LocalEngineStatus;

const HTTP_TIMEOUT: Duration = Duration::from_secs(30);

pub fn resolve_models_daemon_url(args: &[String]) -> String {
    let explicit = args
        .iter()
        .position(|arg| arg == "--daemon-url")
        .and_then(|index| args.get(index + 1))
        .map(String::as_str);
    resolve_daemon_url(explicit)
}

fn local_http_client() -> Result<Client> {
    Client::builder()
        .timeout(HTTP_TIMEOUT)
        .build()
        .context("build HTTP client")
}

fn is_bind_reachable(bind: &str) -> bool {
    if let Ok(mut addrs) = bind.to_socket_addrs()
        && let Some(addr) = addrs.next()
    {
        return TcpStream::connect_timeout(&addr, Duration::from_millis(250)).is_ok();
    }
    false
}

fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let value = bytes as f64;
    if value >= GB {
        format!("{:.1} GB", value / GB)
    } else if value >= MB {
        format!("{:.1} MB", value / MB)
    } else if value >= KB {
        format!("{:.1} KB", value / KB)
    } else {
        format!("{bytes} B")
    }
}

pub fn fetch_local_hardware(daemon_url: &str) -> Result<LocalHardwareResponse> {
    let client = local_http_client()?;
    let response = client
        .get(format!("{daemon_url}/v1/local/hardware"))
        .send()
        .context("GET /v1/local/hardware")?;
    if !response.status().is_success() {
        bail!(
            "GET /v1/local/hardware returned {} — is Medousa Engine running?",
            response.status()
        );
    }
    response
        .json()
        .context("parse /v1/local/hardware json")
}

pub fn fetch_local_catalog(daemon_url: &str) -> Result<LocalCatalogResponse> {
    let client = local_http_client()?;
    let response = client
        .get(format!("{daemon_url}/v1/local/catalog"))
        .send()
        .context("GET /v1/local/catalog")?;
    if !response.status().is_success() {
        bail!("GET /v1/local/catalog returned {}", response.status());
    }
    response.json().context("parse /v1/local/catalog json")
}

pub fn fetch_local_models(daemon_url: &str) -> Result<LocalModelsResponse> {
    let client = local_http_client()?;
    let response = client
        .get(format!("{daemon_url}/v1/local/models"))
        .send()
        .context("GET /v1/local/models")?;
    if !response.status().is_success() {
        bail!("GET /v1/local/models returned {}", response.status());
    }
    response.json().context("parse /v1/local/models json")
}

pub fn fetch_local_engine_status(daemon_url: &str) -> Result<LocalEngineStatus> {
    let client = local_http_client()?;
    let response = client
        .get(format!("{daemon_url}/v1/local/engine/status"))
        .send()
        .context("GET /v1/local/engine/status")?;
    if !response.status().is_success() {
        bail!("GET /v1/local/engine/status returned {}", response.status());
    }
    response
        .json()
        .context("parse /v1/local/engine/status json")
}

pub fn post_local_model_download(
    daemon_url: &str,
    model_id: &str,
) -> Result<LocalModelDownloadResponse> {
    let client = local_http_client()?;
    let response = client
        .post(format!("{daemon_url}/v1/local/models/download"))
        .json(&LocalModelDownloadRequest {
            model_id: model_id.to_string(),
        })
        .send()
        .context("POST /v1/local/models/download")?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        bail!("POST /v1/local/models/download returned {status}: {body}");
    }
    response
        .json()
        .context("parse /v1/local/models/download json")
}

pub fn fetch_download_progress(
    daemon_url: &str,
    job_id: &str,
) -> Result<crate::local_inference::ModelDownloadProgress> {
    let client = local_http_client()?;
    let response = client
        .get(format!("{daemon_url}/v1/local/models/download/{job_id}"))
        .send()
        .context("GET download progress")?;
    if !response.status().is_success() {
        bail!("GET download progress returned {}", response.status());
    }
    response.json().context("parse download progress json")
}

pub fn delete_local_model(daemon_url: &str, model_id: &str) -> Result<()> {
    let client = local_http_client()?;
    let response = client
        .delete(format!("{daemon_url}/v1/local/models/{model_id}"))
        .send()
        .context("DELETE /v1/local/models/{model_id}")?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        bail!("DELETE /v1/local/models/{model_id} returned {status}: {body}");
    }
    Ok(())
}

pub fn post_local_engine_load(
    _daemon_url: &str,
    model_id: Option<&str>,
) -> Result<LocalEngineStatus> {
    let runtime = tokio::runtime::Runtime::new().context("build tokio runtime")?;
    runtime
        .block_on(medousa_host::spawn_and_wait(
            None,
            model_id.map(str::to_string),
        ))
        .map_err(anyhow::Error::msg)
}

pub fn print_models_help() {
    println!("Private brain tools (power users only).");
    println!();
    println!("Everyone else: open Medousa → welcome wizard → chat.");
    println!();
    println!("USAGE:");
    println!("  medousa models probe [--daemon-url <url>]");
    println!("  medousa models catalog [--daemon-url <url>]");
    println!("  medousa models list [--daemon-url <url>]");
    println!("  medousa models download <model-id> [--wait] [--daemon-url <url>]");
    println!("  medousa models remove <model-id> [--daemon-url <url>]");
    println!("  medousa models engine-status [--daemon-url <url>]");
    println!("  medousa models engine-load [--model <model-id>] [--daemon-url <url>]");
    println!();
    println!("EXAMPLES:");
    println!("  medousa models probe");
    println!("  medousa models download gemma-4-e4b-it --wait");
    println!("  medousa models engine-load --model gemma-4-e4b-it");
}

pub fn run_models_command(args: &[String]) -> Result<()> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_models_help();
        return Ok(());
    }

    let daemon_url = resolve_models_daemon_url(args);
    match args.first().map(String::as_str) {
        None => {
            print_models_help();
            Ok(())
        }
        Some("probe") => run_models_probe(&daemon_url),
        Some("catalog") => run_models_catalog(&daemon_url),
        Some("list") => run_models_list(&daemon_url),
        Some("download") => run_models_download(&daemon_url, args),
        Some("remove") => run_models_remove(&daemon_url, args),
        Some("engine-status") | Some("status") => run_models_engine_status(&daemon_url),
        Some("engine-load") | Some("load") => run_models_engine_load(&daemon_url, args),
        Some(other) => bail!(
            "unknown models subcommand '{other}'. run 'medousa models --help' for usage"
        ),
    }
}

fn run_models_probe(daemon_url: &str) -> Result<()> {
    let hardware = fetch_local_hardware(daemon_url)?;
    let profile = &hardware.profile;
    println!("{}", hardware.message);
    println!(
        "tier={} label={} ram_gb={:.1} recommended={}",
        profile.tier.as_str(),
        profile.tier_label,
        profile.probe.total_ram_mb as f64 / 1024.0,
        profile.recommended_display_name
    );
    println!(
        "recommended_model_id={} engine_available={}",
        profile.recommended_model_id, hardware.engine_available
    );
    if !hardware.engine_available {
        println!(
            "[hint] This engine build cannot load a private brain — install Medousa from a release build."
        );
    }
    Ok(())
}

fn run_models_catalog(daemon_url: &str) -> Result<()> {
    let catalog = fetch_local_catalog(daemon_url)?;
    println!(
        "tier={} ({}) family={} recommended={}",
        catalog.tier.as_str(),
        catalog.tier_label,
        catalog.family_default,
        catalog.recommended_model_id
    );
    println!("ID\tDISPLAY\tSIZE\tTIER");
    for model in &catalog.models {
        println!(
            "{}\t{}\t{}\t{}",
            model.id,
            model.display_name,
            format_bytes(model.size_bytes),
            model.tier_min
        );
    }
    Ok(())
}

fn run_models_list(daemon_url: &str) -> Result<()> {
    let models = fetch_local_models(daemon_url)?;
    if models.installed.is_empty() {
        println!("No local models installed.");
    } else {
        println!("INSTALLED");
        for model in &models.installed {
            println!(
                "{}  {}  {}  verified={}",
                model.model_id,
                model.repo,
                format_bytes(model.bytes_on_disk),
                model.verified
            );
        }
    }
    if !models.active_downloads.is_empty() {
        println!();
        println!("ACTIVE DOWNLOADS");
        for job in &models.active_downloads {
            println!(
                "{}  {}  {}  {:.0}%  {}",
                job.job_id,
                job.model_id,
                job.phase,
                job.percent,
                job.message
            );
        }
    }
    Ok(())
}

fn run_models_download(daemon_url: &str, args: &[String]) -> Result<()> {
    let model_id = args
        .get(1)
        .filter(|value| !value.starts_with("--"))
        .map(String::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing model id: medousa models download <model-id>"))?;
    let wait = args.iter().any(|arg| arg == "--wait");

    if env::var("HF_TOKEN")
        .ok()
        .is_none_or(|value| value.trim().is_empty())
    {
        println!(
            "[hint] HF_TOKEN is not set — Gemma downloads may fail if Hugging Face requires authentication."
        );
    }

    let response = post_local_model_download(daemon_url, model_id)?;
    let job = &response.job;
    println!(
        "download started job={} model={} phase={}",
        job.job_id, job.model_id, job.phase
    );

    if !wait {
        println!("poll: medousa models list  (or add --wait)");
        return Ok(());
    }

    let started = std::time::Instant::now();
    loop {
        let progress = fetch_download_progress(daemon_url, &job.job_id)?;
        if progress.bytes_total > 0 {
            println!(
                "{:.0}%  {} / {}  {}",
                progress.percent,
                format_bytes(progress.bytes_done),
                format_bytes(progress.bytes_total),
                progress.message
            );
        } else {
            println!("{}  {}", progress.phase, progress.message);
        }
        if progress.phase == "ready" {
            println!("[ok] Download complete: {}", progress.model_id);
            println!(
                "next: medousa models engine-load --model {}",
                progress.model_id
            );
            return Ok(());
        }
        if progress.phase == "failed" {
            bail!(
                progress
                    .error
                    .unwrap_or_else(|| progress.message.clone())
            );
        }
        if started.elapsed() > Duration::from_secs(60 * 60) {
            bail!("download timed out after 1 hour (job still running — check medousa models list)");
        }
        thread::sleep(Duration::from_millis(500));
    }
}

fn run_models_remove(daemon_url: &str, args: &[String]) -> Result<()> {
    let model_id = args
        .get(1)
        .filter(|value| !value.starts_with("--"))
        .map(String::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing model id: medousa models remove <model-id>"))?;
    delete_local_model(daemon_url, model_id)?;
    println!("[ok] Removed {model_id}");
    Ok(())
}

fn run_models_engine_status(daemon_url: &str) -> Result<()> {
    let status = fetch_local_engine_status(daemon_url)?;
    print_engine_status(&status);
    Ok(())
}

fn run_models_engine_load(daemon_url: &str, args: &[String]) -> Result<()> {
    let model_id = find_flag_value(args, "--model");
    let status = post_local_engine_load(daemon_url, model_id)?;
    print_engine_status(&status);
    Ok(())
}

fn print_engine_status(status: &LocalEngineStatus) {
    println!(
        "feature_enabled={} loaded={} base_url={}",
        status.feature_enabled, status.loaded, status.base_url
    );
    if let Some(bind) = status.bind.as_deref() {
        println!("bind={bind}");
    }
    if let Some(repo) = status.model_repo.as_deref() {
        println!("model_repo={repo}");
    }
    if let Some(alias) = status.model_alias.as_deref() {
        println!("model_alias={alias}");
    }
    println!("message={}", status.message);
}

fn find_flag_value<'a>(args: &'a [String], key: &str) -> Option<&'a str> {
    args.iter()
        .position(|arg| arg == key)
        .and_then(|index| args.get(index + 1))
        .map(String::as_str)
}

pub fn print_doctor_local_inference(daemon_url: &str, daemon_healthy: bool, verbose: bool) {
    println!();
    println!("--- private brain ---");
    if !daemon_healthy {
        println!("status=engine is not running");
        println!("→ Open Medousa (that's the whole setup for most people)");
        return;
    }

    match fetch_local_hardware(daemon_url) {
        Ok(hardware) => {
            let profile = &hardware.profile;
            println!(
                "hardware_tier={} ({}) recommended={}",
                profile.tier.as_str(),
                profile.tier_label,
                profile.recommended_model_id
            );
            println!(
                "embedded_inference={} ram_gb={:.1}",
                if hardware.engine_available {
                    "available"
                } else if crate::local_inference::medousa_local_binary_available() {
                    "package_not_running"
                } else {
                    "not_installed"
                },
                profile.probe.total_ram_mb as f64 / 1024.0
            );
        }
        Err(error) => {
            println!("hardware_probe=failed ({error:#})");
        }
    }

    match fetch_local_models(daemon_url) {
        Ok(models) => {
            if models.installed.is_empty() {
                println!("installed_models=(none)");
            } else {
                let ids = models
                    .installed
                    .iter()
                    .map(|model| model.model_id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                println!("installed_models={ids}");
            }
            if !models.active_downloads.is_empty() {
                for job in &models.active_downloads {
                    println!(
                        "download_active={} model={} phase={} {:.0}%",
                        job.job_id, job.model_id, job.phase, job.percent
                    );
                }
            }
        }
        Err(error) => {
            println!("installed_models=unknown ({error:#})");
        }
    }

    match fetch_local_engine_status(daemon_url) {
        Ok(status) => {
            println!(
                "local_engine loaded={} base_url={} message={}",
                status.loaded, status.base_url, status.message
            );
        }
        Err(error) => {
            println!("local_engine=unknown ({error:#})");
        }
    }

    let engine_bind = env::var("MEDOUSA_LOCAL_ENGINE_BIND")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_LOCAL_ENGINE_BIND.to_string());
    let engine_reachable = is_bind_reachable(&engine_bind);
    println!(
        "local_engine_bind={} tcp_reachable={}",
        engine_bind, engine_reachable
    );

    let hf_token = env::var("HF_TOKEN")
        .ok()
        .is_some_and(|value| !value.trim().is_empty());
    println!(
        "hf_token={}",
        if hf_token {
            "configured"
        } else {
            "missing (may be required for Gemma download)"
        }
    );

    if verbose {
        if let Ok(catalog) = fetch_local_catalog(daemon_url) {
            println!(
                "catalog_models={} recommended={}",
                catalog.models.len(),
                catalog.recommended_model_id
            );
        }
        println!("provider_hint=medousa-local base_url={DEFAULT_MEDOUSA_LOCAL_BASE_URL}");
    }

    if !verbose {
        println!("→ Most people: Medousa handles download, load, and chat");
        println!("→ Troubleshooting: medousa doctor --local-engine  |  medousa models --help");
    } else {
        println!("→ dev offline path: medousa start daemon --inference");
        println!("→ then: medousa models download <id> --wait && medousa models engine-load --model <id>");
    }
}

pub fn print_home_app_hint() {
    println!("[info] Ready to chat? Open Medousa.");
}
