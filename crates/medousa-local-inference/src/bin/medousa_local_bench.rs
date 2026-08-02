//! Content-free lifecycle benchmark for the current embedded inference engine.

use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context;
use chrono::Utc;
use medousa_local_engine::{LocalEngineConfig as RuntimeConfig, LocalEngineRuntime};
use medousa_local_inference::{
    LocalEngineConfig, acquire_activation_lease, admission_for_model_id, builtin_catalog,
    collect_device_telemetry, compiled_backends, config_from_catalog_entry,
    local_repo_if_installed, probe_hardware, record_benchmark_calibration,
};
use medousa_types::{
    LocalBenchmarkArtifactMode, LocalBenchmarkEngineIdentity, LocalBenchmarkGitState,
    LocalBenchmarkHostIdentity, LocalBenchmarkManifest, LocalBenchmarkMemorySample,
    LocalBenchmarkOutcome, LocalBenchmarkPhase, LocalBenchmarkRecipe, LocalBenchmarkResult,
};
use sysinfo::{Pid, ProcessesToUpdate, System};

const SCHEMA_VERSION: u32 = 1;
const DEFAULT_BIND: &str = "127.0.0.1:7422";
const DEFAULT_PROMPT_TOKENS: usize = 64;
const DEFAULT_OUTPUT_TOKENS: usize = 64;
const SAMPLING_SEED: u64 = 42;
const BASELINE_RUNTIME_NAME: &str = "mistral.rs";
const BASELINE_RUNTIME_VERSION: &str = "0.8.1";

#[derive(Debug)]
struct Args {
    model_id: String,
    bind: String,
    prompt_tokens: usize,
    output_tokens: usize,
    max_seq_len: Option<usize>,
    max_batch_size: Option<usize>,
    output: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = parse_args(std::env::args().skip(1).collect())?;
    let started_at = Utc::now();
    let clock = Instant::now();
    let hardware = probe_hardware();
    let admission = admission_for_model_id(&args.model_id).map_err(anyhow::Error::msg)?;
    anyhow::ensure!(admission.admitted, admission.rationale.clone());
    let activation_lease = acquire_activation_lease(&admission).map_err(anyhow::Error::msg)?;

    let entry = builtin_catalog()
        .models
        .into_iter()
        .find(|entry| entry.id == args.model_id)
        .with_context(|| format!("unknown catalog model id: {}", args.model_id))?;
    anyhow::ensure!(
        local_repo_if_installed(&entry.id).is_some(),
        "model {} is not installed; install it from Settings -> Packages before benchmarking",
        entry.id
    );

    let mut config = config_from_catalog_entry(&entry, Some(args.bind.clone()));
    if let Some(max_seq_len) = args.max_seq_len {
        anyhow::ensure!(
            max_seq_len <= config.max_seq_len,
            "--context {max_seq_len} exceeds the catalog recipe cap ({})",
            config.max_seq_len
        );
        config.max_seq_len = max_seq_len;
    }
    if let Some(max_batch_size) = args.max_batch_size {
        anyhow::ensure!(
            max_batch_size <= config.max_batch_size,
            "--batch {max_batch_size} exceeds the catalog recipe cap ({})",
            config.max_batch_size
        );
        config.max_batch_size = max_batch_size;
    }
    let requested_tokens = args
        .prompt_tokens
        .checked_add(args.output_tokens)
        .context("prompt and output token limits overflowed")?;
    anyhow::ensure!(
        requested_tokens <= config.max_seq_len,
        "synthetic prompt plus output limit ({requested_tokens}) exceeds recipe context ({})",
        config.max_seq_len
    );
    // Benchmark runs own their lifecycle; normal idle eviction would add noise
    // while sampling the post-request steady state.
    config.idle_timeout_secs = 0;
    let recipe = recipe(&config, &entry.repo, &args);
    let runtime = Arc::new(LocalEngineRuntime::new());
    let mut sampler = Sampler::new(clock);
    let mut result = empty_result();
    sampler.capture(LocalBenchmarkPhase::BeforeLoad);

    let load_started = Instant::now();
    let worker = medousa_local_inference::worker_status_for_config(&config).await;
    anyhow::ensure!(
        worker.artifact_digest.is_some(),
        "benchmark requires a verified installed artifact with file digests"
    );
    anyhow::ensure!(
        worker.binary_digest.is_some(),
        "benchmark executable digest could not be read"
    );
    let artifact_digest = worker.artifact_digest.clone();
    let binary_digest = worker.binary_digest.clone();
    let recipe_revision = Some(worker.recipe_revision.clone());
    let run = async {
        runtime
            .load(to_runtime_config(config.clone(), worker))
            .await
            .map_err(anyhow::Error::msg)?;
        drop(activation_lease);
        result.load_ms = Some(elapsed_ms(load_started));
        sampler.capture(LocalBenchmarkPhase::AfterLoad);

        run_synthetic_stream(&args, &mut result).await?;
        sampler.capture(LocalBenchmarkPhase::AfterStream);
        anyhow::Ok(())
    }
    .await;

    let unload_started = Instant::now();
    let unload_result = runtime.unload().await.map_err(anyhow::Error::msg);
    result.unload_ms = Some(elapsed_ms(unload_started));
    sampler.capture(LocalBenchmarkPhase::AfterUnload);
    capture_reclaim_samples(&mut sampler, &mut result).await;

    let final_error = run.err().or_else(|| unload_result.err());
    result.outcome = if final_error.is_some() {
        LocalBenchmarkOutcome::Failed
    } else {
        LocalBenchmarkOutcome::Completed
    };
    result.error = final_error.as_ref().map(ToString::to_string);

    let manifest = LocalBenchmarkManifest {
        schema_version: SCHEMA_VERSION,
        started_at,
        finished_at: Utc::now(),
        git: git_state(),
        engine: LocalBenchmarkEngineIdentity {
            control_plane_version: env!("CARGO_PKG_VERSION").to_string(),
            runtime_name: BASELINE_RUNTIME_NAME.to_string(),
            runtime_version: BASELINE_RUNTIME_VERSION.to_string(),
            compiled_backends: compiled_backends()
                .into_iter()
                .map(str::to_string)
                .collect(),
            artifact_digest,
            binary_digest,
            recipe_revision,
        },
        host: host_identity(),
        hardware,
        admission,
        recipe,
        samples: sampler.samples,
        result,
    };
    write_manifest(&manifest, args.output.as_ref())?;
    record_benchmark_calibration(&manifest).map_err(anyhow::Error::msg)?;

    if let Some(error) = final_error {
        anyhow::bail!(error);
    }
    Ok(())
}

async fn run_synthetic_stream(
    args: &Args,
    result: &mut LocalBenchmarkResult,
) -> anyhow::Result<()> {
    let prompt = synthetic_prompt(args.prompt_tokens);
    let request_started = Instant::now();
    let mut response = reqwest::Client::new()
        .post(format!("http://{}/v1/chat/completions", args.bind))
        .json(&serde_json::json!({
            "model": args.model_id,
            "messages": [{"role": "user", "content": prompt}],
            "max_tokens": args.output_tokens,
            "temperature": 0,
            "seed": SAMPLING_SEED,
            "stream_options": {"include_usage": true},
            "stream": true
        }))
        .send()
        .await
        .context("benchmark request failed")?
        .error_for_status()
        .context("benchmark engine returned an error")?;

    let mut sse_buffer = Vec::new();
    while let Some(chunk) = response.chunk().await.context("reading benchmark stream")? {
        if chunk.is_empty() {
            continue;
        }
        if observe_sse_chunk(&mut sse_buffer, &chunk, result) {
            result.ttft_ms.get_or_insert(elapsed_ms(request_started));
        }
        result.response_chunks += 1;
        result.response_bytes += chunk.len() as u64;
    }
    result.stream_ms = Some(elapsed_ms(request_started));
    Ok(())
}

fn observe_sse_chunk(
    buffer: &mut Vec<u8>,
    chunk: &[u8],
    result: &mut LocalBenchmarkResult,
) -> bool {
    buffer.extend_from_slice(chunk);
    let mut line_start = 0;
    let mut found = false;
    for line_end in 0..buffer.len() {
        if buffer[line_end] != b'\n' {
            continue;
        }
        let line = &buffer[line_start..line_end];
        line_start = line_end + 1;
        let Ok(line) = std::str::from_utf8(line) else {
            continue;
        };
        let Some(data) = line.trim().strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data == "[DONE]" {
            continue;
        }
        let Ok(event) = serde_json::from_str::<serde_json::Value>(data) else {
            continue;
        };
        if let Some(content) = event
            .pointer("/choices/0/delta/content")
            .and_then(serde_json::Value::as_str)
            && !content.is_empty()
        {
            result.generated_content_bytes += content.len() as u64;
            found = true;
        }
        if let Some(completion_tokens) = event
            .pointer("/usage/completion_tokens")
            .and_then(serde_json::Value::as_u64)
        {
            result.reported_completion_tokens = Some(completion_tokens);
        }
    }
    if line_start > 0 {
        buffer.drain(..line_start);
    }
    found
}

async fn capture_reclaim_samples(sampler: &mut Sampler, result: &mut LocalBenchmarkResult) {
    let loaded_rss = sampler
        .samples
        .iter()
        .find(|sample| sample.phase == LocalBenchmarkPhase::AfterLoad)
        .map(|sample| sample.process_rss_mb);
    let mut previous = Duration::ZERO;
    for (seconds, phase) in [
        (1, LocalBenchmarkPhase::Reclaimed1s),
        (5, LocalBenchmarkPhase::Reclaimed5s),
        (10, LocalBenchmarkPhase::Reclaimed10s),
    ] {
        let target = Duration::from_secs(seconds);
        tokio::time::sleep(target - previous).await;
        previous = target;
        sampler.capture(phase);
        let reclaimed = loaded_rss.map(|loaded| {
            loaded.saturating_sub(
                sampler
                    .samples
                    .last()
                    .expect("sample just captured")
                    .process_rss_mb,
            )
        });
        match seconds {
            1 => result.rss_reclaimed_mb_1s = reclaimed,
            5 => result.rss_reclaimed_mb_5s = reclaimed,
            10 => result.rss_reclaimed_mb_10s = reclaimed,
            _ => unreachable!(),
        }
    }
}

struct Sampler {
    clock: Instant,
    system: System,
    pid: Pid,
    samples: Vec<LocalBenchmarkMemorySample>,
}

impl Sampler {
    fn new(clock: Instant) -> Self {
        Self {
            clock,
            system: System::new_all(),
            pid: Pid::from_u32(std::process::id()),
            samples: Vec::new(),
        }
    }

    fn capture(&mut self, phase: LocalBenchmarkPhase) {
        self.system.refresh_memory();
        self.system
            .refresh_processes(ProcessesToUpdate::Some(&[self.pid]), true);
        let process_rss_mb = self
            .system
            .process(self.pid)
            .map(|process| process.memory() / 1024 / 1024)
            .unwrap_or_default();
        self.samples.push(LocalBenchmarkMemorySample {
            phase,
            elapsed_ms: elapsed_ms(self.clock),
            process_rss_mb,
            host_available_mb: self.system.available_memory() / 1024 / 1024,
            host_used_swap_mb: self.system.used_swap() / 1024 / 1024,
            devices: collect_device_telemetry(),
        });
    }
}

fn recipe(config: &LocalEngineConfig, catalog_repo: &str, args: &Args) -> LocalBenchmarkRecipe {
    LocalBenchmarkRecipe {
        model_id: config.model_alias.clone(),
        // Use the public catalog identity, never the installed absolute path.
        model_repo: catalog_repo.to_string(),
        artifact_mode: if config.from_uqff.is_some() {
            LocalBenchmarkArtifactMode::PrequantizedUqff
        } else {
            LocalBenchmarkArtifactMode::InSituQuantization
        },
        quantization: config
            .from_uqff
            .as_ref()
            .map(|_| "uqff".to_string())
            .or_else(|| config.in_situ_quant.clone()),
        cpu_only: config.cpu_only,
        max_seq_len: config.max_seq_len,
        max_batch_size: config.max_batch_size,
        synthetic_prompt_tokens: args.prompt_tokens,
        max_output_tokens: args.output_tokens,
        sampling_seed: SAMPLING_SEED,
        bind: config.bind.clone(),
    }
}

fn to_runtime_config(
    config: LocalEngineConfig,
    worker: medousa_types::local::LocalWorkerStatus,
) -> RuntimeConfig {
    RuntimeConfig {
        bind: config.bind,
        model_repo: config.model_repo,
        model_alias: config.model_alias,
        from_uqff: config.from_uqff,
        in_situ_quant: config.in_situ_quant,
        cpu_only: config.cpu_only,
        max_seq_len: config.max_seq_len,
        max_batch_size: config.max_batch_size,
        idle_timeout_secs: config.idle_timeout_secs,
        critical_available_mb: config.critical_available_mb,
        worker,
    }
}

fn synthetic_prompt(approximate_tokens: usize) -> String {
    const WORDS: [&str; 8] = [
        "local",
        "reasoning",
        "memory",
        "tool",
        "plan",
        "verify",
        "answer",
        "briefly",
    ];
    (0..approximate_tokens)
        .map(|index| WORDS[index % WORDS.len()])
        .collect::<Vec<_>>()
        .join(" ")
}

fn git_state() -> LocalBenchmarkGitState {
    let revision = git_output(&["rev-parse", "HEAD"]);
    let dirty = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| !output.stdout.is_empty());
    LocalBenchmarkGitState { revision, dirty }
}

fn host_identity() -> LocalBenchmarkHostIdentity {
    let system = System::new_all();
    LocalBenchmarkHostIdentity {
        os_name: System::name(),
        os_version: System::os_version(),
        kernel_version: System::kernel_version(),
        cpu_brand: system
            .cpus()
            .first()
            .map(|cpu| cpu.brand().trim().to_string())
            .filter(|brand| !brand.is_empty()),
    }
}

fn git_output(args: &[&str]) -> Option<String> {
    Command::new("git")
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|output| output.trim().to_string())
        .filter(|output| !output.is_empty())
}

fn empty_result() -> LocalBenchmarkResult {
    LocalBenchmarkResult {
        outcome: LocalBenchmarkOutcome::Running,
        error: None,
        load_ms: None,
        ttft_ms: None,
        stream_ms: None,
        response_chunks: 0,
        response_bytes: 0,
        generated_content_bytes: 0,
        reported_completion_tokens: None,
        unload_ms: None,
        rss_reclaimed_mb_1s: None,
        rss_reclaimed_mb_5s: None,
        rss_reclaimed_mb_10s: None,
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn write_manifest(
    manifest: &LocalBenchmarkManifest,
    output: Option<&PathBuf>,
) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(manifest)?;
    if let Some(path) = output {
        use std::io::Write;

        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .with_context(|| format!("writing benchmark manifest to {}", path.display()))?;
        file.write_all(format!("{json}\n").as_bytes())
            .with_context(|| format!("writing benchmark manifest to {}", path.display()))?;
        println!("wrote {}", path.display());
    } else {
        println!("{json}");
    }
    Ok(())
}

fn parse_args(values: Vec<String>) -> anyhow::Result<Args> {
    if values
        .iter()
        .any(|value| value == "--help" || value == "-h")
    {
        print_help();
        std::process::exit(0);
    }
    let model_id = flag_value(&values, "--model-id")
        .context("--model-id is required (benchmarks never download models)")?;
    let bind = flag_value(&values, "--bind").unwrap_or_else(|| DEFAULT_BIND.to_string());
    let prompt_tokens = numeric_flag(&values, "--prompt-tokens", DEFAULT_PROMPT_TOKENS)?;
    let output_tokens = numeric_flag(&values, "--output-tokens", DEFAULT_OUTPUT_TOKENS)?;
    let max_seq_len = optional_numeric_flag(&values, "--context")?;
    let max_batch_size = optional_numeric_flag(&values, "--batch")?;
    anyhow::ensure!(
        prompt_tokens > 0,
        "--prompt-tokens must be greater than zero"
    );
    anyhow::ensure!(
        output_tokens > 0,
        "--output-tokens must be greater than zero"
    );
    anyhow::ensure!(
        max_seq_len.is_none_or(|value| value > 0),
        "--context must be greater than zero"
    );
    anyhow::ensure!(
        max_batch_size.is_none_or(|value| value > 0),
        "--batch must be greater than zero"
    );
    Ok(Args {
        model_id,
        bind,
        prompt_tokens,
        output_tokens,
        max_seq_len,
        max_batch_size,
        output: flag_value(&values, "--output").map(PathBuf::from),
    })
}

fn optional_numeric_flag(values: &[String], key: &str) -> anyhow::Result<Option<usize>> {
    flag_value(values, key)
        .map(|value| {
            value
                .parse::<usize>()
                .with_context(|| format!("{key} must be a positive integer"))
        })
        .transpose()
}

fn numeric_flag(values: &[String], key: &str, default: usize) -> anyhow::Result<usize> {
    flag_value(values, key)
        .map(|value| {
            value
                .parse::<usize>()
                .with_context(|| format!("{key} must be a positive integer"))
        })
        .transpose()
        .map(|value| value.unwrap_or(default))
}

fn flag_value(values: &[String], key: &str) -> Option<String> {
    values
        .iter()
        .position(|value| value == key)
        .and_then(|index| values.get(index + 1))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn print_help() {
    println!(
        r#"medousa_local_bench - content-free local inference lifecycle benchmark

usage:
  medousa_local_bench --model-id <installed-id> [options]

options:
  --model-id <id>          Installed catalog model; never downloaded by this command
  --bind <host:port>       Private benchmark bind (default: 127.0.0.1:7422)
  --prompt-tokens <count>  Approximate synthetic prompt size (default: 64)
  --output-tokens <count>  Maximum generated tokens (default: 64)
  --context <count>        Context cap, bounded by the catalog recipe
  --batch <count>          Batch cap, bounded by the catalog recipe
  --output <path>          Write JSON manifest instead of stdout
  -h, --help               Show this help

The manifest records timings, memory, recipe, hardware, and build identity. It
never records prompts, generated text, files, tool results, or credentials.
"#
    );
}

#[cfg(test)]
mod tests {
    use super::{empty_result, observe_sse_chunk, parse_args, synthetic_prompt};

    #[test]
    fn benchmark_requires_explicit_installed_model() {
        assert!(parse_args(Vec::new()).is_err());
    }

    #[test]
    fn synthetic_prompt_is_deterministic_and_bounded() {
        let prompt = synthetic_prompt(16);
        assert_eq!(prompt.split_whitespace().count(), 16);
        assert_eq!(prompt, synthetic_prompt(16));
    }

    #[test]
    fn parses_benchmark_recipe_bounds() {
        let args = parse_args(vec![
            "--model-id".into(),
            "gemma-4-e2b-it-qat".into(),
            "--prompt-tokens".into(),
            "128".into(),
            "--output-tokens".into(),
            "32".into(),
            "--context".into(),
            "2048".into(),
            "--batch".into(),
            "1".into(),
        ])
        .unwrap();
        assert_eq!(args.prompt_tokens, 128);
        assert_eq!(args.output_tokens, 32);
        assert_eq!(args.max_seq_len, Some(2048));
        assert_eq!(args.max_batch_size, Some(1));
    }

    #[test]
    fn rejects_zero_sweep_bounds() {
        assert!(parse_args(vec![
            "--model-id".into(),
            "gemma-4-e2b-it-qat".into(),
            "--context".into(),
            "0".into(),
        ])
        .is_err());
    }

    #[test]
    fn ttft_parser_ignores_role_event_and_handles_fragmented_content() {
        let mut buffer = Vec::new();
        let mut result = empty_result();
        assert!(!observe_sse_chunk(
            &mut buffer,
            b"data: {\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"con",
            &mut result,
        ));
        assert!(observe_sse_chunk(
            &mut buffer,
            b"tent\":\"hello\"}}]}\n\ndata: {\"usage\":{\"completion_tokens\":1}}\n\n",
            &mut result,
        ));
        assert_eq!(result.generated_content_bytes, 5);
        assert_eq!(result.reported_completion_tokens, Some(1));
    }
}
