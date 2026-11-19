//! Bounded capture of background daemon stdout/stderr on Unix.
//!
//! Only takes over when BOTH descriptors point at the canonical daemon log.
//! Interactive terminals, pipes, and service-manager journals stay untouched.

#[cfg(unix)]
mod unix {
    use super::super::log_rotation::{RotateConfig, rotate_if_oversized};
    use std::fs::{File, OpenOptions};
    use std::io::{self, Read, Seek, SeekFrom, Write};
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    use std::os::unix::net::UnixStream;
    use std::path::{Path, PathBuf};
    use std::sync::mpsc::{self, Sender};
    use std::thread::JoinHandle;
    use std::time::{Duration, Instant};

    pub struct OutputGuard {
        path: PathBuf,
        stop: Sender<()>,
        worker: Option<JoinHandle<()>>,
    }

    fn duplicate(fd: i32) -> io::Result<File> {
        // SAFETY: dup returns a new, owned descriptor; never owns the source.
        let copy = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 3) };
        if copy < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(unsafe { File::from_raw_fd(copy) })
    }

    fn replace(source: &impl AsRawFd, target: i32) -> io::Result<()> {
        // SAFETY: source is live, target is one of the standard descriptors.
        if unsafe { libc::dup2(source.as_raw_fd(), target) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    pub(super) struct Sink {
        path: PathBuf,
        file: Option<File>,
        len: u64,
        config: RotateConfig,
    }

    impl Sink {
        pub(super) fn open(path: &Path, config: RotateConfig) -> io::Result<Self> {
            let config = RotateConfig {
                max_bytes: config.max_bytes.max(1),
                max_backups: config.max_backups.clamp(1, 10),
            };
            let mut file = OpenOptions::new().read(true).append(true).open(path)?;
            let len = file.metadata()?.len();
            // Retain only a bounded tail on recovery. Do not archive a multi-GB
            // legacy file. A temporary backup is synced before truncation.
            if len > config.max_bytes {
                let recovery = path.with_extension("log.recovery");
                let mut tail = OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .mode(0o600)
                    .open(&recovery)?;
                file.seek(SeekFrom::Start(len - config.max_bytes))?;
                io::copy(&mut (&mut file).take(config.max_bytes), &mut tail)?;
                tail.sync_all()?;
                // Rotate the bounded recovery through the same backup chain.
                rotate_if_oversized(path, config)?;
                let first = PathBuf::from(format!("{}.1", path.display()));
                std::fs::rename(&recovery, &first)?;
                // Launchers still own descriptors to the legacy inode. Release
                // its blocks now, not only when those handles finally close.
                file.set_len(0)?;
                file = OpenOptions::new()
                    .read(true)
                    .create(true)
                    .append(true)
                    .mode(0o600)
                    .open(path)?;
            }
            let len = file.metadata()?.len();
            Ok(Self {
                path: path.to_owned(),
                file: Some(file),
                len,
                config,
            })
        }

        fn rotate(&mut self) -> io::Result<()> {
            self.file.take();
            // Existing helper rotates only when >, whereas this writer rotates
            // BEFORE the next byte would exceed the limit.
            rotate_if_oversized(
                &self.path,
                RotateConfig {
                    max_bytes: self.len.saturating_sub(1),
                    ..self.config
                },
            )?;
            self.file = Some(
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .mode(0o600)
                    .open(&self.path)?,
            );
            self.len = 0;
            Ok(())
        }

        pub(super) fn write_all(&mut self, mut bytes: &[u8]) -> io::Result<()> {
            while !bytes.is_empty() {
                if self.len >= self.config.max_bytes {
                    self.rotate()?;
                }
                let count = bytes
                    .len()
                    .min((self.config.max_bytes - self.len).min(usize::MAX as u64) as usize);
                self.file
                    .as_mut()
                    .ok_or_else(|| io::Error::other("log sink closed"))?
                    .write_all(&bytes[..count])?;
                self.len += count as u64;
                bytes = &bytes[count..];
            }
            Ok(())
        }
    }

    pub fn capture(path: &Path, config: RotateConfig) -> io::Result<Option<OutputGuard>> {
        let metadata = match std::fs::metadata(path) {
            Ok(metadata) if metadata.is_file() => metadata,
            Ok(_) => return Ok(None),
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err),
        };
        let stdout = duplicate(libc::STDOUT_FILENO)?;
        let stderr = duplicate(libc::STDERR_FILENO)?;
        for descriptor in [&stdout, &stderr] {
            let actual = descriptor.metadata()?;
            if actual.dev() != metadata.dev() || actual.ino() != metadata.ino() {
                return Ok(None);
            }
        }
        let mut sink = Sink::open(path, config)?;
        let (mut reader, writer) = UnixStream::pair()?;
        reader.set_read_timeout(Some(Duration::from_millis(100)))?;
        let (stop, stopped) = mpsc::channel();
        let worker = std::thread::Builder::new()
            .name("daemon-log-writer".into())
            .spawn(move || {
                let mut buffer = [0; 8192];
                let mut deadline = None;
                loop {
                    if stopped.try_recv().is_ok() {
                        deadline = Some(Instant::now() + Duration::from_secs(1));
                    }
                    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                        break;
                    }
                    match reader.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(count) => {
                            // Fail closed on disk errors instead of silently dropping logs
                            // indefinitely. A closed socket makes the failure visible.
                            if sink.write_all(&buffer[..count]).is_err() {
                                break;
                            }
                        }
                        Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
                        Err(err)
                            if matches!(
                                err.kind(),
                                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                            ) =>
                        {
                            if deadline.is_some() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            })?;
        let guard = OutputGuard {
            path: path.to_owned(),
            stop,
            worker: Some(worker),
        };
        if let Err(err) = replace(&writer, libc::STDOUT_FILENO) {
            drop(guard);
            return Err(err);
        }
        if let Err(err) = replace(&writer, libc::STDERR_FILENO) {
            let _ = replace(&stdout, libc::STDOUT_FILENO);
            drop(guard);
            return Err(err);
        }
        // Guard now owns restoration and draining of the captured descriptors.
        Ok(Some(guard))
    }

    impl Drop for OutputGuard {
        fn drop(&mut self) {
            let _ = io::stdout().flush();
            let _ = io::stderr().flush();
            // Disconnect process writers before draining; descendants may keep
            // their inherited socket open, so the worker also has a drain deadline.
            if let Ok(null) = OpenOptions::new().write(true).open("/dev/null") {
                let _ = replace(&null, libc::STDOUT_FILENO);
                let _ = replace(&null, libc::STDERR_FILENO);
            }
            let _ = self.stop.send(());
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
            // main's Result/panic diagnostics happen after local guards drop.
            // Reopen the CURRENT path, not a descriptor to an old rotated inode.
            if let Ok(file) = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)
            {
                let _ = replace(&file, libc::STDOUT_FILENO);
                let _ = replace(&file, libc::STDERR_FILENO);
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        // Descriptor replacement is process-wide: exercise it in a child,
        // never in the parallel unit-test runner itself.
        #[test]
        fn capture_child() {
            let Some(path) = std::env::var_os("MEDOUSA_LOG_TEST_CHILD") else {
                return;
            };
            let output = capture(
                Path::new(&path),
                RotateConfig {
                    max_bytes: 1024,
                    max_backups: 3,
                },
            )
            .unwrap();
            assert!(output.is_some());
            for _ in 0..50 {
                io::stdout().write_all(&[b'O'; 100]).unwrap();
                io::stderr().write_all(&[b'E'; 100]).unwrap();
            }
            io::stdout().flush().unwrap();
            io::stderr().flush().unwrap();
            assert!(
                std::process::Command::new("/bin/sh")
                    .args(["-c", "printf child-stdout; printf child-stderr >&2"])
                    .status()
                    .unwrap()
                    .success()
            );
            drop(output);
        }

        #[test]
        fn redirects_raw_output_and_inherited_child_output() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("daemon.log");
            let log = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .unwrap();
            let test = format!(
                "{}::capture_child",
                module_path!().split_once("::").unwrap().1
            );
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", &test, "--nocapture"])
                .env("MEDOUSA_LOG_TEST_CHILD", &path)
                .stdout(log.try_clone().unwrap())
                .stderr(log)
                .status()
                .unwrap();
            assert!(status.success());
            for entry in std::fs::read_dir(dir.path()).unwrap() {
                assert!(entry.unwrap().metadata().unwrap().len() <= 1024);
            }
            let active = std::fs::read(&path).unwrap();
            assert!(
                active.windows(12).any(|bytes| bytes == b"child-stdout"),
                "active tail: {:?}",
                String::from_utf8_lossy(&active)
            );
            assert!(active.windows(12).any(|bytes| bytes == b"child-stderr"));
            assert!(dir.path().join("daemon.log.3").exists());
            assert!(!dir.path().join("daemon.log.4").exists());
        }

        #[test]
        fn does_not_capture_unrelated_descriptors() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("daemon.log");
            std::fs::write(&path, b"untouched").unwrap();
            assert!(capture(&path, RotateConfig::default()).unwrap().is_none());
            assert_eq!(std::fs::read(path).unwrap(), b"untouched");
        }

        #[test]
        fn bounds_every_file_even_for_large_unbroken_writes() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("daemon.log");
            std::fs::write(&path, b"").unwrap();
            let mut sink = Sink::open(
                &path,
                RotateConfig {
                    max_bytes: 16,
                    max_backups: 2,
                },
            )
            .unwrap();
            sink.write_all(&(0..100u8).collect::<Vec<_>>()).unwrap();
            drop(sink);
            assert_eq!(std::fs::read(&path).unwrap(), vec![96, 97, 98, 99]);
            assert_eq!(
                std::fs::read(dir.path().join("daemon.log.1")).unwrap(),
                (80..96).collect::<Vec<u8>>()
            );
            assert_eq!(
                std::fs::read(dir.path().join("daemon.log.2")).unwrap(),
                (64..80).collect::<Vec<u8>>()
            );
            assert!(!dir.path().join("daemon.log.3").exists());
        }

        #[test]
        fn legacy_oversize_retains_only_bounded_tail() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("daemon.log");
            std::fs::write(&path, (0..100u8).collect::<Vec<_>>()).unwrap();
            let old_handle = File::open(&path).unwrap();
            let mut sink = Sink::open(
                &path,
                RotateConfig {
                    max_bytes: 16,
                    max_backups: 2,
                },
            )
            .unwrap();
            sink.write_all(b"new").unwrap();
            assert_eq!(old_handle.metadata().unwrap().len(), 0);
            assert_eq!(std::fs::read(&path).unwrap(), b"new");
            assert_eq!(
                std::fs::read(dir.path().join("daemon.log.1")).unwrap(),
                (84..100).collect::<Vec<u8>>()
            );
        }
    }
}

#[cfg(unix)]
pub use unix::{OutputGuard, capture};
