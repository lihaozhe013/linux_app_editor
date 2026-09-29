//! Non-blocking validation. Only a missing required key (Name, Exec) or an
//! invalid filename is an error; everything else is a warning the user may
//! ignore (paths may legitimately not exist yet).

use serde::Serialize;
use std::path::Path;

use super::exec::ExecSpec;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidationItem {
    pub severity: Severity,
    pub code: String,
    pub message: String,
}

fn error(code: &str, message: impl Into<String>) -> ValidationItem {
    ValidationItem {
        severity: Severity::Error,
        code: code.to_string(),
        message: message.into(),
    }
}

fn warning(code: &str, message: impl Into<String>) -> ValidationItem {
    ValidationItem {
        severity: Severity::Warning,
        code: code.to_string(),
        message: message.into(),
    }
}

pub fn validate_filename_stem(stem: &str) -> Result<(), String> {
    if stem.is_empty() {
        Err("filename must not be empty".to_string())
    } else if stem.starts_with('.') {
        Err("filename must not start with '.'".to_string())
    } else if !stem
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        Err("filename may only contain letters, digits, '-', '_' and '.'".to_string())
    } else {
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
pub fn validate_entry(
    name: Option<&str>,
    exec: Option<&ExecSpec>,
    icon: Option<&str>,
    work_dir: Option<&str>,
    type_: Option<&str>,
) -> Vec<ValidationItem> {
    let mut items = Vec::new();

    match name {
        None => items.push(error("name-missing", "Name is required by the spec")),
        Some(n) if n.trim().is_empty() => items.push(error("name-empty", "Name must not be empty")),
        Some(_) => {}
    }

    match exec {
        None => items.push(error("exec-missing", "Exec is required by the spec")),
        Some(spec) if spec.executable.trim().is_empty() => {
            items.push(error("exec-empty", "Executable must not be empty"))
        }
        Some(spec) => items.extend(check_executable(&spec.executable)),
    }

    if let Some(icon) =
        icon.filter(|i| !i.is_empty() && i.starts_with('/') && !Path::new(i).exists())
    {
        items.push(warning(
            "icon-missing",
            format!("Icon path does not exist: {icon}"),
        ));
    }

    if let Some(dir) = work_dir.filter(|d| !d.is_empty()) {
        let p = Path::new(dir);
        if !p.is_dir() {
            items.push(warning(
                "workdir-missing",
                format!("Working directory does not exist: {dir}"),
            ));
        }
    }

    if let Some(t) = type_.filter(|t| !t.is_empty() && *t != "Application") {
        items.push(warning(
            "type-not-application",
            format!("Type is \"{t}\", this tool mainly targets Application entries"),
        ));
    }

    items
}

fn check_executable(executable: &str) -> Vec<ValidationItem> {
    if executable.contains('/') {
        let path = Path::new(executable);
        if !path.exists() {
            return vec![warning(
                "executable-missing",
                format!("Executable does not currently exist: {executable}"),
            )];
        }
        let mut items = Vec::new();
        if path.is_dir() {
            items.push(warning(
                "executable-is-directory",
                format!("Executable path is a directory: {executable}"),
            ));
            return items;
        }
        #[cfg(unix)]
        if !is_executable_file(path) {
            items.push(warning(
                "executable-not-executable",
                format!("Executable bit is not set: {executable}"),
            ));
        }
        return items;
    }

    match which::which(executable) {
        Ok(_) => Vec::new(),
        Err(_) => vec![warning(
            "executable-not-in-path",
            format!("Executable not found in $PATH: {executable}"),
        )],
    }
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exec(exe: &str) -> ExecSpec {
        ExecSpec {
            executable: exe.to_string(),
            arguments: vec![],
        }
    }

    #[test]
    fn filename_rules() {
        assert!(validate_filename_stem("my-app").is_ok());
        assert!(validate_filename_stem("org.example.Foo_2").is_ok());
        assert!(validate_filename_stem("").is_err());
        assert!(validate_filename_stem(".hidden").is_err());
        assert!(validate_filename_stem("a/b").is_err());
        assert!(validate_filename_stem("a b").is_err());
    }

    #[test]
    fn missing_name_and_exec_are_errors() {
        let items = validate_entry(None, None, None, None, None);
        assert!(
            items
                .iter()
                .any(|i| i.code == "name-missing" && i.severity == Severity::Error)
        );
        assert!(
            items
                .iter()
                .any(|i| i.code == "exec-missing" && i.severity == Severity::Error)
        );
    }

    #[test]
    fn nonexistent_executable_is_only_a_warning() {
        let e = exec("/definitely/not/here/foo");
        let items = validate_entry(Some("Foo"), Some(&e), None, None, None);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].severity, Severity::Warning);
        assert_eq!(items[0].code, "executable-missing");
    }

    #[test]
    fn path_binary_in_path_is_clean() {
        let e = exec("sh");
        let items = validate_entry(Some("Foo"), Some(&e), None, None, None);
        assert!(items.is_empty());
    }

    #[test]
    fn type_link_is_warning() {
        let e = exec("sh");
        let items = validate_entry(Some("Foo"), Some(&e), None, None, Some("Link"));
        assert!(
            items
                .iter()
                .any(|i| i.code == "type-not-application" && i.severity == Severity::Warning)
        );
    }
}
