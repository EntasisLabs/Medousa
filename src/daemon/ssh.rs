//! Workshop-owned, explicitly granted SSH targets. Models never configure hosts
//! or receive private keys. Execution receipts survive Home and daemon restarts.
mod http;
mod model;
mod process;
#[cfg(test)]
mod tests;
pub use http::surface;
pub use model::{ExecutionQuery, RunInput, TargetsQuery, TerminalInput, TerminalWriteInput};

use crate::request_principal::{Capability, PrincipalKind, RequestPrincipal};
use anyhow::{Result, bail};
use medousa_forge::execution::{ExecutionClass, ForgeExecutionService};
use model::{Execution, SaveTarget, Target, TargetConfig};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use tokio::sync::Mutex;

static HOST: OnceLock<Arc<SshHost>> = OnceLock::new();

pub fn local_host() -> Result<Arc<SshHost>> {
    HOST.get()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("SSH connections unavailable on this workshop"))
}

pub async fn compose(
    execution: Arc<ForgeExecutionService>,
    root: PathBuf,
    shell: Arc<super::shell_session_host::ShellSessionHost>,
) -> Result<()> {
    let host = SshHost::open(execution, root, shell).await?;
    HOST.set(host)
        .map_err(|_| anyhow::anyhow!("SSH host already composed"))
}

#[derive(Clone, Default, Deserialize, Serialize)]
struct Store {
    targets: BTreeMap<String, Target>,
}

pub struct SshHost {
    root: PathBuf,
    shell: Arc<super::shell_session_host::ShellSessionHost>,
    execution: Arc<ForgeExecutionService>,
    store: Mutex<Store>,
    slots: Arc<tokio::sync::Semaphore>,
    ssh_program: PathBuf,
}

impl SshHost {
    async fn open(
        execution: Arc<ForgeExecutionService>,
        root: PathBuf,
        shell: Arc<super::shell_session_host::ShellSessionHost>,
    ) -> Result<Arc<Self>> {
        let path = root.join("connections.json");
        let store: Store = match tokio::fs::read(&path).await {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Store::default(),
            Err(e) => return Err(e.into()),
        };
        let host = Arc::new(Self {
            root,
            shell,
            execution,
            store: Mutex::new(store),
            slots: Arc::new(tokio::sync::Semaphore::new(4)),
            ssh_program: "ssh".into(),
        });
        host.persist(&*host.store.lock().await).await?;
        host.recover().await?;
        Ok(host)
    }

    async fn persist(&self, store: &Store) -> Result<()> {
        self.write_json("connections.json", store).await
    }

    async fn write_json(&self, name: &str, value: &impl Serialize) -> Result<()> {
        let bytes = serde_json::to_vec(value)?;
        let root = self.root.clone();
        let name = name.to_string();
        self.execution
            .run(ExecutionClass::StoreIo, bytes.len(), move || {
                Ok((|| -> Result<()> {
                    std::fs::create_dir_all(&root)?;
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))?;
                    }
                    let temporary = root.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
                    let mut options = std::fs::OpenOptions::new();
                    options.write(true).create_new(true);
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::OpenOptionsExt;
                        options.mode(0o600);
                    }
                    use std::io::Write;
                    let mut file = options.open(&temporary)?;
                    file.write_all(&bytes)?;
                    file.sync_all()?;
                    std::fs::rename(&temporary, root.join(name))?;
                    Ok(())
                })())
            })
            .await??;
        Ok(())
    }

    async fn read_execution(&self, id: &str) -> Result<Option<Execution>> {
        let hash = id.strip_prefix("ssh-exec-").unwrap_or("");
        if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            bail!("invalid SSH execution id");
        }
        match tokio::fs::read(self.root.join(format!("{id}.json"))).await {
            Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn update_execution<T>(
        &self,
        id: &str,
        mutate: impl FnOnce(&mut Execution) -> Result<T>,
    ) -> Result<T> {
        let _lock = self.store.lock().await;
        let mut record = self
            .read_execution(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("SSH execution not found"))?;
        let result = mutate(&mut record)?;
        self.write_json(&format!("{id}.json"), &record).await?;
        Ok(result)
    }

    async fn recover(&self) -> Result<()> {
        let mut files = tokio::fs::read_dir(&self.root).await?;
        while let Some(file) = files.next_entry().await? {
            let name = file.file_name().to_string_lossy().to_string();
            if let Some(id) = name
                .strip_suffix(".json")
                .filter(|n| n.starts_with("ssh-exec-"))
            {
                self.update_execution(id, |record| {
                    if matches!(record.status.as_str(), "starting" | "running" | "terminal") {
                        record.status = "unknown".into();
                        record.error = Some(
                            "workshop restarted; remote outcome must be checked before new work"
                                .into(),
                        );
                    }
                    Ok(())
                })
                .await?;
            }
        }
        Ok(())
    }

    async fn change<T>(&self, mutate: impl FnOnce(&mut Store) -> Result<T>) -> Result<T> {
        let mut store = self.store.lock().await;
        let mut next = store.clone();
        let result = mutate(&mut next)?;
        self.persist(&next).await?;
        *store = next;
        Ok(result)
    }

    pub async fn targets(&self, owner: &str, agents_only: bool) -> serde_json::Value {
        let store = self.store.lock().await;
        let targets: Vec<_> = store
            .targets
            .values()
            .filter(|t| t.owner == owner && (!agents_only || t.config.agent_access))
            .map(Target::summary)
            .collect();
        serde_json::json!({"targets": targets})
    }

    async fn save(&self, owner: String, input: SaveTarget) -> Result<serde_json::Value> {
        process::validate(&input.config)?;
        let keys = process::normalize_keys(&input.host_keys)?;
        let target = Target {
            id: format!("ssh-{}", uuid::Uuid::new_v4()),
            owner,
            config: input.config,
            host_keys: keys,
        };
        self.change(|store| {
            if store.targets.len() >= 64 {
                bail!("SSH target capacity reached; remove an unused connection");
            }
            store.targets.insert(target.id.clone(), target.clone());
            Ok(target.summary())
        })
        .await
    }

    async fn access(&self, owner: &str, id: &str, agent: bool) -> Result<Target> {
        let store = self.store.lock().await;
        let target = store
            .targets
            .get(id)
            .filter(|t| t.owner == owner)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("SSH target not found for this owner"))?;
        if agent && !target.config.agent_access {
            bail!("agent access is disabled for this SSH target");
        }
        Ok(target)
    }

    async fn set_access(&self, owner: &str, id: &str, enabled: bool) -> Result<()> {
        self.change(|store| {
            let target = store
                .targets
                .get_mut(id)
                .filter(|t| t.owner == owner)
                .ok_or_else(|| anyhow::anyhow!("SSH target not found for this owner"))?;
            target.config.agent_access = enabled;
            Ok(())
        })
        .await
    }

    async fn remove(&self, owner: &str, id: &str) -> Result<()> {
        self.access(owner, id, false).await?;
        self.change(|store| {
            store.targets.remove(id);
            Ok(())
        })
        .await
    }

    pub async fn status(&self, owner: &str, id: &str) -> Result<serde_json::Value> {
        let record = self
            .read_execution(id)
            .await?
            .filter(|e| e.owner == owner)
            .ok_or_else(|| anyhow::anyhow!("SSH execution not found for this owner"))?;
        Ok(record.summary())
    }

    async fn reserve(
        &self,
        owner: &str,
        target_id: &str,
        request_key: &str,
        command: Option<String>,
        timeout_ms: Option<u64>,
    ) -> Result<(Execution, bool)> {
        if request_key.trim().is_empty() || request_key.len() > 128 {
            bail!("request_key must be between 1 and 128 bytes");
        }
        if command
            .as_ref()
            .is_some_and(|c| c.trim().is_empty() || c.len() > 16 * 1024 || c.contains('\0'))
        {
            bail!("remote command is empty or too large");
        }
        if timeout_ms == Some(0) {
            bail!("timeout_ms must be greater than zero");
        }
        use sha2::{Digest, Sha256};
        let id = format!(
            "ssh-exec-{:x}",
            Sha256::digest(format!("{}:{owner}{request_key}", owner.len()))
        );
        let _lock = self.store.lock().await;
        if let Some(old) = self.read_execution(&id).await? {
            if old.owner != owner
                || old.target_id != target_id
                || old.command != command
                || old.timeout_ms != timeout_ms
            {
                bail!("request_key already belongs to a different SSH operation");
            }
            return Ok((old, false));
        }
        let record = Execution {
            execution_id: id,
            owner: owner.into(),
            target_id: target_id.into(),
            request_key: request_key.into(),
            command,
            timeout_ms,
            status: "starting".into(),
            stdout: String::new(),
            stderr: String::new(),
            output_truncated: false,
            exit_code: None,
            session_id: None,
            error: None,
        };
        self.write_json(&format!("{}.json", record.execution_id), &record)
            .await?;
        Ok((record, true))
    }

    pub async fn run(
        self: &Arc<Self>,
        owner: &str,
        input: RunInput,
        agent: bool,
    ) -> Result<serde_json::Value> {
        let target = self.access(owner, &input.target_id, agent).await?;
        // Admit before reserving a receipt, so overload never creates phantom work.
        let permit = self.slots.clone().try_acquire_owned().map_err(|_| {
            anyhow::anyhow!("SSH execution busy; poll existing work before retrying")
        })?;
        let (record, fresh) = self
            .reserve(
                owner,
                &input.target_id,
                &input.request_key,
                Some(input.command),
                input.timeout_ms,
            )
            .await?;
        if fresh {
            let host = self.clone();
            let job = record.clone();
            tokio::spawn(async move {
                let _permit = permit;
                if let Err(error) = host.execute(target, job.clone()).await {
                    let message = error.to_string();
                    if let Err(error) = host
                        .update_execution(&job.execution_id, |receipt| {
                            receipt.status = "unknown".into();
                            receipt.error = Some(message);
                            Ok(())
                        })
                        .await
                    {
                        tracing::error!(%error, "cannot persist SSH failure receipt");
                    }
                }
            });
        }
        Ok(record.summary())
    }

    async fn execute(&self, target: Target, job: Execution) -> Result<()> {
        // Recheck a grant after queueing; never borrow an operator credential.
        self.access(&job.owner, &target.id, true).await?;
        let known_hosts = process::prepare_known_hosts(&self.root, &target).await?;
        let args = process::argv(&target, &known_hosts, job.command.as_deref());
        let mut command = process::ssh_command_for(&self.ssh_program, &args);
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                return self
                    .update_execution(&job.execution_id, |receipt| {
                        receipt.status = "not_started".into();
                        receipt.error = Some(error.to_string());
                        Ok(())
                    })
                    .await;
            }
        };
        self.update_execution(&job.execution_id, |receipt| {
            receipt.status = "running".into();
            Ok(())
        })
        .await?;
        let stdout = tokio::spawn(process::drain(child.stdout.take().expect("piped stdout")));
        let stderr = tokio::spawn(process::drain(child.stderr.take().expect("piped stderr")));
        let mut timed_out = false;
        let status = if let Some(ms) = job.timeout_ms {
            match tokio::time::timeout(std::time::Duration::from_millis(ms), child.wait()).await {
                Ok(status) => status?,
                Err(_) => {
                    timed_out = true;
                    child.kill().await?;
                    child.wait().await?
                }
            }
        } else {
            child.wait().await?
        };
        let (out, out_truncated) = stdout.await??;
        let (err, err_truncated) = stderr.await??;
        self.update_execution(&job.execution_id, |receipt| {
            receipt.exit_code = status.code();
            receipt.stdout = out;
            receipt.stderr = err;
            receipt.output_truncated = out_truncated || err_truncated;
            receipt.status =
                if timed_out || status.code() == Some(255) || status.code().is_none() {
                    "unknown"
                } else if status.success() {
                    "succeeded"
                } else {
                    "failed"
                }
                .into();
            if timed_out {
                receipt.error =
                    Some("SSH observation timed out; remote command may still have run".into());
            }
            Ok(())
        })
        .await
    }

    pub async fn open_terminal(
        self: &Arc<Self>,
        owner: &str,
        input: TerminalInput,
        agent: bool,
    ) -> Result<serde_json::Value> {
        let target = self.access(owner, &input.target_id, agent).await?;
        let (record, fresh) = self
            .reserve(owner, &input.target_id, &input.request_key, None, None)
            .await?;
        if !fresh {
            return Ok(record.summary());
        }
        let result = async {
            let known_hosts = process::prepare_known_hosts(&self.root, &target).await?;
            let mut argv = vec!["ssh".into()];
            argv.extend(process::argv(&target, &known_hosts, None));
            super::shell_session_host::create_ssh_session(&self.shell, &argv)
                .await
                .map_err(|(_, message)| anyhow::anyhow!(message))
        }
        .await;
        self.update_execution(&record.execution_id, |receipt| {
            match result {
                Ok(id) => {
                    receipt.session_id = Some(id);
                    receipt.status = "terminal".into();
                }
                Err(error) => {
                    receipt.status = "unknown".into();
                    receipt.error = Some(error.to_string());
                }
            }
            Ok(receipt.summary())
        })
        .await
    }

    pub async fn terminal_input(
        &self,
        owner: &str,
        input: TerminalWriteInput,
    ) -> Result<serde_json::Value> {
        let record = self
            .read_execution(&input.execution_id)
            .await?
            .filter(|e| e.owner == owner)
            .ok_or_else(|| anyhow::anyhow!("SSH terminal not found for this owner"))?;
        self.access(owner, &record.target_id, true).await?;
        if record.status != "terminal" {
            bail!(
                "SSH terminal is no longer connected; inspect remote state before opening a new one"
            );
        }
        if input.input.as_ref().is_some_and(|v| v.len() > 16 * 1024) {
            bail!("terminal input exceeds limit");
        }
        let id = record
            .session_id
            .ok_or_else(|| anyhow::anyhow!("SSH terminal has no session"))?;
        let output = crate::coding_tools::stream_session_input(
            &id,
            input.input.as_deref().map(str::as_bytes),
            input.wait_ms.unwrap_or(1000).clamp(100, 15000),
            input.after_sequence,
            None,
        )
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        Ok(
            serde_json::json!({"execution_id": record.execution_id, "output": output.output,
            "input_written": output.input_written, "next_sequence": output.next_sequence,
            "replay_truncated": output.replay_truncated, "output_truncated": output.output_truncated,
            "command_completion": "untracked_interactive_input"}),
        )
    }
}

pub fn turn_owner() -> Result<String> {
    let turn = crate::agent_runtime::execution_context::active_turn_execution_context()
        .ok_or_else(|| anyhow::anyhow!("SSH tools require an admitted turn"))?;
    owner(
        turn.principal(),
        turn.legacy_scope().identity_user_id.as_deref(),
    )
}

fn owner(principal: &RequestPrincipal, admitted: Option<&str>) -> Result<String> {
    if !principal
        .capabilities()
        .contains(Capability::WorkshopInteract)
    {
        bail!("principal cannot use SSH connections");
    }
    principal
        .profile_id()
        .or_else(|| {
            (principal.kind() == PrincipalKind::LocalApp)
                .then_some(admitted)
                .flatten()
        })
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("SSH requires a bound owner identity"))
}

pub async fn inspect(config: TargetConfig) -> Result<serde_json::Value> {
    let host = local_host()?;
    host.execution
        .run_async(
            ExecutionClass::WorkEnvironment,
            256 * 1024,
            None,
            async move { Ok(process::inspect(config).await) },
        )
        .await?
}
