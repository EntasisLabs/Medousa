//! Durable Slack Socket Mode intake. Events are written before Slack is ACKed.

use std::fs::{self, OpenOptions};
use std::future::Future;
use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{Mutex, Notify};

const MAX_PENDING: usize = 500;
const MAX_COMPLETED: usize = 5_000;
const MAX_EVENT_BYTES: usize = 64 * 1024;

#[derive(Clone, Serialize, Deserialize)]
pub struct StoredEvent {
    pub id: String,
    pub callback: Value,
}

#[derive(Default, Serialize, Deserialize)]
struct InboxFile {
    #[serde(default)]
    pending: Vec<StoredEvent>,
    #[serde(default)]
    completed: Vec<String>,
}

pub struct SlackInbox {
    path: PathBuf,
    lock: Mutex<()>,
    notify: Notify,
    capacity: usize,
}

impl SlackInbox {
    pub fn for_workshop() -> Result<Self> {
        let root = std::env::var_os("MEDOUSA_DATA_DIR")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .or_else(|| dirs::data_local_dir().map(|path| path.join("medousa")))
            .context("no durable Medousa data directory for Slack intake")?;
        Ok(Self::at(root.join("slack").join("inbox.json"), MAX_PENDING))
    }

    fn at(path: PathBuf, capacity: usize) -> Self {
        Self {
            path,
            lock: Mutex::new(()),
            notify: Notify::new(),
            capacity,
        }
    }

    /// Returns false for a Slack retry already pending or completed.
    pub async fn record(&self, id: String, callback: Value) -> Result<bool> {
        if id.is_empty() || id.len() > 128 || serde_json::to_vec(&callback)?.len() > MAX_EVENT_BYTES
        {
            bail!("Slack event exceeds durable intake limits");
        }
        let _guard = self.lock.lock().await;
        let path = self.path.clone();
        let capacity = self.capacity;
        let inserted = tokio::task::spawn_blocking(move || {
            let mut file = read(&path)?;
            if file.completed.iter().any(|seen| seen == &id)
                || file.pending.iter().any(|event| event.id == id)
            {
                return Ok(false);
            }
            if file.pending.len() >= capacity {
                bail!("Slack intake inbox is full; event must be retried by Slack");
            }
            file.pending.push(StoredEvent { id, callback });
            write(&path, &file)?;
            Ok(true)
        })
        .await
        .context("join Slack intake write")??;
        if inserted {
            self.notify.notify_one();
        }
        Ok(inserted)
    }

    async fn pending(&self) -> Result<Vec<StoredEvent>> {
        let _guard = self.lock.lock().await;
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || read(&path).map(|file| file.pending))
            .await
            .context("join Slack intake read")?
    }

    async fn complete(&self, id: String) -> Result<()> {
        let _guard = self.lock.lock().await;
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || {
            let mut file = read(&path)?;
            file.pending.retain(|event| event.id != id);
            if !file.completed.contains(&id) {
                file.completed.push(id);
            }
            if file.completed.len() > MAX_COMPLETED {
                let drop = file.completed.len() - MAX_COMPLETED;
                file.completed.drain(..drop);
            }
            write(&path, &file)
        })
        .await
        .context("join Slack intake completion")?
    }

    pub async fn drain_once<F, Fut>(&self, mut deliver: F) -> Result<usize>
    where
        F: FnMut(Value) -> Fut,
        Fut: Future<Output = Result<()>>,
    {
        let mut delivered = 0;
        for event in self.pending().await? {
            deliver(event.callback).await?;
            self.complete(event.id).await?;
            delivered += 1;
        }
        Ok(delivered)
    }

    pub async fn wait_for_work(&self) {
        tokio::select! {
            _ = self.notify.notified() => {},
            _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {},
        }
    }
}

fn read(path: &PathBuf) -> Result<InboxFile> {
    #[cfg(windows)]
    {
        let backup = path.with_extension("json.backup");
        if !path.exists() && backup.exists() {
            fs::rename(&backup, path).with_context(|| format!("restore {}", path.display()))?;
        }
    }
    if !path.exists() {
        return Ok(InboxFile::default());
    }
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))
}

fn write(path: &PathBuf, value: &InboxFile) -> Result<()> {
    let parent = path.parent().context("Slack inbox path has no parent")?;
    fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    }
    let stage = path.with_extension(format!("json.tmp-{}", std::process::id()));
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&stage)?;
        file.write_all(&serde_json::to_vec(value)?)?;
        file.sync_all()?;
        #[cfg(windows)]
        {
            let backup = path.with_extension("json.backup");
            if !path.exists() && backup.exists() {
                fs::rename(&backup, path)?;
            }
            if path.exists() {
                if backup.exists() {
                    fs::remove_file(&backup)?;
                }
                fs::rename(path, &backup)?;
                if let Err(error) = fs::rename(&stage, path) {
                    let _ = fs::rename(&backup, path);
                    return Err(error.into());
                }
                let _ = fs::remove_file(backup);
                return Ok(());
            }
        }
        fs::rename(&stage, path)?;
        #[cfg(unix)]
        fs::File::open(parent)?.sync_all()?;
        Ok::<(), anyhow::Error>(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&stage);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn offline_retry_crash_recovery_and_duplicate_delivery() {
        let root = std::env::temp_dir().join(format!(
            "medousa-slack-inbox-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = root.join("slack").join("inbox.json");
        let first = SlackInbox::at(path.clone(), 1);
        assert!(
            first
                .record("event-1".into(), serde_json::json!({"message": "hello"}))
                .await
                .unwrap()
        );
        assert!(
            !first
                .record("event-1".into(), serde_json::json!({"message": "hello"}))
                .await
                .unwrap()
        );
        assert!(
            first
                .record("event-2".into(), serde_json::json!({}))
                .await
                .is_err()
        );

        // The ACKed event remains on disk after an adapter restart or daemon 503.
        let restarted = SlackInbox::at(path.clone(), 1);
        assert!(
            restarted
                .drain_once(|_| async { Err(anyhow::anyhow!("daemon HTTP 503")) })
                .await
                .is_err()
        );
        assert_eq!(restarted.pending().await.unwrap().len(), 1);
        restarted.drain_once(|_| async { Ok(()) }).await.unwrap();
        let again = SlackInbox::at(path.clone(), 1);
        assert!(again.pending().await.unwrap().is_empty());
        assert!(
            !again
                .record("event-1".into(), serde_json::json!({"message": "hello"}))
                .await
                .unwrap()
        );
        fs::remove_dir_all(root).unwrap();
    }
}
