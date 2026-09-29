//! Serializer for the ordered document model. Only two normalizations are
//! ever applied: CRLF line endings become LF and the output is guaranteed to
//! end with a single trailing newline.

use super::model::{DesktopFile, Item};

pub fn serialize(file: &DesktopFile) -> String {
    let mut out = String::new();
    for group in &file.groups {
        if !group.name.is_empty() {
            out.push('[');
            out.push_str(&group.name);
            out.push_str("]\n");
        }
        for item in &group.items {
            match item {
                Item::Entry { key, value } => {
                    out.push_str(&key.to_storage());
                    out.push('=');
                    out.push_str(value);
                    out.push('\n');
                }
                Item::Comment { text } | Item::Raw { text } => {
                    out.push_str(text);
                    out.push('\n');
                }
                Item::Blank => out.push('\n'),
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::parser::parse;
    use super::*;

    #[test]
    fn round_trips_simple_file() {
        let input = "[Desktop Entry]\nType=Application\nName=Foo\nExec=foo\n";
        assert_eq!(serialize(&parse(input)), input);
    }

    #[test]
    fn round_trips_comments_blank_lines_and_groups() {
        let input = "# comment\n[Desktop Entry]\nName=Foo\n\n[Desktop Action open]\nExec=foo open\nName=Open\n";
        assert_eq!(serialize(&parse(input)), input);
    }

    #[test]
    fn normalizes_missing_trailing_newline() {
        let out = serialize(&parse("[Desktop Entry]\nName=Foo"));
        assert_eq!(out, "[Desktop Entry]\nName=Foo\n");
    }

    #[test]
    fn set_raw_replaces_in_place_and_keeps_neighbors() {
        let input = "# top\n[Desktop Entry]\nName=Foo\nExec=foo\n";
        let mut file = parse(input);
        file.set_raw("Name", "Bar");
        assert_eq!(
            serialize(&file),
            "# top\n[Desktop Entry]\nName=Bar\nExec=foo\n"
        );
    }

    #[test]
    fn set_raw_appends_when_missing() {
        let mut file = parse("[Desktop Entry]\nName=Foo\n");
        file.set_raw("Terminal", "true");
        assert_eq!(
            serialize(&file),
            "[Desktop Entry]\nName=Foo\nTerminal=true\n"
        );
    }

    #[test]
    fn set_raw_deduplicates_keeping_first_position() {
        let mut file = parse("[Desktop Entry]\nName=A\nName=B\n");
        file.set_raw("Name", "C");
        assert_eq!(serialize(&file), "[Desktop Entry]\nName=C\n");
    }

    #[test]
    fn remove_raw_leaves_other_keys() {
        let mut file = parse("[Desktop Entry]\nName=Foo\nExec=foo\n");
        assert!(file.remove_raw("Exec"));
        assert_eq!(serialize(&file), "[Desktop Entry]\nName=Foo\n");
        assert!(!file.remove_raw("Exec"));
    }
}
