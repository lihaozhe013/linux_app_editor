use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

pub mod templates;
pub mod unit_text;

const MAX_UNITS: usize = 500;
const EDITOR_DROP_IN: &str = "90-linux-app-editor.conf";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    User,
    System,
}

impl Scope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::System => "system",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceItem {
    pub unit_name: String,
    pub path: PathBuf,
    pub scope: Scope,
    pub origin: String,
    pub writable: bool,
    pub is_vendor: bool,
    pub masked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceDocument {
    pub scope: Scope,
    pub unit_name: String,
    pub source_path: PathBuf,
    pub target_path: PathBuf,
    pub target_mode: Option<u32>,
    pub contents: String,
    pub source_contents: String,
    pub expected_contents: Option<String>,
    pub is_drop_in: bool,
    pub is_new: bool,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub severity: Severity,
    pub line: Option<usize>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveOutcome {
    pub path: PathBuf,
    pub target_mode: Option<u32>,
    pub staged_path: Option<PathBuf>,
    pub install_command: Option<String>,
    pub reload_command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyOutcome {
    pub available: bool,
    pub exit_code: Option<i32>,
    pub output: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("{0}: {1}")]
    Io(PathBuf, #[source] io::Error),
    #[error("invalid service unit name: {0}")]
    InvalidUnitName(String),
    #[error(
        "cannot determine the user configuration directory: XDG_CONFIG_HOME and HOME are unset or relative"
    )]
    HomeNotFound,
    #[error("service file changed on disk; reopen it before saving")]
    ChangedOnDisk,
    #[error("refusing to write through a symbolic link: {0}")]
    Symlink(PathBuf),
    #[error(
        "system scope writes require administrator permissions; rerun with: sudo systemd-service-editor --scope system"
    )]
    SystemPermissionRequired,
    #[error("unit is masked and cannot be edited: {0}")]
    Masked(PathBuf),
    #[error("service draft contains errors; fix them before saving")]
    InvalidDraft,
    #[error("{0} has repeated or continued values; edit it in raw mode")]
    AmbiguousFormField(String),
    #[error("systemd-analyze is unavailable")]
    AnalyzerUnavailable,
    #[error("unknown service template: {0}")]
    UnknownTemplate(String),
    #[error("enter the absolute path of the program the service should run")]
    EmptyExecPath,
    #[error("ExecStart must be an absolute path: {0}")]
    RelativeExecPath(String),
    #[error("program not found: {0}")]
    ExecNotFound(PathBuf),
    #[error("ExecStart must point to a file, not a directory or device: {0}")]
    ExecNotAFile(PathBuf),
    #[error("program is not executable: {0}")]
    ExecNotExecutable(PathBuf),
}

fn io_error(path: impl Into<PathBuf>, error: io::Error) -> ServiceError {
    ServiceError::Io(path.into(), error)
}

fn config_home() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|p| Path::new(p).is_absolute())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|p| Path::new(p).is_absolute())
                .map(|p| PathBuf::from(p).join(".config"))
        })
}

pub fn state_home() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .filter(|p| Path::new(p).is_absolute())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|p| Path::new(p).is_absolute())
                .map(|p| PathBuf::from(p).join(".local/state"))
        })
        .unwrap_or_else(std::env::temp_dir)
}

pub fn systemd_paths(scope: Scope) -> Vec<PathBuf> {
    let scope_arg = match scope {
        Scope::User => "--user",
        Scope::System => "--system",
    };
    if let Ok(output) = Command::new("systemd-analyze")
        .args([scope_arg, "unit-paths"])
        .output()
        && output.status.success()
    {
        let paths: Vec<PathBuf> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .filter(|path| Path::new(path).is_absolute())
            .map(PathBuf::from)
            .collect();
        if !paths.is_empty() {
            return paths;
        }
    }

    match scope {
        Scope::System => [
            "/etc/systemd/system",
            "/run/systemd/system",
            "/usr/local/lib/systemd/system",
            "/usr/lib/systemd/system",
            "/lib/systemd/system",
        ]
        .into_iter()
        .map(PathBuf::from)
        .collect(),
        Scope::User => {
            let mut paths = Vec::new();
            if let Some(config_home) = config_home() {
                paths.push(config_home.join("systemd/user"));
            }
            if let Some(data_home) =
                std::env::var_os("XDG_DATA_HOME").filter(|p| Path::new(p).is_absolute())
            {
                paths.push(PathBuf::from(data_home).join("systemd/user"));
            } else if let Some(home) = std::env::var_os("HOME") {
                paths.push(PathBuf::from(home).join(".local/share/systemd/user"));
            }
            paths.extend(
                [
                    "/etc/systemd/user",
                    "/run/systemd/user",
                    "/usr/local/lib/systemd/user",
                    "/usr/lib/systemd/user",
                    "/usr/share/systemd/user",
                ]
                .into_iter()
                .map(PathBuf::from),
            );
            paths
        }
    }
}

fn is_user_config(path: &Path) -> bool {
    config_home().is_some_and(|home| path_is_within(path, &home.join("systemd/user")))
}

fn is_system_config(path: &Path) -> bool {
    path_is_within(path, Path::new("/etc/systemd/system"))
}

fn path_is_within(path: &Path, directory: &Path) -> bool {
    if path.starts_with(directory) {
        return true;
    }
    fs::canonicalize(path)
        .ok()
        .zip(fs::canonicalize(directory).ok())
        .is_some_and(|(resolved_path, resolved_directory)| {
            resolved_path.starts_with(resolved_directory)
        })
}

fn is_config_path(path: &Path, scope: Scope) -> bool {
    match scope {
        Scope::User => is_user_config(path),
        Scope::System => is_system_config(path),
    }
}

fn unit_name_from(path: &Path) -> Option<&str> {
    let name = path.file_name()?.to_str()?;
    (name.ends_with(".service") && validate_unit_name(name).is_ok()).then_some(name)
}

pub fn validate_unit_name(name: &str) -> Result<(), ServiceError> {
    let base = name.strip_suffix(".service").unwrap_or("");
    let bytes = base.as_bytes();
    let mut index = 0;
    let mut valid = !base.is_empty() && base != "." && base != "..";
    while valid && index < bytes.len() {
        if bytes[index].is_ascii_alphanumeric() || b":_.@-".contains(&bytes[index]) {
            index += 1;
        } else if bytes[index] == b'\\'
            && index + 3 < bytes.len()
            && bytes[index + 1] == b'x'
            && bytes[index + 2].is_ascii_hexdigit()
            && bytes[index + 3].is_ascii_hexdigit()
        {
            index += 4;
        } else {
            valid = false;
        }
    }
    if valid {
        Ok(())
    } else {
        Err(ServiceError::InvalidUnitName(name.to_string()))
    }
}

pub fn list_services(scope: Scope) -> Result<Vec<ServiceItem>, ServiceError> {
    list_services_from_paths(scope, &systemd_paths(scope))
}

fn list_services_from_paths(
    scope: Scope,
    directories: &[PathBuf],
) -> Result<Vec<ServiceItem>, ServiceError> {
    let mut found = BTreeMap::<String, ServiceItem>::new();
    for directory in directories {
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let Some(unit_name) = unit_name_from(&path) else {
                continue;
            };
            let Ok(meta) = fs::symlink_metadata(&path) else {
                continue;
            };
            if !meta.is_file() && !meta.file_type().is_symlink() {
                continue;
            }
            let masked = meta.file_type().is_symlink()
                && fs::read_link(&path).is_ok_and(|target| target == Path::new("/dev/null"));
            let writable = is_config_path(&path, scope)
                && !meta.file_type().is_symlink()
                && (scope != Scope::System || effective_uid_is_root());
            found
                .entry(unit_name.to_string())
                .or_insert_with(|| ServiceItem {
                    unit_name: unit_name.to_string(),
                    path: path.clone(),
                    scope,
                    origin: directory.display().to_string(),
                    writable,
                    is_vendor: !is_config_path(&path, scope),
                    masked,
                });
        }
        if found.len() >= MAX_UNITS {
            break;
        }
    }
    Ok(found.into_values().collect())
}

pub fn open_service(path: &Path, scope: Scope) -> Result<ServiceDocument, ServiceError> {
    open_service_with_paths(path, scope, &systemd_paths(scope))
}

fn open_service_with_paths(
    path: &Path,
    scope: Scope,
    unit_paths: &[PathBuf],
) -> Result<ServiceDocument, ServiceError> {
    let unit_name = unit_name_from(path)
        .ok_or_else(|| ServiceError::InvalidUnitName(path.display().to_string()))?
        .to_string();
    let meta = fs::symlink_metadata(path).map_err(|error| io_error(path, error))?;
    if meta.file_type().is_symlink()
        && fs::read_link(path).is_ok_and(|target| target == Path::new("/dev/null"))
    {
        return Err(ServiceError::Masked(path.to_path_buf()));
    }
    let source_contents = fs::read_to_string(path).map_err(|error| io_error(path, error))?;
    let is_known_path = unit_paths
        .iter()
        .any(|directory| path_is_within(path, directory));
    let is_drop_in = is_known_path && !is_config_path(path, scope);
    let target_path = if is_drop_in {
        config_dir(scope)?.join(format!("{unit_name}.d/{EDITOR_DROP_IN}"))
    } else {
        path.to_path_buf()
    };
    let (contents, expected_contents, is_new) = if is_drop_in {
        match fs::read_to_string(&target_path) {
            Ok(contents) => (contents.clone(), Some(contents), false),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                (String::from("[Service]\n"), None, true)
            }
            Err(error) => return Err(io_error(&target_path, error)),
        }
    } else {
        (
            source_contents.clone(),
            Some(source_contents.clone()),
            false,
        )
    };
    let target_mode = mode_at(&target_path)?;
    let diagnostics = validate(&contents, is_drop_in);
    Ok(ServiceDocument {
        scope,
        unit_name,
        source_path: path.to_path_buf(),
        target_path,
        target_mode,
        contents,
        source_contents,
        expected_contents,
        is_drop_in,
        is_new,
        diagnostics,
    })
}

pub fn create_service(scope: Scope, unit_name: &str) -> Result<ServiceDocument, ServiceError> {
    validate_unit_name(unit_name)?;
    let install_target = match scope {
        Scope::User => "default.target",
        Scope::System => "multi-user.target",
    };
    let contents = format!(
        "[Unit]\nDescription=New service\n\n[Service]\nType=simple\nExecStart=/usr/bin/true\n\n[Install]\nWantedBy={install_target}\n"
    );
    new_document(scope, unit_name, contents)
}

/// Build a new unit from a template. The template decides the install scope,
/// so the document is created in the template's scope rather than a caller-
/// supplied one.
pub fn create_service_from_template(
    template_id: &str,
    unit_name: &str,
    exec_path: &str,
) -> Result<ServiceDocument, ServiceError> {
    validate_unit_name(unit_name)?;
    let template = templates::find(template_id)
        .ok_or_else(|| ServiceError::UnknownTemplate(template_id.to_string()))?;
    let contents = templates::render(template_id, unit_name, exec_path)?;
    new_document(template.scope, unit_name, contents)
}

fn new_document(
    scope: Scope,
    unit_name: &str,
    contents: String,
) -> Result<ServiceDocument, ServiceError> {
    let target_path = config_dir(scope)?.join(unit_name);
    Ok(ServiceDocument {
        scope,
        unit_name: unit_name.to_string(),
        source_path: target_path.clone(),
        target_mode: None,
        target_path,
        source_contents: String::new(),
        expected_contents: None,
        diagnostics: validate(&contents, false),
        contents,
        is_drop_in: false,
        is_new: true,
    })
}

pub fn validate(contents: &str, is_drop_in: bool) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut section = String::new();
    let mut has_service = false;
    let mut has_exec_start = false;
    let mut continued = false;
    for (index, line) in contents.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
            continue;
        }
        if trimmed.starts_with('[') {
            if !trimmed.ends_with(']') || trimmed.len() < 3 {
                diagnostics.push(Diagnostic {
                    severity: Severity::Error,
                    line: Some(line_number),
                    message: "Malformed section header".to_string(),
                });
                continue;
            }
            section = trimmed[1..trimmed.len() - 1].to_string();
            has_service |= section == "Service";
            continue;
        }
        if !continued && !trimmed.contains('=') {
            diagnostics.push(Diagnostic {
                severity: Severity::Error,
                line: Some(line_number),
                message: "Expected a KEY=VALUE assignment or a continued line".to_string(),
            });
            continue;
        }
        if section == "Service" && trimmed.starts_with("ExecStart=") {
            has_exec_start |= trimmed != "ExecStart=";
        }
        continued = line.trim_end().ends_with('\\');
    }
    if !is_drop_in && !has_service {
        diagnostics.push(Diagnostic {
            severity: Severity::Error,
            line: None,
            message: "A service unit must contain a [Service] section".to_string(),
        });
    }
    if !is_drop_in && has_service && !has_exec_start {
        diagnostics.push(Diagnostic {
            severity: Severity::Warning,
            line: None,
            message:
                "No non-empty ExecStart= directive was found; check service type and stop behavior"
                    .to_string(),
        });
    }
    diagnostics
}

pub fn save_service(
    document: &ServiceDocument,
    contents: &str,
    make_backup: bool,
    stage_system: bool,
) -> Result<SaveOutcome, ServiceError> {
    let diagnostics = validate(contents, document.is_drop_in);
    if diagnostics
        .iter()
        .any(|item| item.severity == Severity::Error)
    {
        return Err(ServiceError::InvalidDraft);
    }
    let (current, current_mode) = match fs::symlink_metadata(&document.target_path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            return Err(ServiceError::Symlink(document.target_path.clone()));
        }
        Ok(meta) => (
            Some(
                fs::read_to_string(&document.target_path)
                    .map_err(|error| io_error(&document.target_path, error))?,
            ),
            permission_mode(&meta),
        ),
        Err(error) if error.kind() == io::ErrorKind::NotFound => (None, None),
        Err(error) => return Err(io_error(&document.target_path, error)),
    };
    if current != document.expected_contents || current_mode != document.target_mode {
        return Err(ServiceError::ChangedOnDisk);
    }

    if document.scope == Scope::System && stage_system {
        let staged_path = stage_file(&document.unit_name, contents)?;
        let install_command = format!(
            "sudo install -D -m {:04o} -- {} {}",
            document.target_mode.unwrap_or(0o644) & 0o777,
            shell_quote(&staged_path.display().to_string()),
            shell_quote(&document.target_path.display().to_string())
        );
        return Ok(SaveOutcome {
            path: document.target_path.clone(),
            target_mode: Some(document.target_mode.unwrap_or(0o644) & 0o777),
            staged_path: Some(staged_path),
            install_command: Some(install_command),
            reload_command: "sudo systemctl daemon-reload".to_string(),
        });
    }

    if document.scope == Scope::System && !effective_uid_is_root() {
        return Err(ServiceError::SystemPermissionRequired);
    }
    atomic_write(
        &document.target_path,
        contents.as_bytes(),
        document.expected_contents.as_deref(),
        document.target_mode,
        make_backup,
    )?;
    Ok(SaveOutcome {
        path: document.target_path.clone(),
        target_mode: Some(document.target_mode.unwrap_or(0o644) & 0o777),
        staged_path: None,
        install_command: None,
        reload_command: match document.scope {
            Scope::User => "systemctl --user daemon-reload".to_string(),
            Scope::System => "systemctl daemon-reload".to_string(),
        },
    })
}

pub fn verify_service(path: &Path, scope: Scope) -> Result<VerifyOutcome, ServiceError> {
    if which_systemd_analyze().is_none() {
        return Ok(VerifyOutcome {
            available: false,
            exit_code: None,
            output: String::new(),
        });
    }
    let mut command = Command::new("systemd-analyze");
    command.arg("verify");
    command.arg(match scope {
        Scope::User => "--user",
        Scope::System => "--system",
    });
    command.arg(path);
    let output = command.output().map_err(|error| io_error(path, error))?;
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    Ok(VerifyOutcome {
        available: true,
        exit_code: output.status.code(),
        output: text,
    })
}

pub fn reload(scope: Scope) -> Result<String, ServiceError> {
    if scope == Scope::System && !effective_uid_is_root() {
        return Err(ServiceError::SystemPermissionRequired);
    }
    let output = Command::new("systemctl")
        .args(match scope {
            Scope::User => vec!["--user", "daemon-reload"],
            Scope::System => vec!["daemon-reload"],
        })
        .output()
        .map_err(|error| io_error("systemctl", error))?;
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    if !output.status.success() {
        return Err(ServiceError::Io(
            PathBuf::from("systemctl"),
            io::Error::other(text.trim().to_string()),
        ));
    }
    Ok(text)
}

pub fn diff(before: &str, after: &str) -> String {
    let old: Vec<_> = before.lines().collect();
    let new: Vec<_> = after.lines().collect();
    let mut result = String::new();
    result.push_str("--- saved\n+++ draft\n");
    let max = old.len().max(new.len());
    for index in 0..max {
        match (old.get(index), new.get(index)) {
            (Some(left), Some(right)) if left == right => {
                result.push_str("  ");
                result.push_str(left);
                result.push('\n');
            }
            (Some(left), Some(right)) => {
                result.push_str("- ");
                result.push_str(left);
                result.push('\n');
                result.push_str("+ ");
                result.push_str(right);
                result.push('\n');
            }
            (Some(left), None) => {
                result.push_str("- ");
                result.push_str(left);
                result.push('\n');
            }
            (None, Some(right)) => {
                result.push_str("+ ");
                result.push_str(right);
                result.push('\n');
            }
            (None, None) => {}
        }
    }
    result
}

fn config_dir(scope: Scope) -> Result<PathBuf, ServiceError> {
    match scope {
        Scope::User => config_home()
            .map(|home| home.join("systemd/user"))
            .ok_or(ServiceError::HomeNotFound),
        Scope::System => Ok(PathBuf::from("/etc/systemd/system")),
    }
}

fn stage_file(file_name: &str, contents: &str) -> Result<PathBuf, ServiceError> {
    let directory = state_home().join("linux-app-editor/staged");
    fs::create_dir_all(&directory).map_err(|error| io_error(&directory, error))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
            .map_err(|error| io_error(&directory, error))?;
    }
    let mut file = tempfile::Builder::new()
        .prefix("systemd-unit-")
        .suffix(&format!("-{file_name}"))
        .tempfile_in(&directory)
        .map_err(|error| io_error(&directory, error))?;
    file.write_all(contents.as_bytes())
        .and_then(|()| file.as_file().sync_all())
        .map_err(|error| io_error(&directory, error))?;
    file.keep()
        .map(|(_, path)| path)
        .map_err(|error| io_error(&directory, error.error))
}

fn atomic_write(
    path: &Path,
    contents: &[u8],
    expected: Option<&str>,
    expected_mode: Option<u32>,
    make_backup: bool,
) -> Result<(), ServiceError> {
    let parent = path
        .parent()
        .ok_or_else(|| io_error(path, io::Error::other("target has no parent directory")))?;
    fs::create_dir_all(parent).map_err(|error| io_error(parent, error))?;
    let existing = match fs::symlink_metadata(path) {
        Ok(meta) => Some(meta),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(io_error(path, error)),
    };
    if existing
        .as_ref()
        .is_some_and(|meta| meta.file_type().is_symlink())
    {
        return Err(ServiceError::Symlink(path.to_path_buf()));
    }
    if read_current(path)? != expected.map(str::to_string) {
        return Err(ServiceError::ChangedOnDisk);
    }
    if mode_at(path)? != expected_mode {
        return Err(ServiceError::ChangedOnDisk);
    }
    let mut file = tempfile::Builder::new()
        .prefix(".linux-app-editor-unit-")
        .tempfile_in(parent)
        .map_err(|error| io_error(parent, error))?;
    if let Some(meta) = existing.as_ref() {
        file.as_file()
            .set_permissions(meta.permissions())
            .map_err(|error| io_error(path, error))?;
    } else {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.as_file()
                .set_permissions(fs::Permissions::from_mode(0o644))
                .map_err(|error| io_error(path, error))?;
        }
    }
    file.write_all(contents)
        .and_then(|()| file.as_file().sync_all())
        .map_err(|error| io_error(path, error))?;
    if read_current(path)? != expected.map(str::to_string) || mode_at(path)? != expected_mode {
        return Err(ServiceError::ChangedOnDisk);
    }
    if make_backup && existing.is_some() {
        backup_file(path)?;
    }
    file.persist(path)
        .map_err(|error| io_error(path, error.error))?;
    if let Ok(directory) = fs::File::open(parent) {
        let _ = directory.sync_all();
    }
    Ok(())
}

fn read_current(path: &Path) -> Result<Option<String>, ServiceError> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => Err(ServiceError::Symlink(path.to_path_buf())),
        Ok(_) => fs::read_to_string(path)
            .map(Some)
            .map_err(|error| io_error(path, error)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io_error(path, error)),
    }
}

fn mode_at(path: &Path) -> Result<Option<u32>, ServiceError> {
    match fs::symlink_metadata(path) {
        Ok(meta) => Ok(permission_mode(&meta)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io_error(path, error)),
    }
}

fn permission_mode(metadata: &fs::Metadata) -> Option<u32> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        Some(metadata.permissions().mode() & 0o777)
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        None
    }
}

fn backup_file(path: &Path) -> Result<(), ServiceError> {
    let contents = read_current(path)?
        .ok_or_else(|| io_error(path, io::Error::other("backup source disappeared")))?;
    let mut backup = path.as_os_str().to_os_string();
    backup.push(".bak");
    let backup = PathBuf::from(backup);
    let expected = read_current(&backup)?;
    let expected_mode = mode_at(&backup)?;
    atomic_write(
        &backup,
        contents.as_bytes(),
        expected.as_deref(),
        expected_mode,
        false,
    )
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn effective_uid_is_root() -> bool {
    #[cfg(unix)]
    {
        unsafe { libc::geteuid() == 0 }
    }
    #[cfg(not(unix))]
    {
        false
    }
}

fn which_systemd_analyze() -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|directory| directory.join("systemd-analyze"))
            .find(|path| path.is_file())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_unknown_and_repeated_directives_when_opening() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.service");
        let original = "# keep me\n[Unit]\nDescription=first\nDescription=last\nX-Custom=yes\n\n[Service]\nExecStart=/usr/bin/true\n";
        fs::write(&path, original).unwrap();
        let document = open_service(&path, Scope::User).unwrap();
        assert_eq!(document.contents, original);
        assert!(
            document
                .contents
                .contains("Description=first\nDescription=last")
        );
        assert!(document.contents.contains("X-Custom=yes"));
    }

    #[cfg(unix)]
    #[test]
    fn opens_symlinked_vendor_directory_as_a_drop_in() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().unwrap();
        let vendor = directory.path().join("vendor");
        let alias = directory.path().join("alias");
        fs::create_dir(&vendor).unwrap();
        symlink(&vendor, &alias).unwrap();
        let unit_name = format!("editor-vendor-alias-{}.service", std::process::id());
        let source = vendor.join(&unit_name);
        fs::write(&source, "[Service]\nExecStart=/usr/bin/true\n").unwrap();

        let document = open_service_with_paths(
            &alias.join(&unit_name),
            Scope::System,
            std::slice::from_ref(&vendor),
        )
        .unwrap();

        assert!(document.is_drop_in);
        assert_eq!(document.source_path, alias.join(&unit_name));
        assert_eq!(
            document.target_path,
            Path::new("/etc/systemd/system")
                .join(format!("{unit_name}.d"))
                .join(EDITOR_DROP_IN)
        );
    }

    #[cfg(unix)]
    #[test]
    fn lists_services_non_recursively_with_first_path_precedence_and_masks() {
        use std::os::unix::fs::symlink;

        let primary = tempfile::tempdir().unwrap();
        let vendor = tempfile::tempdir().unwrap();
        fs::write(primary.path().join("worker.service"), "[Service]\n").unwrap();
        fs::write(vendor.path().join("worker.service"), "[Service]\n").unwrap();
        symlink("/dev/null", primary.path().join("masked.service")).unwrap();
        fs::create_dir(primary.path().join("nested")).unwrap();
        fs::write(primary.path().join("nested/hidden.service"), "[Service]\n").unwrap();

        let items = list_services_from_paths(
            Scope::User,
            &[primary.path().to_path_buf(), vendor.path().to_path_buf()],
        )
        .unwrap();
        assert_eq!(items.len(), 2);
        let worker = items
            .iter()
            .find(|item| item.unit_name == "worker.service")
            .unwrap();
        assert_eq!(worker.path, primary.path().join("worker.service"));
        let masked = items
            .iter()
            .find(|item| item.unit_name == "masked.service")
            .unwrap();
        assert!(masked.masked);
        assert!(!items.iter().any(|item| item.unit_name == "hidden.service"));
    }

    #[test]
    fn validates_unit_names() {
        assert!(validate_unit_name("worker@.service").is_ok());
        assert!(validate_unit_name("worker@north.service").is_ok());
        assert!(validate_unit_name(r"worker\x2dnorth.service").is_ok());
        assert!(validate_unit_name(r"worker\xZZ.service").is_err());
        assert!(validate_unit_name("../escape.service").is_err());
        assert!(validate_unit_name("worker.socket").is_err());
    }

    #[test]
    fn checks_structure_without_rewriting_raw_text() {
        let text = "[Service]\nExecStart=/usr/bin/true \\\n  --flag\n# comment\nCustom=kept\n";
        assert!(validate(text, false).is_empty());
        assert!(
            validate("[Unit]\nDescription=missing service\n", false)
                .iter()
                .any(|item| item.severity == Severity::Error)
        );
    }

    #[test]
    fn creates_scope_specific_templates() {
        let user = create_service(Scope::User, "user-app.service").unwrap();
        let system = create_service(Scope::System, "system-app.service").unwrap();
        assert!(user.contents.contains("WantedBy=default.target"));
        assert!(system.contents.contains("WantedBy=multi-user.target"));
        assert!(user.target_path.ends_with("systemd/user/user-app.service"));
        assert_eq!(
            system.target_path,
            Path::new("/etc/systemd/system/system-app.service")
        );
    }

    #[test]
    fn renders_reviewable_line_diff_and_quotes_paths() {
        let rendered = diff("old\n", "new\n");
        assert!(rendered.contains("- old"));
        assert!(rendered.contains("+ new"));
        assert_eq!(shell_quote("a'b"), "'a'\\''b'");
    }

    #[test]
    fn save_refuses_external_changes_and_keeps_backup() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("worker.service");
        let old = "[Service]\nExecStart=/usr/bin/true\n";
        let updated = "[Service]\nExecStart=/usr/bin/false\n";
        fs::write(&path, old).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        }
        let document = ServiceDocument {
            scope: Scope::User,
            unit_name: "worker.service".into(),
            source_path: path.clone(),
            target_path: path.clone(),
            target_mode: mode_at(&path).unwrap(),
            contents: old.into(),
            source_contents: old.into(),
            expected_contents: Some(old.into()),
            is_drop_in: false,
            is_new: false,
            diagnostics: Vec::new(),
        };
        let outcome = save_service(&document, updated, true, false).unwrap();
        assert_eq!(outcome.path, path);
        assert_eq!(fs::read_to_string(&path).unwrap(), updated);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o640
            );
        }
        assert_eq!(
            fs::read_to_string(path.with_extension("service.bak")).unwrap(),
            old
        );

        fs::write(&path, "changed outside\n").unwrap();
        assert!(matches!(
            save_service(
                &document,
                "[Service]\nExecStart=/usr/bin/replacement\n",
                false,
                false
            ),
            Err(ServiceError::ChangedOnDisk)
        ));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::write(&path, old).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
            assert!(matches!(
                save_service(&document, updated, false, false),
                Err(ServiceError::ChangedOnDisk)
            ));
        }
    }

    #[cfg(unix)]
    #[test]
    fn new_unit_permissions_are_reported_for_follow_up_saves() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("worker.service");
        let initial = "[Service]\nExecStart=/usr/bin/true\n";
        let updated = "[Service]\nExecStart=/usr/bin/false\n";
        let document = ServiceDocument {
            scope: Scope::User,
            unit_name: "worker.service".into(),
            source_path: path.clone(),
            target_path: path.clone(),
            target_mode: None,
            contents: initial.into(),
            source_contents: String::new(),
            expected_contents: None,
            is_drop_in: false,
            is_new: true,
            diagnostics: Vec::new(),
        };

        let created = save_service(&document, initial, false, false).unwrap();
        assert_eq!(created.target_mode, Some(0o644));
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o644
        );
        let reopened = ServiceDocument {
            contents: initial.into(),
            expected_contents: Some(initial.into()),
            target_mode: created.target_mode,
            is_new: false,
            ..document
        };
        save_service(&reopened, updated, false, false).unwrap();
        assert_eq!(fs::read_to_string(path).unwrap(), updated);
    }

    #[cfg(unix)]
    #[test]
    fn system_save_stages_private_file_and_exact_install_command() {
        use std::os::unix::fs::PermissionsExt;

        let state_home = tempfile::tempdir().unwrap();
        let original_state_home = std::env::var_os("XDG_STATE_HOME");
        unsafe { std::env::set_var("XDG_STATE_HOME", state_home.path()) };
        let system_dir = tempfile::tempdir().unwrap();
        let target = system_dir.path().join("worker.service");
        let contents = "[Service]\nExecStart=/usr/bin/true\n";
        let document = ServiceDocument {
            scope: Scope::System,
            unit_name: "worker.service".into(),
            source_path: target.clone(),
            target_path: target.clone(),
            target_mode: None,
            contents: String::new(),
            source_contents: String::new(),
            expected_contents: None,
            is_drop_in: false,
            is_new: true,
            diagnostics: Vec::new(),
        };
        let outcome = save_service(&document, contents, false, true).unwrap();
        let staged = outcome.staged_path.unwrap();
        assert_eq!(fs::read_to_string(&staged).unwrap(), contents);
        assert_eq!(
            fs::metadata(&staged).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(
            outcome
                .install_command
                .unwrap()
                .contains("sudo install -D -m 0644")
        );
        assert_eq!(outcome.reload_command, "sudo systemctl daemon-reload");
        let _ = fs::remove_file(staged);

        let existing = system_dir.path().join("existing.service");
        let existing_contents = "[Service]\nExecStart=/usr/bin/true\n";
        fs::write(&existing, existing_contents).unwrap();
        fs::set_permissions(&existing, fs::Permissions::from_mode(0o640)).unwrap();
        let existing_document = ServiceDocument {
            scope: Scope::System,
            unit_name: "existing.service".into(),
            source_path: existing.clone(),
            target_path: existing.clone(),
            target_mode: Some(0o640),
            contents: existing_contents.into(),
            source_contents: existing_contents.into(),
            expected_contents: Some(existing_contents.into()),
            is_drop_in: false,
            is_new: false,
            diagnostics: Vec::new(),
        };
        let existing_outcome = save_service(&existing_document, contents, false, true).unwrap();
        assert!(
            existing_outcome
                .install_command
                .unwrap()
                .contains("-m 0640")
        );
        let _ = fs::remove_file(existing_outcome.staged_path.unwrap());
        if let Some(value) = original_state_home {
            unsafe { std::env::set_var("XDG_STATE_HOME", value) };
        } else {
            unsafe { std::env::remove_var("XDG_STATE_HOME") };
        }
    }

    #[cfg(unix)]
    #[test]
    fn save_refuses_symlink_targets() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().unwrap();
        let real = directory.path().join("real.service");
        let link = directory.path().join("link.service");
        fs::write(&real, "[Service]\nExecStart=/usr/bin/true\n").unwrap();
        symlink(&real, &link).unwrap();
        let document = ServiceDocument {
            scope: Scope::User,
            unit_name: "link.service".into(),
            source_path: link.clone(),
            target_path: link,
            target_mode: None,
            contents: "[Service]\nExecStart=/usr/bin/true\n".into(),
            source_contents: "[Service]\nExecStart=/usr/bin/true\n".into(),
            expected_contents: Some("[Service]\nExecStart=/usr/bin/true\n".into()),
            is_drop_in: false,
            is_new: false,
            diagnostics: Vec::new(),
        };
        assert!(matches!(
            save_service(
                &document,
                "[Service]\nExecStart=/usr/bin/false\n",
                false,
                false
            ),
            Err(ServiceError::Symlink(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn save_refuses_symlink_backup_targets_without_changing_the_original() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("worker.service");
        let protected = directory.path().join("protected");
        let backup = directory.path().join("worker.service.bak");
        let original = "[Service]\nExecStart=/usr/bin/true\n";
        fs::write(&path, original).unwrap();
        fs::write(&protected, "preserve this file").unwrap();
        symlink(&protected, &backup).unwrap();
        let document = ServiceDocument {
            scope: Scope::User,
            unit_name: "worker.service".into(),
            source_path: path.clone(),
            target_path: path.clone(),
            target_mode: mode_at(&path).unwrap(),
            contents: original.into(),
            source_contents: original.into(),
            expected_contents: Some(original.into()),
            is_drop_in: false,
            is_new: false,
            diagnostics: Vec::new(),
        };

        assert!(matches!(
            save_service(
                &document,
                "[Service]\nExecStart=/usr/bin/false\n",
                true,
                false
            ),
            Err(ServiceError::Symlink(_))
        ));
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        assert_eq!(
            fs::read_to_string(&protected).unwrap(),
            "preserve this file"
        );
    }
}
