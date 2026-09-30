use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Scope, ServiceError};

pub const USER_BASIC_RESTART: &str = "user-basic-restart";
pub const SYSTEM_BASIC_RESTART: &str = "system-basic-restart";

/// A unit template offered by the create flow. The scope belongs to the
/// template because the install directory and the `[Install]` target differ
/// between user and machine-wide services.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceTemplate {
    pub id: String,
    pub label: String,
    pub description: String,
    pub scope: Scope,
    pub install_target: String,
}

pub fn templates() -> Vec<ServiceTemplate> {
    vec![
        ServiceTemplate {
            id: USER_BASIC_RESTART.to_string(),
            label: "Autostart program (user)".to_string(),
            description: "Start one program at login for the current user and restart it when it exits."
                .to_string(),
            scope: Scope::User,
            install_target: "default.target".to_string(),
        },
        ServiceTemplate {
            id: SYSTEM_BASIC_RESTART.to_string(),
            label: "Autostart program (system, sudo)".to_string(),
            description: "Start one program at boot for every user and restart it when it exits. Saving stages the file for a sudo install."
                .to_string(),
            scope: Scope::System,
            install_target: "multi-user.target".to_string(),
        },
    ]
}

pub fn find(id: &str) -> Option<ServiceTemplate> {
    templates().into_iter().find(|item| item.id == id)
}

pub fn id_for_scope(scope: Scope) -> &'static str {
    match scope {
        Scope::User => USER_BASIC_RESTART,
        Scope::System => SYSTEM_BASIC_RESTART,
    }
}

/// Derive a unit name from an executable path: the file name without extension,
/// reduced to the character set systemd accepts for unit names.
pub fn suggested_unit_name(exec_path: &str) -> String {
    let file_name = Path::new(exec_path.trim())
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let stem = match file_name.rsplit_once('.') {
        Some((head, tail)) if !tail.is_empty() => head,
        _ => file_name,
    };
    let mut cleaned = String::new();
    for character in stem.chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '_' | '@' | '-') {
            cleaned.push(character);
        } else if !cleaned.ends_with('-') {
            cleaned.push('-');
        }
    }
    // A trailing `@` would turn the suggestion into an instance unit, which
    // needs a template name to be meaningful.
    let cleaned = cleaned.trim_matches(['-', '@']).to_string();
    let base = if cleaned.is_empty() {
        "app".to_string()
    } else {
        cleaned
    };
    format!("{base}.service")
}

/// Resolve and sanity-check the program a template will run. A unit pointing at
/// a missing or non-executable file is rejected by systemd at start time, so the
/// template flow refuses to build one.
pub fn resolve_exec_path(exec_path: &str) -> Result<PathBuf, ServiceError> {
    let trimmed = exec_path.trim();
    if trimmed.is_empty() {
        return Err(ServiceError::EmptyExecPath);
    }
    let path = PathBuf::from(trimmed);
    if !path.is_absolute() {
        return Err(ServiceError::RelativeExecPath(trimmed.to_string()));
    }
    let metadata = std::fs::metadata(&path).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => ServiceError::ExecNotFound(path.clone()),
        _ => ServiceError::Io(path.clone(), error),
    })?;
    if !metadata.is_file() {
        return Err(ServiceError::ExecNotAFile(path));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err(ServiceError::ExecNotExecutable(path));
        }
    }
    Ok(path)
}

/// Render the ExecStart value. `%` starts a systemd specifier and whitespace
/// splits the command line, so both need escaping before the raw path is
/// written into the unit.
fn exec_start_value(path: &Path) -> String {
    let escaped = path
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('%', "%%");
    if escaped.contains(char::is_whitespace) {
        format!("\"{escaped}\"")
    } else {
        escaped
    }
}

fn description_from_unit(unit_name: &str) -> String {
    let stem = unit_name.strip_suffix(".service").unwrap_or(unit_name);
    let words: Vec<String> = stem
        .split(['-', '_', '.', '@'])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut characters = word.chars();
            match characters.next() {
                Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
                None => String::new(),
            }
        })
        .collect();
    if words.is_empty() {
        "Managed program".to_string()
    } else {
        format!("{} (managed by linux-app-editor)", words.join(" "))
    }
}

pub fn render(template_id: &str, unit_name: &str, exec_path: &str) -> Result<String, ServiceError> {
    let template =
        find(template_id).ok_or_else(|| ServiceError::UnknownTemplate(template_id.to_string()))?;
    let path = resolve_exec_path(exec_path)?;
    let user_hint = match template.scope {
        Scope::User => {
            "# Uncomment to run the program as another account or from a specific directory.\n\
             # User=%U\n\
             # WorkingDirectory=/path/to/working/dir\n"
        }
        Scope::System => {
            "# The service runs as root unless User= and Group= are set below.\n\
             # User=nobody\n\
             # Group=nogroup\n\
             # WorkingDirectory=/path/to/working/dir\n"
        }
    };
    Ok(format!(
        "[Unit]\n\
         Description={description}\n\
         # Restart limits keep a crashing program from looping forever.\n\
         StartLimitIntervalSec=60\n\
         StartLimitBurst=5\n\
         \n\
         [Service]\n\
         Type=simple\n\
         ExecStart={exec_start}\n\
         Restart=always\n\
         RestartSec=5s\n\
         {user_hint}\
         \n\
         [Install]\n\
         WantedBy={install_target}\n",
        description = description_from_unit(unit_name),
        exec_start = exec_start_value(&path),
        user_hint = user_hint,
        install_target = template.install_target,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate;

    #[test]
    fn suggests_a_unit_name_from_the_program_path() {
        assert_eq!(
            suggested_unit_name("/opt/my apps/foo bar"),
            "foo-bar.service"
        );
        assert_eq!(suggested_unit_name("/usr/bin/worker@.sh"), "worker.service");
        assert_eq!(suggested_unit_name("relative/thing"), "thing.service");
        assert_eq!(suggested_unit_name("/"), "app.service");
    }

    #[cfg(unix)]
    fn executable_program(directory: &Path, name: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = directory.join(name);
        std::fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[test]
    fn renders_user_and_system_bodies() {
        let directory = tempfile::tempdir().unwrap();
        let program = executable_program(directory.path(), "myapp");
        let path = program.to_str().unwrap();

        let user = render(USER_BASIC_RESTART, "myapp.service", path).unwrap();
        assert!(user.contains("WantedBy=default.target"));
        assert!(user.contains("Restart=always"));
        assert!(user.contains("RestartSec=5s"));
        assert!(user.contains(&format!("ExecStart={}", program.display())));
        assert!(user.contains("Description=Myapp (managed by linux-app-editor)"));

        let system = render(SYSTEM_BASIC_RESTART, "myapp.service", path).unwrap();
        assert!(system.contains("WantedBy=multi-user.target"));
        assert!(system.contains("runs as root"));
        assert!(validate(&system, false).is_empty());
    }

    #[test]
    fn quotes_paths_with_whitespace_and_escapes_specifiers() {
        let directory = tempfile::tempdir().unwrap();
        let program = executable_program(directory.path(), "my app");
        let rendered =
            render(USER_BASIC_RESTART, "app.service", program.to_str().unwrap()).unwrap();
        assert!(rendered.contains(&format!("ExecStart=\"{}\"", program.display())));
        assert_eq!(
            exec_start_value(Path::new("/opt/100%discount/tool")),
            "/opt/100%%discount/tool"
        );
    }

    #[test]
    fn rejects_unusable_exec_paths_and_unknown_templates() {
        assert!(matches!(
            resolve_exec_path("  "),
            Err(ServiceError::EmptyExecPath)
        ));
        assert!(matches!(
            resolve_exec_path("bin/tool"),
            Err(ServiceError::RelativeExecPath(_))
        ));
        assert!(matches!(
            resolve_exec_path("/definitely/missing/tool"),
            Err(ServiceError::ExecNotFound(_))
        ));
        assert!(matches!(
            resolve_exec_path("/tmp"),
            Err(ServiceError::ExecNotAFile(_))
        ));
        assert!(matches!(
            render("no-such-template", "app.service", "/bin/sh"),
            Err(ServiceError::UnknownTemplate(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_a_non_executable_program() {
        let directory = tempfile::tempdir().unwrap();
        let data = directory.path().join("data.txt");
        std::fs::write(&data, "not a program").unwrap();
        assert!(matches!(
            resolve_exec_path(data.to_str().unwrap()),
            Err(ServiceError::ExecNotExecutable(_))
        ));
    }

    #[test]
    fn every_scope_maps_to_exactly_one_template() {
        assert_eq!(templates().len(), 2);
        for scope in [Scope::User, Scope::System] {
            let template = find(id_for_scope(scope)).unwrap();
            assert_eq!(template.scope, scope);
        }
    }
}
