use serde::{Deserialize, Serialize};

use crate::ServiceError;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServiceForm {
    pub description: String,
    pub service_type: String,
    pub exec_start: String,
    pub working_directory: String,
    pub user: String,
    pub group: String,
    pub restart: String,
    pub restart_sec: String,
    pub wanted_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormProjection {
    pub form: ServiceForm,
    pub locked_fields: Vec<String>,
}

const FIELDS: [(&str, &str, &str); 9] = [
    ("description", "Unit", "Description"),
    ("service_type", "Service", "Type"),
    ("exec_start", "Service", "ExecStart"),
    ("working_directory", "Service", "WorkingDirectory"),
    ("user", "Service", "User"),
    ("group", "Service", "Group"),
    ("restart", "Service", "Restart"),
    ("restart_sec", "Service", "RestartSec"),
    ("wanted_by", "Install", "WantedBy"),
];

pub fn project_form(contents: &str, is_drop_in: bool) -> FormProjection {
    let mut form = ServiceForm::default();
    let mut locked_fields = Vec::new();
    let lines: Vec<&str> = contents.lines().collect();
    for (field, section, key) in FIELDS {
        let matching: Vec<(usize, &str)> = directive_lines(&lines, section, key);
        let effective = if field == "exec_start" && is_drop_in {
            let values: Vec<&str> = matching
                .iter()
                .map(|(_, line)| value_part(line))
                .filter(|value| !value.trim().is_empty())
                .collect();
            if values.len() == 1 && matching.len() <= 2 {
                values.first().copied()
            } else {
                None
            }
        } else if matching.len() == 1 {
            Some(value_part(matching[0].1))
        } else {
            None
        };
        let continued = matching.iter().any(|(index, line)| {
            line.trim_end().ends_with('\\')
                || (*index > 0 && lines[*index - 1].trim_end().ends_with('\\'))
        });
        if let Some(value) = effective.filter(|_| !continued) {
            let value = value.trim().to_string();
            set_form_value(&mut form, field, value);
        } else if !matching.is_empty() {
            locked_fields.push(field.to_string());
        }
    }
    FormProjection {
        form,
        locked_fields,
    }
}

pub fn apply_form(
    original: &str,
    form: &ServiceForm,
    dirty_fields: &[String],
    is_drop_in: bool,
) -> Result<String, ServiceError> {
    let projection = project_form(original, is_drop_in);
    let mut text = original.to_string();
    for (field, section, key) in FIELDS {
        if !dirty_fields.iter().any(|dirty| dirty == field) {
            continue;
        }
        if projection
            .locked_fields
            .iter()
            .any(|locked| locked == field)
        {
            return Err(ServiceError::AmbiguousFormField(field.to_string()));
        }
        let value = form_value(form, field);
        if field == "exec_start" && is_drop_in {
            text = set_drop_in_exec_start(&text, value)?;
        } else {
            text = set_directive(&text, section, key, value)?;
        }
    }
    Ok(text)
}

fn set_form_value(form: &mut ServiceForm, key: &str, value: String) {
    match key {
        "description" => form.description = value,
        "service_type" => form.service_type = value,
        "exec_start" => form.exec_start = value,
        "working_directory" => form.working_directory = value,
        "user" => form.user = value,
        "group" => form.group = value,
        "restart" => form.restart = value,
        "restart_sec" => form.restart_sec = value,
        "wanted_by" => form.wanted_by = value,
        _ => unreachable!(),
    }
}

fn form_value<'a>(form: &'a ServiceForm, key: &str) -> &'a str {
    match key {
        "description" => &form.description,
        "service_type" => &form.service_type,
        "exec_start" => &form.exec_start,
        "working_directory" => &form.working_directory,
        "user" => &form.user,
        "group" => &form.group,
        "restart" => &form.restart,
        "restart_sec" => &form.restart_sec,
        "wanted_by" => &form.wanted_by,
        _ => unreachable!(),
    }
}

fn directive_lines<'a>(
    lines: &'a [&'a str],
    target_section: &str,
    target_key: &str,
) -> Vec<(usize, &'a str)> {
    let mut section = "";
    lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| {
            let trimmed = line.trim();
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                section = &trimmed[1..trimmed.len() - 1];
                return None;
            }
            if section != target_section {
                return None;
            }
            let (key, _) = trimmed.split_once('=')?;
            (key.trim() == target_key).then_some((index, *line))
        })
        .collect()
}

fn value_part(line: &str) -> &str {
    line.split_once('=').map_or("", |(_, value)| value)
}

fn set_directive(
    text: &str,
    section: &str,
    key: &str,
    value: &str,
) -> Result<String, ServiceError> {
    let mut lines = split_lines(text);
    let line_ending = line_ending(text);
    let trailing_newline = has_trailing_newline(text);
    let borrowed: Vec<&str> = lines.iter().map(String::as_str).collect();
    let matches: Vec<(usize, String)> = directive_lines(&borrowed, section, key)
        .into_iter()
        .map(|(index, line)| (index, line.to_string()))
        .collect();
    if matches.len() > 1 {
        return Err(ServiceError::AmbiguousFormField(key.to_string()));
    }
    if let Some((index, line)) = matches.first() {
        if value.is_empty() {
            lines.remove(*index);
        } else {
            let (left, _) = line.split_once('=').unwrap_or((line.as_str(), ""));
            lines[*index] = format!("{left}={value}");
        }
    } else if !value.is_empty() {
        insert_directive(&mut lines, section, key, value);
    }
    Ok(join_lines(lines, trailing_newline, line_ending))
}

fn set_drop_in_exec_start(text: &str, value: &str) -> Result<String, ServiceError> {
    let mut lines = split_lines(text);
    let line_ending = line_ending(text);
    let trailing_newline = has_trailing_newline(text);
    let borrowed: Vec<&str> = lines.iter().map(String::as_str).collect();
    let indices: Vec<usize> = directive_lines(&borrowed, "Service", "ExecStart")
        .into_iter()
        .map(|(index, _)| index)
        .collect();
    for index in indices.into_iter().rev() {
        lines.remove(index);
    }
    insert_directive(&mut lines, "Service", "ExecStart", "");
    if !value.is_empty() {
        insert_directive(&mut lines, "Service", "ExecStart", value);
    }
    Ok(join_lines(lines, trailing_newline, line_ending))
}

fn insert_directive(lines: &mut Vec<String>, section: &str, key: &str, value: &str) {
    let header = format!("[{section}]");
    let start = lines.iter().position(|line| line.trim() == header);
    if let Some(start) = start {
        let end = lines[start + 1..]
            .iter()
            .position(|line| line.trim().starts_with('[') && line.trim().ends_with(']'))
            .map(|offset| start + 1 + offset)
            .unwrap_or(lines.len());
        lines.insert(end, format!("{key}={value}"));
    } else {
        if lines.last().is_some_and(|line| !line.is_empty()) {
            lines.push(String::new());
        }
        lines.push(header);
        lines.push(format!("{key}={value}"));
    }
}

fn split_lines(text: &str) -> Vec<String> {
    text.split_terminator('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line).to_string())
        .collect()
}

fn line_ending(text: &str) -> &'static str {
    if text.contains("\r\n") { "\r\n" } else { "\n" }
}

fn has_trailing_newline(text: &str) -> bool {
    text.ends_with('\n')
}

fn join_lines(lines: Vec<String>, trailing_newline: bool, newline: &str) -> String {
    let mut text = lines.join(newline);
    if trailing_newline && !text.is_empty() {
        text.push_str(newline);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_only_dirty_form_fields_and_keeps_raw_lines() {
        let original = "# keep\n[Unit]\nDescription=Old\nX-Unknown=yes\n\n[Service]\nType=simple\nExecStart=/usr/bin/old\nExecStartPre=/usr/bin/check\n";
        let mut form = project_form(original, false).form;
        form.description = "New".into();
        let updated = apply_form(original, &form, &["description".into()], false).unwrap();
        assert!(updated.contains("Description=New"));
        assert!(updated.contains("X-Unknown=yes"));
        assert!(updated.contains("ExecStartPre=/usr/bin/check"));
        assert!(!updated.contains("Description=Old"));
    }

    #[test]
    fn repeated_fields_are_locked_for_form_edits() {
        let original = "[Service]\nExecStart=/usr/bin/a\nExecStart=/usr/bin/b\n";
        let projection = project_form(original, false);
        assert!(projection.locked_fields.contains(&"exec_start".to_string()));
        assert!(
            apply_form(
                original,
                &ServiceForm::default(),
                &["exec_start".into()],
                false
            )
            .is_err()
        );
    }

    #[test]
    fn drop_in_exec_start_resets_base_commands_before_adding_override() {
        let mut form = project_form("[Service]\n", true).form;
        form.exec_start = "/usr/bin/new --flag".into();
        let updated = apply_form("[Service]\n", &form, &["exec_start".into()], true).unwrap();
        assert!(updated.contains("ExecStart=\nExecStart=/usr/bin/new --flag"));
    }

    #[test]
    fn preserves_crlf_when_patching_a_field() {
        let original = "[Unit]\r\nDescription=Before\r\n[Service]\r\nExecStart=/usr/bin/true\r\n";
        let mut form = project_form(original, false).form;
        form.description = "After".into();
        let updated = apply_form(original, &form, &["description".into()], false).unwrap();
        assert!(updated.contains("Description=After\r\n"));
        assert!(!updated.contains("\n") || updated.contains("\r\n"));
    }
}
