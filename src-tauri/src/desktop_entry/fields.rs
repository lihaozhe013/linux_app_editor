//! Typed access to the well-known keys of the `Desktop Entry` group.
//!
//! Values are stored raw in the document model; this layer decodes them on
//! read and encodes them on write. Unknown keys, `X-*` keys, locale keys and
//! other groups are never touched here.

use serde::{Deserialize, Serialize};

use super::exec::{ExecSpec, parse_exec};
use super::model::DesktopFile;

pub const MANAGED_KEY: &str = "X-LauncherEditor-Managed";
pub const MANAGED_VERSION_KEY: &str = "X-LauncherEditor-Version";
pub const MANAGED_VERSION_VALUE: &str = "1";
pub const SPEC_VERSION_VALUE: &str = "1.5";

/// Keys recognized by the spec (plus this tool's own keys) for the
/// "unknown keys preserved" report.
pub const KNOWN_KEYS: &[&str] = &[
    "Version",
    "Type",
    "Name",
    "GenericName",
    "Comment",
    "Icon",
    "Exec",
    "TryExec",
    "Path",
    "Terminal",
    "Categories",
    "Keywords",
    "StartupNotify",
    "StartupWMClass",
    "URL",
    "MimeType",
    "NoDisplay",
    "Hidden",
    "OnlyShowIn",
    "NotShowIn",
    "Actions",
    "DBusActivatable",
    "Implements",
    MANAGED_KEY,
    MANAGED_VERSION_KEY,
];

#[derive(Debug, Clone, Serialize)]
pub struct KnownFields {
    pub version: Option<String>,
    pub type_: Option<String>,
    pub name: Option<String>,
    pub exec: Option<ExecSpec>,
    pub exec_parse_error: bool,
    pub icon: Option<String>,
    pub path: Option<String>,
    pub terminal: Option<bool>,
    pub comment: Option<String>,
    pub categories: Option<Vec<String>>,
    pub keywords: Option<Vec<String>>,
    pub startup_notify: Option<bool>,
    pub startup_wm_class: Option<String>,
    pub mime_types: Option<Vec<String>>,
    pub no_display: Option<bool>,
    pub hidden: Option<bool>,
    pub only_show_in: Option<Vec<String>>,
    pub not_show_in: Option<Vec<String>>,
    pub actions: Option<Vec<String>>,
    pub managed: bool,
    pub managed_version: Option<String>,
}

fn unescape_string(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    let mut chars = v.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('s') => out.push(' '),
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn escape_string(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    for c in v.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            _ => out.push(c),
        }
    }
    out
}

/// Icon values must escape spaces as `\s` per the spec.
fn escape_icon(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    for c in escape_string(v).chars() {
        if c == ' ' {
            out.push_str("\\s");
        } else {
            out.push(c);
        }
    }
    out
}

fn parse_bool(v: &str) -> Option<bool> {
    match v {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn bool_value(v: bool) -> String {
    if v { "true".into() } else { "false".into() }
}

/// Spec: list values are semicolon-separated with a trailing semicolon.
fn join_list(items: &[String]) -> String {
    let mut out = String::new();
    for item in items {
        out.push_str(item);
        out.push(';');
    }
    out
}

fn split_list(v: &str) -> Vec<String> {
    v.split(';')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

pub fn read_fields(file: &DesktopFile) -> KnownFields {
    let string = |key: &str| file.get_raw(key).map(unescape_string);
    let boolean = |key: &str| file.get_raw(key).and_then(parse_bool);
    let list = |key: &str| file.get_raw(key).map(split_list);

    let (exec, exec_parse_error) = match file.get_raw("Exec") {
        None => (None, false),
        Some(raw) => match parse_exec(raw) {
            Ok(spec) => (Some(spec), false),
            // Keep the raw text visible and editable instead of failing to
            // open the file; validation tells the user it is malformed.
            Err(_) => (
                Some(ExecSpec {
                    executable: raw.to_string(),
                    arguments: Vec::new(),
                }),
                true,
            ),
        },
    };

    KnownFields {
        version: file.get_raw("Version").map(str::to_string),
        type_: file.get_raw("Type").map(str::to_string),
        name: string("Name"),
        exec,
        exec_parse_error,
        icon: string("Icon"),
        path: string("Path"),
        terminal: boolean("Terminal"),
        comment: string("Comment"),
        categories: list("Categories"),
        keywords: list("Keywords"),
        startup_notify: boolean("StartupNotify"),
        startup_wm_class: string("StartupWMClass"),
        mime_types: list("MimeType"),
        no_display: boolean("NoDisplay"),
        hidden: boolean("Hidden"),
        only_show_in: list("OnlyShowIn"),
        not_show_in: list("NotShowIn"),
        actions: list("Actions"),
        managed: boolean(MANAGED_KEY).unwrap_or(false),
        managed_version: file.get_raw(MANAGED_VERSION_KEY).map(str::to_string),
    }
}

/// Patch semantics per field: `None` = leave untouched, `Some(None)` =
/// remove the key, `Some(Some(v))` = set the value. JSON representation:
/// absent / `null` / value.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct FieldPatch {
    pub name: Option<Option<String>>,
    pub exec: Option<Option<ExecSpec>>,
    pub icon: Option<Option<String>>,
    pub path: Option<Option<String>>,
    pub terminal: Option<Option<bool>>,
    pub comment: Option<Option<String>>,
    pub categories: Option<Option<Vec<String>>>,
    pub keywords: Option<Option<Vec<String>>>,
    pub startup_notify: Option<Option<bool>>,
    pub startup_wm_class: Option<Option<String>>,
    pub mime_types: Option<Option<Vec<String>>>,
    pub no_display: Option<Option<bool>>,
    pub hidden: Option<Option<bool>>,
    pub only_show_in: Option<Option<Vec<String>>>,
    pub not_show_in: Option<Option<Vec<String>>>,
    pub actions: Option<Option<Vec<String>>>,
}

pub fn apply_patch(file: &mut DesktopFile, patch: &FieldPatch) {
    let str_patch = |file: &mut DesktopFile,
                     key: &str,
                     p: &Option<Option<String>>,
                     escape: fn(&str) -> String| {
        match p {
            None => {}
            Some(None) => {
                file.remove_raw(key);
            }
            Some(Some(v)) => file.set_raw(key, &escape(v)),
        }
    };

    str_patch(file, "Name", &patch.name, escape_string);
    match &patch.exec {
        None => {}
        Some(None) => {
            file.remove_raw("Exec");
        }
        // Exec has its own quoting rules (see exec.rs); it must not go
        // through the string-key escape functions.
        Some(Some(spec)) => file.set_raw("Exec", &super::exec::build_exec(spec)),
    }
    str_patch(file, "Icon", &patch.icon, escape_icon);
    str_patch(file, "Path", &patch.path, escape_string);
    str_patch(file, "Comment", &patch.comment, escape_string);
    str_patch(
        file,
        "StartupWMClass",
        &patch.startup_wm_class,
        escape_string,
    );

    let bool_patch = |file: &mut DesktopFile, key: &str, p: &Option<Option<bool>>| match p {
        None => {}
        Some(None) => {
            file.remove_raw(key);
        }
        Some(Some(v)) => file.set_raw(key, &bool_value(*v)),
    };
    bool_patch(file, "Terminal", &patch.terminal);
    bool_patch(file, "StartupNotify", &patch.startup_notify);
    bool_patch(file, "NoDisplay", &patch.no_display);
    bool_patch(file, "Hidden", &patch.hidden);

    let list_patch = |file: &mut DesktopFile, key: &str, p: &Option<Option<Vec<String>>>| match p {
        None => {}
        Some(None) => {
            file.remove_raw(key);
        }
        Some(Some(items)) => file.set_raw(key, &join_list(items)),
    };
    list_patch(file, "Categories", &patch.categories);
    list_patch(file, "Keywords", &patch.keywords);
    list_patch(file, "MimeType", &patch.mime_types);
    list_patch(file, "OnlyShowIn", &patch.only_show_in);
    list_patch(file, "NotShowIn", &patch.not_show_in);
    list_patch(file, "Actions", &patch.actions);
}

/// Marker keys for launchers created by this tool. Editing an existing file
/// never adds or removes them; the file itself is the source of truth.
pub fn apply_managed_markers(file: &mut DesktopFile) {
    file.set_raw(MANAGED_KEY, "true");
    file.set_raw(MANAGED_VERSION_KEY, MANAGED_VERSION_VALUE);
}

#[cfg(test)]
mod tests {
    use super::super::parser::parse;
    use super::super::serializer::serialize;
    use super::*;

    #[test]
    fn read_decodes_strings_and_bools() {
        let file = parse("[Desktop Entry]\nName=My\\sApp\nTerminal=true\nCategories=Dev;Tool;\n");
        let f = read_fields(&file);
        assert_eq!(f.name.as_deref(), Some("My App"));
        assert_eq!(f.terminal, Some(true));
        assert_eq!(f.categories, Some(vec!["Dev".into(), "Tool".into()]));
    }

    #[test]
    fn read_reports_unparseable_exec() {
        let file = parse("[Desktop Entry]\nExec=broke \"n quote\n");
        let f = read_fields(&file);
        assert!(f.exec_parse_error);
        assert_eq!(f.exec.unwrap().executable, "broke \"n quote");
    }

    #[test]
    fn apply_patch_sets_removes_and_preserves_the_rest() {
        let mut file = parse(
            "[Desktop Entry]\nType=Application\nName=Old\nExec=old\nX-Custom=keep\nName[zh_CN]=旧\n",
        );
        let patch = FieldPatch {
            name: Some(Some("New".into())),
            exec: Some(Some(ExecSpec {
                executable: "/path with space/foo".into(),
                arguments: vec!["%f".into()],
            })),
            terminal: Some(Some(true)),
            comment: Some(None),
            ..Default::default()
        };
        apply_patch(&mut file, &patch);
        let out = serialize(&file);
        assert!(out.contains("Name=New\n"));
        assert!(out.contains("Exec=\"/path with space/foo\" %f\n"));
        assert!(out.contains("Terminal=true\n"));
        assert!(!out.contains("Comment="));
        assert!(out.contains("X-Custom=keep\n"));
        assert!(out.contains("Name[zh_CN]=旧\n"));
        assert!(out.contains("Type=Application\n"));
    }

    #[test]
    fn managed_markers_round_trip() {
        let mut file = DesktopFile::new_entry_file();
        apply_managed_markers(&mut file);
        assert_eq!(file.get_raw(MANAGED_KEY), Some("true"));
        assert_eq!(file.get_raw(MANAGED_VERSION_KEY), Some("1"));
        let f = read_fields(&file);
        assert!(f.managed);
        assert_eq!(f.managed_version.as_deref(), Some("1"));
    }

    #[test]
    fn unknown_key_count_ignores_known_and_locale() {
        let file = parse("[Desktop Entry]\nName=A\nX-Foo=1\nCustom=2\nName[de]=C\n");
        assert_eq!(file.unknown_key_count(KNOWN_KEYS), 2); // X-Foo + Custom
        assert_eq!(file.locale_key_count(), 1);
    }

    #[test]
    fn entry_group_constant_matches_spec() {
        assert_eq!(super::super::model::ENTRY_GROUP, "Desktop Entry");
    }
}
