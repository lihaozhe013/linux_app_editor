//! Well-known desktop-entry locations across distributions and desktop
//! environments, for the read-only "Open .desktop" browser.
//!
//! The list is fixed and bounded: XDG standard directories, autostart, and
//! the documented homes of Flatpak, Snap and Nix. The scan is user-initiated,
//! non-recursive, capped, and purely informational — no ownership detection
//! and no watching. A directory that does not exist is still listed so users
//! can see where desktop entries would live on their system.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::desktop_entry::fields::read_fields;
use crate::desktop_entry::parser;
use crate::filesystem::{applications_dir, data_home};

pub const MAX_FILES_PER_DIR: usize = 1000;
pub const MAX_TOTAL_FILES: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LocationKind {
    User,
    System,
    Extra,
}

#[derive(Debug, Clone, Serialize)]
pub struct DesktopFileSummary {
    pub path: PathBuf,
    pub file_name: String,
    pub name: Option<String>,
    pub type_: Option<String>,
    pub no_display: bool,
    pub hidden: bool,
    /// Set when the file exists but cannot be read as UTF-8 text.
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocationGroup {
    pub path: PathBuf,
    pub label: String,
    pub kind: LocationKind,
    pub exists: bool,
    /// Per-directory read error; the group stays listed for transparency.
    pub error: Option<String>,
    pub files: Vec<DesktopFileSummary>,
}

pub struct KnownDir {
    pub path: PathBuf,
    pub label: String,
    pub kind: LocationKind,
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|h| !h.is_empty() && Path::new(h).is_absolute())
        .map(PathBuf::from)
}

fn config_home() -> Option<PathBuf> {
    if let Some(value) = std::env::var_os("XDG_CONFIG_HOME") {
        let path = PathBuf::from(&value);
        if !value.is_empty() && path.is_absolute() {
            return Some(path);
        }
    }
    home_dir().map(|h| h.join(".config"))
}

pub fn known_desktop_dirs() -> Vec<KnownDir> {
    let mut dirs: Vec<KnownDir> = Vec::new();

    if let Ok(app_dir) = applications_dir() {
        dirs.push(KnownDir {
            path: app_dir,
            label: "Applications (user)".to_string(),
            kind: LocationKind::User,
        });
    }
    if let Some(config_home) = config_home() {
        dirs.push(KnownDir {
            path: config_home.join("autostart"),
            label: "Autostart (user)".to_string(),
            kind: LocationKind::User,
        });
    }

    let data_dirs = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".to_string());
    for part in data_dirs.split(':').filter(|p| !p.is_empty()) {
        let dir = PathBuf::from(part).join("applications");
        if !dirs.iter().any(|d| d.path == dir) {
            dirs.push(KnownDir {
                path: dir,
                label: "Applications (system)".to_string(),
                kind: LocationKind::System,
            });
        }
    }
    dirs.push(KnownDir {
        path: PathBuf::from("/etc/xdg/autostart"),
        label: "Autostart (system)".to_string(),
        kind: LocationKind::System,
    });

    // Documented homes of packaged app formats, kept as a fixed list.
    if let Ok(data_home) = data_home() {
        dirs.push(KnownDir {
            path: data_home.join("flatpak/exports/share/applications"),
            label: "Flatpak (user)".to_string(),
            kind: LocationKind::Extra,
        });
    }
    dirs.push(KnownDir {
        path: PathBuf::from("/var/lib/flatpak/exports/share/applications"),
        label: "Flatpak (system)".to_string(),
        kind: LocationKind::Extra,
    });
    dirs.push(KnownDir {
        path: PathBuf::from("/var/lib/snapd/desktop/applications"),
        label: "Snap".to_string(),
        kind: LocationKind::Extra,
    });
    if let Some(home) = home_dir() {
        dirs.push(KnownDir {
            path: home.join(".nix-profile/share/applications"),
            label: "Nix profile (user)".to_string(),
            kind: LocationKind::Extra,
        });
    }
    dirs.push(KnownDir {
        path: PathBuf::from("/run/current-system/sw/share/applications"),
        label: "NixOS system profile".to_string(),
        kind: LocationKind::Extra,
    });

    // Manually installed trees under /opt, one level deep, existence-gated.
    if let Ok(entries) = std::fs::read_dir("/opt") {
        let mut opt_dirs: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path().join("share/applications"))
            .filter(|p| p.is_dir())
            .collect();
        opt_dirs.sort();
        for dir in opt_dirs {
            dirs.push(KnownDir {
                path: dir,
                label: "/opt application".to_string(),
                kind: LocationKind::Extra,
            });
        }
    }

    dirs
}

fn summarize(path: &Path, content: &str) -> DesktopFileSummary {
    let file = parser::parse(content);
    let fields = read_fields(&file);
    DesktopFileSummary {
        path: path.to_path_buf(),
        file_name: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        name: fields.name,
        type_: fields.type_,
        no_display: fields.no_display.unwrap_or(false),
        hidden: fields.hidden.unwrap_or(false),
        error: None,
    }
}

fn scan_location(dir: &KnownDir, budget: &mut usize) -> LocationGroup {
    let mut group = LocationGroup {
        path: dir.path.clone(),
        label: dir.label.clone(),
        kind: dir.kind,
        exists: dir.path.is_dir(),
        error: None,
        files: Vec::new(),
    };
    let read_dir = match std::fs::read_dir(&dir.path) {
        Ok(read_dir) => read_dir,
        // Missing directories are expected; they stay listed with exists=false.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return group,
        Err(e) => {
            group.error = Some(e.to_string());
            return group;
        }
    };

    let mut entries: Vec<_> = read_dir.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        if group.files.len() >= MAX_FILES_PER_DIR {
            group.error = Some(format!("listing truncated at {MAX_FILES_PER_DIR} files"));
            break;
        }
        if *budget == 0 {
            group.error = Some(format!("listing truncated (total cap {MAX_TOTAL_FILES})"));
            break;
        }
        if !entry.file_type().is_ok_and(|t| t.is_file()) {
            continue;
        }
        let path = entry.path();
        let is_desktop = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("desktop"));
        if !is_desktop {
            continue;
        }
        *budget -= 1;
        // Unreadable files are still listed (without display name) so the
        // view stays truthful about what exists on disk.
        let summary = match std::fs::read_to_string(&path) {
            Ok(content) => summarize(&path, &content),
            Err(e) => DesktopFileSummary {
                path,
                file_name: entry.file_name().to_string_lossy().into_owned(),
                name: None,
                type_: None,
                no_display: false,
                hidden: false,
                error: Some(e.to_string()),
            },
        };
        group.files.push(summary);
    }
    group
}

pub fn list_desktop_locations() -> Vec<LocationGroup> {
    let mut budget = MAX_TOTAL_FILES;
    known_desktop_dirs()
        .iter()
        .map(|dir| scan_location(dir, &mut budget))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env_lock::{env_lock, set_var};
    use std::sync::MutexGuard;

    /// Holds the shared env lock for the whole test so concurrent tests in
    /// this binary cannot race on process-global environment variables.
    struct EnvGuard {
        _lock: MutexGuard<'static, ()>,
        data_home: PathBuf,
        data_dirs: String,
        config_home: PathBuf,
    }

    impl EnvGuard {
        fn new() -> Self {
            let lock = env_lock();
            let data_home = tempfile::tempdir().unwrap();
            let config_home = tempfile::tempdir().unwrap();
            let system1 = tempfile::tempdir().unwrap();
            let system2 = tempfile::tempdir().unwrap();
            set_var!(
                "XDG_DATA_HOME",
                data_home.path().to_string_lossy().into_owned()
            );
            set_var!(
                "XDG_DATA_DIRS",
                format!("{}:{}", system1.path().display(), system2.path().display())
            );
            set_var!(
                "XDG_CONFIG_HOME",
                config_home.path().to_string_lossy().into_owned()
            );
            EnvGuard {
                _lock: lock,
                data_home: data_home.path().to_path_buf(),
                data_dirs: format!("{}:{}", system1.path().display(), system2.path().display()),
                config_home: config_home.path().to_path_buf(),
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            // The lock guard field is still alive while Drop runs, so
            // restoring here cannot race with other env tests.
            set_var!(
                "XDG_DATA_HOME",
                self.data_home.to_string_lossy().into_owned()
            );
            set_var!("XDG_DATA_DIRS", self.data_dirs.clone());
            set_var!(
                "XDG_CONFIG_HOME",
                self.config_home.to_string_lossy().into_owned()
            );
        }
    }

    fn write_entry(dir: &Path, name: &str, display_name: &str) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join(name);
        std::fs::write(
            &path,
            format!("[Desktop Entry]\nType=Application\nName={display_name}\nExec=foo\n"),
        )
        .unwrap();
        path
    }

    #[test]
    fn groups_user_system_and_autostart_dirs() {
        let guard = EnvGuard::new();
        let user_app = write_entry(
            &guard.data_home.join("applications"),
            "mine.desktop",
            "Mine",
        );
        let autostart = write_entry(&guard.config_home.join("autostart"), "auto.desktop", "Auto");
        let sys1 = guard.data_dirs.split(':').next().unwrap().to_string();
        let sys_app = write_entry(
            &PathBuf::from(&sys1).join("applications"),
            "sys.desktop",
            "Sys",
        );

        let groups = list_desktop_locations();
        let find = |path: &Path| groups.iter().find(|g| g.path == path).unwrap();

        let user_group = find(&guard.data_home.join("applications"));
        assert_eq!(user_group.kind, LocationKind::User);
        assert_eq!(user_group.files.len(), 1);
        assert_eq!(user_group.files[0].name.as_deref(), Some("Mine"));
        assert_eq!(user_group.files[0].path, user_app);

        let auto_group = find(&guard.config_home.join("autostart"));
        assert_eq!(auto_group.files[0].name.as_deref(), Some("Auto"));
        assert_eq!(auto_group.files[0].path, autostart);

        let sys_group = find(&PathBuf::from(&sys1).join("applications"));
        assert_eq!(sys_group.kind, LocationKind::System);
        assert_eq!(sys_group.files[0].name.as_deref(), Some("Sys"));
        assert_eq!(sys_group.files[0].path, sys_app);
    }

    #[test]
    fn non_desktop_and_missing_dirs_are_handled() {
        let guard = EnvGuard::new();
        let app_dir = guard.data_home.join("applications");
        write_entry(&app_dir, "real.desktop", "Real");
        std::fs::write(app_dir.join("notes.txt"), "not a desktop file").unwrap();

        let groups = list_desktop_locations();
        let user_group = groups.iter().find(|g| g.path == app_dir).unwrap();
        assert_eq!(user_group.files.len(), 1);
        assert_eq!(user_group.files[0].file_name, "real.desktop");

        // Missing directories still appear, with no files and no error.
        let snap = groups
            .iter()
            .find(|g| g.path == Path::new("/var/lib/snapd/desktop/applications"))
            .unwrap();
        assert!(!snap.exists);
        assert!(snap.files.is_empty());
        assert!(snap.error.is_none());
    }

    #[test]
    fn per_dir_cap_truncates_with_note() {
        let guard = EnvGuard::new();
        let app_dir = guard.data_home.join("applications");
        std::fs::create_dir_all(&app_dir).unwrap();
        for i in 0..(MAX_FILES_PER_DIR + 5) {
            std::fs::write(
                app_dir.join(format!("f{i:04}.desktop")),
                "[Desktop Entry]\n",
            )
            .unwrap();
        }

        let groups = list_desktop_locations();
        let user_group = groups.iter().find(|g| g.path == app_dir).unwrap();
        assert_eq!(user_group.files.len(), MAX_FILES_PER_DIR);
        assert!(
            user_group
                .error
                .as_deref()
                .is_some_and(|e| e.contains("truncated"))
        );
    }

    #[test]
    fn unreadable_file_is_listed_without_name() {
        let guard = EnvGuard::new();
        let app_dir = guard.data_home.join("applications");
        std::fs::create_dir_all(&app_dir).unwrap();
        // Invalid UTF-8, so read_to_string fails while the file exists.
        let path = app_dir.join("bad.desktop");
        std::fs::write(&path, [0xff, 0xfe, b'x']).unwrap();

        let groups = list_desktop_locations();
        let user_group = groups.iter().find(|g| g.path == app_dir).unwrap();
        let bad = user_group
            .files
            .iter()
            .find(|f| f.file_name == "bad.desktop")
            .expect("unreadable file should still be listed");
        assert!(bad.error.is_some());
        assert_eq!(bad.name, None);
    }
}
