//! Filesystem helpers: explicit XDG resolution and atomic writes.
//!
//! No privilege escalation of any kind: permission errors are reported to
//! the caller as-is.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};

/// `$XDG_DATA_HOME` when set to an absolute path, otherwise `$HOME/.local/share`
/// per the XDG Base Directory Specification (empty values fall back too).
pub fn data_home() -> AppResult<PathBuf> {
    if let Some(value) = std::env::var_os("XDG_DATA_HOME") {
        let path = PathBuf::from(&value);
        if !value.is_empty() && path.is_absolute() {
            return Ok(path);
        }
    }
    let home = std::env::var_os("HOME")
        .filter(|h| !h.is_empty() && Path::new(h).is_absolute())
        .ok_or(AppError::HomeNotFound)?;
    Ok(PathBuf::from(home).join(".local").join("share"))
}

pub fn applications_dir() -> AppResult<PathBuf> {
    Ok(data_home()?.join("applications"))
}

/// Write via temp file + fsync + rename in the target directory so a crash
/// never leaves a truncated file behind. Missing parent directories are
/// created (the XDG applications dir may not exist yet on first run).
pub fn atomic_write(path: &Path, contents: &[u8]) -> AppResult<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(|| {
            AppError::io(
                path,
                std::io::Error::other(format!("{} has no parent directory", path.display())),
            )
        })?;

    fs::create_dir_all(parent).map_err(|e| AppError::io(parent, e))?;

    let mut tmp = tempfile::Builder::new()
        .prefix(".linux-app-editor-tmp-")
        .tempfile_in(parent)
        .map_err(|e| AppError::io(parent, e))?;

    let write_res = tmp
        .write_all(contents)
        .and_then(|()| tmp.as_file().sync_all());
    if let Err(e) = write_res {
        // Dropping `tmp` removes the temp file; the original stays intact.
        return Err(AppError::io(path, e));
    }

    tmp.persist(path)
        .map(|_| ())
        .map_err(|e| AppError::io(path, e.error))?;

    // Best-effort directory fsync so the rename itself is durable.
    if let Ok(dir) = fs::File::open(parent) {
        let _ = dir.sync_all();
    }
    Ok(())
}

/// Copy `path` to `path.bak`. Returns false when the source does not exist.
pub fn backup_file(path: &Path) -> AppResult<bool> {
    if !path.exists() {
        return Ok(false);
    }
    let mut backup = path.as_os_str().to_os_string();
    backup.push(".bak");
    let backup = PathBuf::from(backup);
    fs::copy(path, &backup)
        .map(|_| true)
        .map_err(|e| AppError::io(&backup, e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env_lock::{env_lock, remove_var, set_var};
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn absolute_xdg_data_home_wins() {
        let _guard = env_lock();
        let dir = tempfile::tempdir().unwrap();
        set_var!("XDG_DATA_HOME", dir.path());
        assert_eq!(data_home().unwrap(), dir.path());
        assert_eq!(applications_dir().unwrap(), dir.path().join("applications"));
        remove_var!("XDG_DATA_HOME");
    }

    #[test]
    fn relative_or_empty_xdg_falls_back_to_home() {
        let _guard = env_lock();
        let home = tempfile::tempdir().unwrap();
        set_var!("HOME", home.path());

        set_var!("XDG_DATA_HOME", "relative/path");
        assert_eq!(data_home().unwrap(), home.path().join(".local/share"));

        set_var!("XDG_DATA_HOME", "");
        assert_eq!(data_home().unwrap(), home.path().join(".local/share"));
        remove_var!("XDG_DATA_HOME");
    }

    #[test]
    fn missing_home_and_xdg_is_an_error() {
        let _guard = env_lock();
        let home = tempfile::tempdir().unwrap();
        set_var!("HOME", home.path());
        remove_var!("XDG_DATA_HOME");
        remove_var!("HOME");
        let err = data_home().unwrap_err();
        assert_eq!(err.code(), "home-not-found");
        set_var!("HOME", home.path());
    }

    #[test]
    fn atomic_write_creates_parents_and_content() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("a/b/c/foo.desktop");
        atomic_write(&target, b"[Desktop Entry]\n").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"[Desktop Entry]\n");
    }

    #[test]
    fn atomic_write_overwrites_and_leaves_no_temp_files() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("foo.desktop");
        atomic_write(&target, b"old").unwrap();
        atomic_write(&target, b"new content").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"new content");
        let entries: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(entries, vec!["foo.desktop"]);
    }

    #[test]
    fn permission_failure_is_reported() {
        if is_root() {
            return; // chmod does not stop root
        }
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("foo.desktop");
        atomic_write(&target, b"x").unwrap();
        // Atomic replace needs directory write permission, not file
        // permission, so a read-only directory is what must be rejected.
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o555)).unwrap();
        let err = atomic_write(&target, b"y").unwrap_err();
        assert_eq!(err.code(), "permission-denied");
        assert_eq!(fs::read(&target).unwrap(), b"x");
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn backup_copies_when_present() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("foo.desktop");
        assert!(!backup_file(&target).unwrap());
        fs::write(&target, "v1").unwrap();
        assert!(backup_file(&target).unwrap());
        assert_eq!(fs::read(dir.path().join("foo.desktop.bak")).unwrap(), b"v1");
    }

    fn is_root() -> bool {
        std::process::Command::new("id")
            .arg("-u")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "0")
            .unwrap_or(false)
    }
}
