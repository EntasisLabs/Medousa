//! Device-local layout persistence. No UI-thread filesystem writes or unbounded queues.
use serde_json::Value;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const MAX_BYTES: usize = 2 * 1024 * 1024;
const DEBOUNCE: Duration = Duration::from_millis(100);
static TEMP_ID: AtomicU64 = AtomicU64::new(0);

/// Only reads at most the limit plus one sentinel byte, even for growing files.
/// Structural layout validation remains the caller's responsibility.
pub fn load(path: &Path) -> Result<Option<Value>, String> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("Could not open layout: {e}")),
    };
    let mut bytes = Vec::new();
    file.take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Could not read layout: {e}"))?;
    if bytes.len() > MAX_BYTES {
        return Err("Layout exceeds 2 MiB limit".into());
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|e| format!("Invalid layout JSON: {e}"))
}

#[derive(Default)]
struct State {
    pending: Option<(u64, Value)>,
    ack: Option<Result<Value, String>>,
    submitted: u64,
    completed: u64,
    last_error: Option<String>,
    immediate: bool,
    stopping: bool,
}

/// One pending snapshot, one in-flight snapshot, and one latest acknowledgement.
/// A successful queue is acceptance, NOT confirmation of persistence. Poll for that.
pub struct Store {
    shared: Arc<(Mutex<State>, Condvar)>,
}

impl Store {
    pub fn new(path: PathBuf) -> Self {
        let shared = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let worker = Arc::clone(&shared);
        if let Err(e) = thread::Builder::new()
            .name("workspace-layout-store".into())
            .spawn(move || run(path, worker))
        {
            let mut state = shared.0.lock().unwrap();
            state.stopping = true;
            state.ack = Some(Err(format!("Could not start layout writer: {e}")));
        }
        Self { shared }
    }

    /// Replaces the pending value. False means the worker could not be started.
    pub fn queue(&self, value: Value) -> bool {
        let mut state = self.shared.0.lock().unwrap();
        if state.stopping {
            return false;
        }
        state.submitted += 1;
        state.pending = Some((state.submitted, value));
        self.shared.1.notify_one();
        true
    }

    /// Returns only the latest completed write; Ok carries the exact written value.
    pub fn poll(&self) -> Option<Result<Value, String>> {
        self.shared.0.lock().unwrap().ack.take()
    }

    /// Blocking, bounded wait for the current generation (or a newer coalesced one).
    /// For explicit non-UI shutdown/tests only. Timeout does not cancel a write.
    pub fn flush(&self, timeout: Duration) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        let mut state = self.shared.0.lock().unwrap();
        if state.stopping {
            return Err("Layout writer is stopped".into());
        }
        let target = state.submitted;
        state.immediate = true;
        self.shared.1.notify_one();
        while state.completed < target {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err("Timed out flushing layout".into());
            }
            state = self.shared.1.wait_timeout(state, remaining).unwrap().0;
        }
        state.last_error.clone().map_or(Ok(()), Err)
    }
}

impl Drop for Store {
    fn drop(&mut self) {
        // Never join or block on disk on the UI thread. Worker retains the mailbox,
        // flushes the latest pending snapshot, then exits. Abrupt process exit is
        // not a durability guarantee; shutdown callers should explicitly flush.
        let mut state = self.shared.0.lock().unwrap();
        state.stopping = true;
        self.shared.1.notify_one();
    }
}

fn run(path: PathBuf, shared: Arc<(Mutex<State>, Condvar)>) {
    loop {
        let (generation, value) = {
            let mut state = shared.0.lock().unwrap();
            while state.pending.is_none() && !state.stopping {
                state = shared.1.wait(state).unwrap();
            }
            if state.pending.is_none() {
                return;
            }
            // Fixed bounded coalescing window: a steady stream cannot starve writes.
            let deadline = Instant::now() + DEBOUNCE;
            while !state.stopping && !state.immediate && Instant::now() < deadline {
                state = shared
                    .1
                    .wait_timeout(state, deadline.saturating_duration_since(Instant::now()))
                    .unwrap()
                    .0;
            }
            state.immediate = false;
            state.pending.take().unwrap()
        };
        let result = save(&path, &value);
        let mut state = shared.0.lock().unwrap();
        state.completed = generation;
        state.last_error = result.as_ref().err().cloned();
        state.ack = Some(result.map(|()| value));
        shared.1.notify_all();
    }
}

/// Streaming cap avoids constructing an oversized serialized buffer.
struct LimitedBytes(Vec<u8>);
impl Write for LimitedBytes {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_BYTES.saturating_sub(self.0.len()) {
            return Err(io::Error::other("Layout exceeds 2 MiB limit"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn save(path: &Path, value: &Value) -> Result<(), String> {
    let mut bytes = LimitedBytes(Vec::new());
    serde_json::to_writer(&mut bytes, value)
        .map_err(|e| format!("Could not serialize layout: {e}"))?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(|e| format!("Could not create layout directory: {e}"))?;
    let name = path.file_name().ok_or("Layout path has no filename")?;
    let mut file = None;
    for _ in 0..32 {
        let mut temp_name = name.to_os_string();
        temp_name.push(format!(
            ".{}.{}.tmp",
            std::process::id(),
            TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let temp = parent.join(temp_name);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&temp) {
            Ok(handle) => {
                file = Some((temp, handle));
                break;
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                return Err(format!(
                    "Could not create private layout temporary file: {e}"
                ));
            }
        }
    }
    let (temp, mut handle) = file.ok_or("Could not reserve layout temporary file")?;
    let result = (|| {
        handle.write_all(&bytes.0)?;
        handle.sync_all()?;
        drop(handle);
        fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result.map_err(|e| format!("Could not atomically save layout: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "medousa-layout-test-{}-{}",
                std::process::id(),
                TEMP_ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&p).unwrap();
            Self(p)
        }
        fn path(&self) -> PathBuf {
            self.0.join("layout.json")
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn roundtrip_and_private_atomic_replacement() {
        let dir = Directory::new();
        assert_eq!(load(&dir.path()).unwrap(), None);
        let store = Store::new(dir.path());
        for n in 0..2 {
            let value = json!({"desktop": n});
            assert!(store.queue(value.clone()));
            store.flush(Duration::from_secs(5)).unwrap();
            assert_eq!(store.poll().unwrap().unwrap(), value);
            assert_eq!(load(&dir.path()).unwrap(), Some(value));
            assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 1);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(dir.path()).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn oversized_and_malformed_inputs_are_rejected() {
        let dir = Directory::new();
        let f = File::create(dir.path()).unwrap();
        f.set_len((MAX_BYTES + 1) as u64).unwrap();
        assert!(load(&dir.path()).unwrap_err().contains("2 MiB"));
        fs::write(dir.path(), b"not JSON").unwrap();
        assert!(
            load(&dir.path())
                .unwrap_err()
                .contains("Invalid layout JSON")
        );
        assert!(save(&dir.path(), &json!("x".repeat(MAX_BYTES))).is_err());
        assert_eq!(fs::read(dir.path()).unwrap(), b"not JSON");
        assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 1);
    }

    #[test]
    fn latest_pending_snapshot_coalesces() {
        let dir = Directory::new();
        let store = Store::new(dir.path());
        for n in 0..100 {
            assert!(store.queue(json!({"revision": n})));
        }
        store.flush(Duration::from_secs(5)).unwrap();
        assert_eq!(load(&dir.path()).unwrap(), Some(json!({"revision": 99})));
        assert_eq!(store.poll().unwrap().unwrap(), json!({"revision": 99}));
        assert!(store.poll().is_none());
    }

    #[test]
    fn failed_ack_is_not_success_and_can_be_retried() {
        let dir = Directory::new();
        fs::create_dir(dir.path()).unwrap(); // destination cannot be replaced by a file
        let store = Store::new(dir.path());
        let value = json!({"revision": 1});
        assert!(store.queue(value.clone()));
        assert!(store.flush(Duration::from_secs(5)).is_err());
        assert!(store.poll().unwrap().is_err());
        assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 1); // no temporary leftovers
        fs::remove_dir(dir.path()).unwrap();
        assert!(store.queue(value.clone()));
        store.flush(Duration::from_secs(5)).unwrap();
        assert_eq!(store.poll().unwrap().unwrap(), value);
    }

    #[test]
    fn drop_flushes_pending_without_joining() {
        let dir = Directory::new();
        let store = Store::new(dir.path());
        store.queue(json!({"final": true}));
        drop(store);
        let deadline = Instant::now() + Duration::from_secs(5);
        while !dir.path().exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(load(&dir.path()).unwrap(), Some(json!({"final": true})));
    }
}
