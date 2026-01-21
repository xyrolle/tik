use std::fs::OpenOptions;
use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, Instant};

use crate::{Result, TikError};

#[derive(Debug)]
pub struct LockGuard {
    _file: std::fs::File,
}

#[derive(Debug, Clone, Copy)]
pub enum LockKind {
    Shared,
    Exclusive,
}

pub fn acquire_lock(path: &Path, kind: LockKind, timeout: Duration) -> Result<LockGuard> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|err| TikError::io("open lock file", err))?;

    let start = Instant::now();
    loop {
        let locked = match kind {
            LockKind::Shared => fs4::fs_std::FileExt::try_lock_shared(&file),
            LockKind::Exclusive => fs4::fs_std::FileExt::try_lock_exclusive(&file),
        };

        match locked {
            Ok(()) => {
                return Ok(LockGuard { _file: file });
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                if start.elapsed() >= timeout {
                    return Err(TikError::LockContention(format!(
                        "lock busy: {}",
                        path.display()
                    )));
                }
                sleep(Duration::from_millis(50));
            }
            Err(err) => return Err(TikError::io("lock file", err)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::process::Command;
    use std::time::Duration as StdDuration;
    use tempfile::tempdir;

    #[test]
    fn lock_contention_returns_error() {
        let dir = tempdir().unwrap();
        let lock_path = dir.path().join("repo.lock");
        let ready_path = lock_path.with_extension("ready");

        let exe = std::env::current_exe().unwrap();
        let mut child = Command::new(exe)
            .arg("--exact")
            .arg("lock::tests::lock_helper_holds_lock")
            .arg("--nocapture")
            .env("TIK_LOCK_HELPER", "1")
            .env("TIK_LOCK_PATH", lock_path.to_string_lossy().to_string())
            .spawn()
            .unwrap();

        let start = std::time::Instant::now();
        while !ready_path.exists() {
            if start.elapsed() > StdDuration::from_secs(2) {
                break;
            }
            std::thread::sleep(StdDuration::from_millis(10));
        }

        if !ready_path.exists() {
            let _ = child.kill();
            panic!("lock helper did not signal readiness");
        }

        let err =
            acquire_lock(&lock_path, LockKind::Exclusive, Duration::from_millis(50)).unwrap_err();
        assert!(matches!(err, TikError::LockContention(_)));

        let _ = child.wait();
    }

    #[test]
    fn shared_lock_can_be_acquired() {
        let dir = tempdir().unwrap();
        let lock_path = dir.path().join("repo.lock");

        let _guard = acquire_lock(&lock_path, LockKind::Shared, Duration::from_millis(50)).unwrap();
        assert!(lock_path.exists());
    }

    #[test]
    fn exclusive_lock_can_be_acquired() {
        let dir = tempdir().unwrap();
        let lock_path = dir.path().join("repo.lock");

        let _guard =
            acquire_lock(&lock_path, LockKind::Exclusive, Duration::from_millis(50)).unwrap();
        assert!(lock_path.exists());
    }

    #[test]
    fn lock_helper_holds_lock() {
        if std::env::var("TIK_LOCK_HELPER").is_err() {
            return;
        }

        let path = std::env::var("TIK_LOCK_PATH").unwrap();
        let lock_path = PathBuf::from(path);
        let ready_path = lock_path.with_extension("ready");

        let _guard = acquire_lock(&lock_path, LockKind::Exclusive, Duration::from_secs(2)).unwrap();
        std::fs::write(&ready_path, "ready").unwrap();
        std::thread::sleep(StdDuration::from_millis(200));
    }
}
