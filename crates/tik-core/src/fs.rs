use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use tempfile::NamedTempFile;

use crate::{Result, TikError};

pub fn ensure_dir(path: &Path) -> Result<()> {
    reject_symlink_components(path)?;
    fs::create_dir_all(path).map_err(|err| TikError::io("create directory", err))
}

pub fn read_to_string(path: &Path) -> Result<String> {
    fs::read_to_string(path).map_err(|err| TikError::io("read file", err))
}

pub fn write_string_atomic(path: &Path, contents: &str) -> Result<()> {
    atomic_write(path, contents.as_bytes())
}

pub fn atomic_write(path: &Path, contents: &[u8]) -> Result<()> {
    reject_symlink_components(path)?;
    let parent = path
        .parent()
        .ok_or_else(|| TikError::internal("missing parent directory"))?;

    let mut tmp =
        NamedTempFile::new_in(parent).map_err(|err| TikError::io("create temp file", err))?;
    tmp.write_all(contents)
        .map_err(|err| TikError::io("write temp file", err))?;
    tmp.as_file()
        .sync_all()
        .map_err(|err| TikError::io("sync temp file", err))?;

    let tmp_path = tmp.into_temp_path();
    if let Err(err) = atomic_rename(tmp_path.as_ref(), path) {
        let _ = tmp_path.close();
        return Err(TikError::io("rename temp file", err));
    }

    sync_dir(parent)?;
    Ok(())
}

pub fn append_string_atomic(path: &Path, contents: &str) -> Result<()> {
    append_bytes_atomic(path, contents.as_bytes())
}

pub fn append_bytes_atomic(path: &Path, contents: &[u8]) -> Result<()> {
    reject_symlink_components(path)?;
    let parent = path
        .parent()
        .ok_or_else(|| TikError::internal("missing parent directory"))?;

    let mut tmp =
        NamedTempFile::new_in(parent).map_err(|err| TikError::io("create temp file", err))?;

    if path.exists() {
        let mut existing =
            fs::File::open(path).map_err(|err| TikError::io("open file for append", err))?;
        std::io::copy(&mut existing, &mut tmp)
            .map_err(|err| TikError::io("copy file for append", err))?;
    }

    tmp.write_all(contents)
        .map_err(|err| TikError::io("write temp file", err))?;
    tmp.as_file()
        .sync_all()
        .map_err(|err| TikError::io("sync temp file", err))?;

    let tmp_path = tmp.into_temp_path();
    if let Err(err) = atomic_rename(tmp_path.as_ref(), path) {
        let _ = tmp_path.close();
        return Err(TikError::io("rename temp file", err));
    }

    sync_dir(parent)?;
    Ok(())
}

pub fn atomic_replace_path(from: &Path, to: &Path) -> Result<()> {
    reject_symlink_components(to)?;
    let parent = to
        .parent()
        .ok_or_else(|| TikError::internal("missing parent directory"))?;

    atomic_rename(from, to).map_err(|err| TikError::io("rename temp file", err))?;
    sync_dir(parent)?;
    Ok(())
}

pub fn remove_file_safe(path: &Path) -> Result<()> {
    reject_symlink_components(path)?;
    if path.exists() {
        fs::remove_file(path).map_err(|err| TikError::io("remove file", err))?;
    }
    Ok(())
}

fn sync_dir(path: &Path) -> Result<()> {
    let dir = fs::File::open(path).map_err(|err| TikError::io("open dir for sync", err))?;
    dir.sync_all().map_err(|err| TikError::io("sync dir", err))
}

fn reject_symlink_components(path: &Path) -> Result<()> {
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(TikError::Permission(format!(
            "path traversal not allowed: {}",
            path.display()
        )));
    }

    let mut current = PathBuf::new();
    let temp_dir = std::env::temp_dir();
    for component in path.components() {
        current.push(component.as_os_str());
        if temp_dir.starts_with(&current) {
            continue;
        }
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    return Err(TikError::Permission(format!(
                        "symlink not allowed: {}",
                        current.display()
                    )));
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(TikError::io("read path metadata", err)),
        }
    }

    Ok(())
}

#[cfg(not(windows))]
fn atomic_rename(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::rename(from, to)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn atomic_write_writes_contents() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("file.txt");
        write_string_atomic(&path, "hello").unwrap();
        let contents = read_to_string(&path).unwrap();
        assert_eq!(contents, "hello");
    }

    #[test]
    fn append_string_atomic_appends_contents() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("file.txt");
        write_string_atomic(&path, "hello").unwrap();
        append_string_atomic(&path, "\nworld").unwrap();
        let contents = read_to_string(&path).unwrap();
        assert_eq!(contents, "hello\nworld");
    }

    #[cfg(unix)]
    #[test]
    fn write_string_atomic_rejects_symlink() {
        use std::os::unix::fs::symlink;

        let dir = tempdir().unwrap();
        let target = dir.path().join("target.txt");
        write_string_atomic(&target, "ok").unwrap();
        let link = dir.path().join("link.txt");
        symlink(&target, &link).unwrap();

        let err = write_string_atomic(&link, "nope").unwrap_err();
        assert!(matches!(err, TikError::Permission(_)));
    }

    #[cfg(unix)]
    #[test]
    fn write_string_atomic_rejects_symlink_parent() {
        use std::os::unix::fs::symlink;

        let dir = tempdir().unwrap();
        let tik_dir = dir.path().join(".tik");
        fs::create_dir_all(&tik_dir).unwrap();
        let target_dir = tik_dir.join("real");
        fs::create_dir_all(&target_dir).unwrap();
        let link_dir = tik_dir.join("link");
        symlink(&target_dir, &link_dir).unwrap();

        let path = link_dir.join("file.txt");
        let err = write_string_atomic(&path, "nope").unwrap_err();
        assert!(matches!(err, TikError::Permission(_)));
    }
}

#[cfg(windows)]
fn atomic_rename(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let from_w: Vec<u16> = from
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let to_w: Vec<u16> = to
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let result = unsafe {
        MoveFileExW(
            from_w.as_ptr(),
            to_w.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };

    if result == 0 {
        return Err(std::io::Error::last_os_error());
    }

    Ok(())
}
