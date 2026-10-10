//! Tauri command handlers. Thin wrappers: all desktop-entry logic lives in
//! the `desktop_entry` and helper modules; the frontend gets no arbitrary
//! filesystem access.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::appimage;
use crate::desktop_entry::exec::ExecSpec;
use crate::desktop_entry::fields::{
    FieldPatch, KNOWN_KEYS, SPEC_VERSION_VALUE, apply_managed_markers, apply_patch, escape_icon,
    escape_string, read_fields,
};
use crate::desktop_entry::model::DesktopFile;
use crate::desktop_entry::{parser, serializer, validation};
use crate::error::{AppError, AppResult};
use crate::filesystem::{applications_dir, atomic_write, backup_file};
use crate::icons::{IconCandidate, find_nearby_icons as scan_nearby_icons};

#[derive(Debug, Deserialize)]
pub struct CreateLauncherRequest {
    pub filename_stem: String,
    pub name: String,
    pub exec: ExecSpec,
    pub icon: Option<String>,
    pub working_directory: Option<String>,
    pub terminal: bool,
    pub comment: Option<String>,
    pub categories: Option<Vec<String>>,
    /// Must be set to true by the frontend after the user confirmed
    /// overwriting an existing file.
    #[serde(default)]
    pub overwrite: bool,
}

#[derive(Debug, Serialize)]
pub struct CreateOutcome {
    pub path: PathBuf,
    pub warnings: Vec<validation::ValidationItem>,
}

#[tauri::command]
pub fn create_launcher(req: CreateLauncherRequest) -> AppResult<CreateOutcome> {
    validation::validate_filename_stem(&req.filename_stem).map_err(AppError::InvalidFilename)?;

    let warnings = validation::validate_entry(
        Some(&req.name),
        Some(&req.exec),
        req.icon.as_deref(),
        req.working_directory.as_deref(),
        Some("Application"),
    );
    if let Some(err) = warnings
        .iter()
        .find(|i| i.severity == validation::Severity::Error)
    {
        return Err(AppError::Validation(err.message.clone()));
    }

    let dir = applications_dir()?;
    let target = dir.join(format!("{}.desktop", req.filename_stem));
    if target.exists() && !req.overwrite {
        return Err(AppError::AlreadyExists(target));
    }

    let mut file = DesktopFile::new_entry_file();
    file.set_raw("Version", SPEC_VERSION_VALUE);
    file.set_raw("Type", "Application");
    file.set_raw("Name", &escape_string(req.name.trim()));
    if let Some(comment) = non_empty(&req.comment) {
        file.set_raw("Comment", &escape_string(comment));
    }
    file.set_raw("Exec", &crate::desktop_entry::exec::build_exec(&req.exec));
    if let Some(icon) = non_empty(&req.icon) {
        file.set_raw("Icon", &escape_icon(icon));
    }
    if let Some(dir) = non_empty(&req.working_directory) {
        file.set_raw("Path", &escape_string(dir));
    }
    file.set_raw("Terminal", if req.terminal { "true" } else { "false" });
    if let Some(categories) = req.categories.as_ref().filter(|c| !c.is_empty()) {
        let mut joined = String::new();
        for c in categories {
            joined.push_str(c);
            joined.push(';');
        }
        file.set_raw("Categories", &joined);
    }
    apply_managed_markers(&mut file);

    atomic_write(&target, serializer::serialize(&file).as_bytes())?;
    Ok(CreateOutcome {
        path: target,
        warnings,
    })
}

#[derive(Debug, Serialize)]
pub struct EntryMeta {
    pub desktop_actions: Vec<String>,
    pub locale_key_count: usize,
    pub unknown_key_count: usize,
}

#[derive(Debug, Serialize)]
pub struct OpenedEntry {
    pub path: PathBuf,
    pub fields: crate::desktop_entry::fields::KnownFields,
    pub meta: EntryMeta,
}

#[tauri::command]
pub fn open_desktop_entry(path: String) -> AppResult<OpenedEntry> {
    let path = PathBuf::from(path);
    let content = std::fs::read_to_string(&path).map_err(|e| AppError::io(&path, e))?;
    let file = parser::parse(&content);
    Ok(OpenedEntry {
        path,
        fields: read_fields(&file),
        meta: EntryMeta {
            desktop_actions: file.desktop_action_names(),
            locale_key_count: file.locale_key_count(),
            unknown_key_count: file.unknown_key_count(KNOWN_KEYS),
        },
    })
}

#[derive(Debug, Deserialize)]
pub struct SaveRequest {
    pub path: String,
    /// Save As target; defaults to the original path.
    pub new_path: Option<String>,
    pub fields: FieldPatch,
    #[serde(default)]
    pub make_backup: bool,
}

#[derive(Debug, Serialize)]
pub struct SaveOutcome {
    pub path: PathBuf,
    pub warnings: Vec<validation::ValidationItem>,
}

#[tauri::command]
pub fn save_desktop_entry(req: SaveRequest) -> AppResult<SaveOutcome> {
    let source = PathBuf::from(&req.path);
    let target = req
        .new_path
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| source.clone());

    // The file on disk is the source of truth: re-read it so external
    // changes to parts the UI never touched are kept.
    let mut file = match std::fs::read_to_string(&source) {
        Ok(content) => parser::parse(&content),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DesktopFile::new_entry_file(),
        Err(e) => return Err(AppError::io(&source, e)),
    };

    apply_patch(&mut file, &req.fields);

    let after = read_fields(&file);
    let warnings = validation::validate_entry(
        after.name.as_deref(),
        after.exec.as_ref(),
        after.icon.as_deref(),
        after.path.as_deref(),
        after.type_.as_deref(),
    );
    if let Some(err) = warnings
        .iter()
        .find(|i| i.severity == validation::Severity::Error)
    {
        return Err(AppError::Validation(err.message.clone()));
    }

    if req.make_backup && target.exists() {
        backup_file(&target)?;
    }
    atomic_write(&target, serializer::serialize(&file).as_bytes())?;
    Ok(SaveOutcome {
        path: target,
        warnings,
    })
}

#[tauri::command]
pub fn delete_launcher(path: String) -> AppResult<()> {
    let path = PathBuf::from(path);
    let meta = std::fs::symlink_metadata(&path).map_err(|e| AppError::io(&path, e))?;
    if !meta.is_file() {
        return Err(AppError::io(
            &path,
            std::io::Error::other("refusing to delete: not a regular file"),
        ));
    }
    if !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("desktop"))
    {
        return Err(AppError::InvalidFilename(
            path.to_string_lossy().into_owned(),
        ));
    }
    std::fs::remove_file(&path).map_err(|e| AppError::io(&path, e))
}

#[derive(Debug, Serialize)]
pub struct ManagedItem {
    pub file_name: String,
    pub path: PathBuf,
    pub name: Option<String>,
    pub icon: Option<String>,
    pub exec: Option<String>,
    pub terminal: bool,
    pub no_display: bool,
    pub hidden: bool,
    pub executable_missing: bool,
}

#[tauri::command]
pub fn list_managed_launchers() -> AppResult<Vec<ManagedItem>> {
    let dir = applications_dir()?;
    let Ok(read_dir) = std::fs::read_dir(&dir) else {
        return Ok(Vec::new());
    };

    let mut items = Vec::new();
    let mut entries: Vec<_> = read_dir.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let is_desktop = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("desktop"));
        if !is_desktop || !entry.file_type().is_ok_and(|t| t.is_file()) {
            continue;
        }
        // Unreadable files cannot carry the managed marker; skip them.
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let file = parser::parse(&content);
        let fields = read_fields(&file);
        if !fields.managed {
            continue;
        }
        let executable_missing = fields.exec.as_ref().is_some_and(|exec| {
            exec.executable.contains('/') && !Path::new(&exec.executable).exists()
        });
        items.push(ManagedItem {
            file_name: entry.file_name().to_string_lossy().into_owned(),
            path,
            name: crate::desktop_entry::fields::display_name(&file),
            icon: fields.icon,
            exec: fields.exec.map(|e| e.executable),
            terminal: fields.terminal.unwrap_or(false),
            no_display: fields.no_display.unwrap_or(false),
            hidden: fields.hidden.unwrap_or(false),
            executable_missing,
        });
    }
    Ok(items)
}

/// Read-only listing of well-known desktop-entry directories (XDG standard,
/// autostart, Flatpak, Snap, Nix, /opt) with one level of file summaries.
#[tauri::command]
pub fn list_desktop_locations() -> Vec<crate::locations::LocationGroup> {
    crate::locations::list_desktop_locations()
}

#[derive(Debug, Deserialize)]
pub struct ValidateFormRequest {
    pub name: Option<String>,
    pub exec: Option<ExecSpec>,
    pub icon: Option<String>,
    pub working_directory: Option<String>,
    pub type_: Option<String>,
}

#[tauri::command]
pub fn validate_form(req: ValidateFormRequest) -> Vec<validation::ValidationItem> {
    validation::validate_entry(
        req.name.as_deref(),
        req.exec.as_ref(),
        req.icon.as_deref(),
        req.working_directory.as_deref(),
        req.type_.as_deref(),
    )
}

#[derive(Debug, Serialize)]
pub struct DfvResult {
    pub available: bool,
    pub exit_code: Option<i32>,
    pub output: String,
}

/// Optional integration with desktop-file-utils. Not a hard dependency:
/// `available: false` when the binary is not on $PATH.
#[tauri::command]
pub fn run_desktop_file_validate(path: String) -> DfvResult {
    const TIMEOUT: Duration = Duration::from_secs(10);

    let Ok(exe) = which::which("desktop-file-validate") else {
        return DfvResult {
            available: false,
            exit_code: None,
            output: String::new(),
        };
    };

    let mut command = std::process::Command::new(exe);
    command
        .arg(&path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let Ok(mut child) = command.spawn() else {
        return DfvResult {
            available: false,
            exit_code: None,
            output: String::new(),
        };
    };

    // dfv output is tiny; reading after exit cannot deadlock in practice.
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let deadline = Instant::now() + TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() > deadline => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(_) => break None,
        }
    };

    let mut output = String::new();
    if let Some(io) = stdout.as_mut() {
        let _ = io.read_to_string(&mut output);
    }
    if let Some(io) = stderr.as_mut() {
        let _ = io.read_to_string(&mut output);
    }
    if status.is_none() {
        output.push_str("\n(desktop-file-validate timed out)");
    }
    DfvResult {
        available: true,
        exit_code: status.and_then(|s| s.code()),
        output,
    }
}

#[tauri::command]
pub fn find_nearby_icons(executable: String) -> Vec<IconCandidate> {
    scan_nearby_icons(Path::new(&executable))
}

/// Icon and metadata extraction for AppImage executables. The icon is copied
/// into the app-owned icons directory so the entry keeps working if the
/// AppImage moves or is replaced by an updated download with the same name.
#[tauri::command]
pub fn extract_appimage_metadata(path: String) -> AppResult<appimage::Extracted> {
    appimage::extract(Path::new(&path))
}

#[derive(Debug, Serialize)]
pub struct PathStatus {
    pub exists: bool,
    pub is_file: bool,
    pub is_dir: bool,
    pub is_executable: bool,
}

#[tauri::command]
pub fn stat_path(path: String) -> PathStatus {
    let meta = std::fs::metadata(&path);
    match meta {
        Ok(m) => {
            #[cfg(unix)]
            let executable = {
                use std::os::unix::fs::PermissionsExt;
                m.is_file() && m.permissions().mode() & 0o111 != 0
            };
            #[cfg(not(unix))]
            let executable = m.is_file();
            PathStatus {
                exists: true,
                is_file: m.is_file(),
                is_dir: m.is_dir(),
                is_executable: executable,
            }
        }
        Err(_) => PathStatus {
            exists: false,
            is_file: false,
            is_dir: false,
            is_executable: false,
        },
    }
}

fn non_empty(value: &Option<String>) -> Option<&str> {
    value.as_deref().filter(|v| !v.trim().is_empty())
}
