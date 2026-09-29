//! Round-trip and field-editing integration tests over fixture files.

use linux_app_editor_lib::desktop_entry::exec::ExecSpec;
use linux_app_editor_lib::desktop_entry::fields::{
    FieldPatch, KNOWN_KEYS, apply_patch, read_fields,
};
use linux_app_editor_lib::desktop_entry::{parser, serializer};

fn fixture(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read_to_string(path).expect("fixture readable")
}

const GOLDEN: &[&str] = &[
    "simple.desktop",
    "localized.desktop",
    "unknown-x-keys.desktop",
    "actions.desktop",
    "spaces.desktop",
    "field-codes.desktop",
    "flatpak-like.desktop",
    "weird-but-valid.desktop",
];

#[test]
fn parse_serialize_is_byte_identical() {
    for name in GOLDEN {
        let input = fixture(name);
        assert_eq!(
            serializer::serialize(&parser::parse(&input)),
            input,
            "{name}"
        );
    }
}

#[test]
fn editing_name_keeps_everything_else() {
    let input = fixture("localized.desktop");
    let mut file = parser::parse(&input);
    apply_patch(
        &mut file,
        &FieldPatch {
            name: Some(Some("Wörterbuch".into())),
            ..Default::default()
        },
    );
    let out = serializer::serialize(&file);
    assert!(out.contains("Name=Wörterbuch\n"));
    assert!(out.contains("Name[zh_CN]=词典\n"));
    assert!(out.contains("Name[zh_TW]=詞典\n"));
    assert!(out.contains("Comment[de]=Wörter nachschlagen\n"));
    assert!(out.contains("Exec=gnome-dictionary\n"));
}

#[test]
fn editing_exec_keeps_x_keys_and_actions() {
    let input = fixture("actions.desktop");
    let mut file = parser::parse(&input);
    apply_patch(
        &mut file,
        &FieldPatch {
            exec: Some(Some(ExecSpec {
                executable: "/opt/Browser/browser".into(),
                arguments: vec!["--new-tab".into(), "%u".into()],
            })),
            ..Default::default()
        },
    );
    let out = serializer::serialize(&file);
    assert!(out.contains("Exec=/opt/Browser/browser --new-tab %u\n"));
    assert!(out.contains("Actions=new-window;new-private-window;\n"));
    assert!(
        out.contains("[Desktop Action new-window]\nName=New Window\nExec=browser --new-window")
    );
    assert!(out.contains("Name[zh_CN]=新建隐私窗口\n"));
    assert!(out.contains("Exec=browser --private\n"));
}

#[test]
fn field_codes_survive_untouched_exec() {
    let input = fixture("field-codes.desktop");
    let file = parser::parse(&input);
    let fields = read_fields(&file);
    let exec = fields.exec.expect("exec present");
    assert_eq!(
        exec.arguments,
        vec![
            "%F",
            "--url",
            "%u",
            "--icon",
            "%i",
            "--name",
            "%c",
            "--file",
            "%k",
            "--progress",
            "50%"
        ]
    );
    // Rebuilding the parsed spec reproduces the original Exec line.
    let mut file2 = parser::parse(&input);
    apply_patch(
        &mut file2,
        &FieldPatch {
            exec: Some(Some(exec)),
            ..Default::default()
        },
    );
    assert!(
        serializer::serialize(&file2).contains(
            "Exec=file-thing %F --url %u --icon %i --name %c --file %k --progress 50%%\n"
        )
    );
}

#[test]
fn spaces_fixture_parses_quoted_exec() {
    let file = parser::parse(&fixture("spaces.desktop"));
    let fields = read_fields(&file);
    let exec = fields.exec.unwrap();
    assert_eq!(exec.executable, "/opt/My Apps/spacey binary");
    assert_eq!(
        exec.arguments,
        vec![
            "--profile=my profile".to_string(),
            "file name.txt".to_string()
        ]
    );
    // Icon value with \s escape decodes to a plain path.
    assert_eq!(fields.icon.as_deref(), Some("/opt/My Apps/spacey.png"));
    assert!(!fields.exec_parse_error);
}

#[test]
fn icon_with_space_is_escaped_again_on_write() {
    let mut file = parser::parse(&fixture("simple.desktop"));
    apply_patch(
        &mut file,
        &FieldPatch {
            icon: Some(Some("/opt/My Apps/icon.png".into())),
            ..Default::default()
        },
    );
    assert!(serializer::serialize(&file).contains("Icon=/opt/My\\sApps/icon.png\n"));
}

#[test]
fn flatpak_fixture_round_trips_and_parses() {
    let input = fixture("flatpak-like.desktop");
    let file = parser::parse(&input);
    let fields = read_fields(&file);
    let exec = fields.exec.unwrap();
    assert_eq!(exec.executable, "flatpak");
    assert!(exec.arguments.contains(&"@@u".to_string()));
    assert!(exec.arguments.contains(&"%u".to_string()));
    assert_eq!(serializer::serialize(&file), input);
}

#[test]
fn weird_fixture_reports_and_preserves() {
    let input = fixture("weird-but-valid.desktop");
    let file = parser::parse(&input);
    // Duplicate key: last occurrence wins on read.
    assert_eq!(file.get_raw("Name"), Some("Second Wins"));
    // Unparseable line is preserved.
    assert!(serializer::serialize(&file).contains("not a valid line at all"));
    // Escaped string values decode.
    let fields = read_fields(&file);
    assert_eq!(fields.name.as_deref(), Some("Second Wins"));
    assert_eq!(fields.icon.as_deref(), Some(""));
    assert_eq!(fields.comment.as_deref(), Some("Multi\nline\tvalue"));
    // Meta counts.
    assert_eq!(file.unknown_key_count(KNOWN_KEYS), 1); // X-Empty
}

#[test]
fn crlf_input_is_read_and_normalized() {
    let input = "[Desktop Entry]\r\nType=Application\r\nName=CRLF\r\n";
    let file = parser::parse(input);
    assert_eq!(file.get_raw("Name"), Some("CRLF"));
    assert_eq!(
        serializer::serialize(&file),
        "[Desktop Entry]\nType=Application\nName=CRLF\n"
    );
}

#[test]
fn file_without_entry_group_still_opens_and_edits() {
    let input = "[Other Group]\nKey=value\n";
    let mut file = parser::parse(input);
    assert_eq!(file.get_raw("Name"), None);
    file.set_raw("Name", "Inserted");
    let out = serializer::serialize(&file);
    assert!(out.starts_with("[Desktop Entry]\nName=Inserted\n"));
    assert!(out.contains("[Other Group]\nKey=value\n"));
}

#[test]
fn removing_optional_fields_keeps_required_layout() {
    let input = fixture("simple.desktop");
    let mut file = parser::parse(&input);
    apply_patch(
        &mut file,
        &FieldPatch {
            comment: Some(None),
            keywords: Some(None),
            only_show_in: Some(None),
            ..Default::default()
        },
    );
    let out = serializer::serialize(&file);
    assert!(!out.contains("Comment="));
    assert!(!out.contains("Keywords="));
    assert!(!out.contains("OnlyShowIn="));
    assert!(out.contains("Name=Simple App\n"));
    assert!(out.contains("NotShowIn=XFCE;\n"));
}
