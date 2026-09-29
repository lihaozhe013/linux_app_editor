//! Lenient line-oriented parser. Anything that does not look like a group
//! header, comment, blank line or `key=value` pair is kept as a raw line so
//! serializing never destroys it.

use super::model::{DesktopFile, Group, Item, Key};

pub fn parse(input: &str) -> DesktopFile {
    let input = input.strip_prefix('\u{feff}').unwrap_or(input);
    // A trailing newline must not materialize as a phantom blank item.
    let body = if input.is_empty() {
        ""
    } else {
        input.strip_suffix('\n').unwrap_or(input)
    };
    let mut file = DesktopFile::default();

    for raw_line in body.split('\n') {
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        let trimmed = line.trim();

        if trimmed.is_empty() {
            push_item(&mut file, Item::Blank);
        } else if let Some(header) = group_header(trimmed) {
            file.groups.push(Group {
                name: header.to_string(),
                items: Vec::new(),
            });
        } else if trimmed.starts_with('#') {
            push_item(
                &mut file,
                Item::Comment {
                    text: line.to_string(),
                },
            );
        } else if let Some((key_part, value)) = split_key_value(trimmed) {
            match parse_key(key_part) {
                Some(key) => push_item(
                    &mut file,
                    Item::Entry {
                        key,
                        value: value.to_string(),
                    },
                ),
                None => push_item(
                    &mut file,
                    Item::Raw {
                        text: line.to_string(),
                    },
                ),
            }
        } else {
            push_item(
                &mut file,
                Item::Raw {
                    text: line.to_string(),
                },
            );
        }
    }
    file
}

/// Items that appear before any group header are collected into an anonymous
/// group whose name is empty; the serializer emits them without a header.
fn push_item(file: &mut DesktopFile, item: Item) {
    let reuse_last = match file.groups.last() {
        Some(group) => !group.name.is_empty() || file.groups.len() == 1,
        None => false,
    };
    if !reuse_last {
        file.groups.push(Group {
            name: String::new(),
            items: Vec::new(),
        });
    }
    file.groups
        .last_mut()
        .expect("a group exists")
        .items
        .push(item);
}

fn group_header(trimmed: &str) -> Option<&str> {
    let inner = trimmed.strip_prefix('[')?;
    let header = inner.strip_suffix(']')?;
    if header.is_empty() {
        return None;
    }
    Some(header)
}

fn split_key_value(trimmed: &str) -> Option<(&str, &str)> {
    let idx = trimmed.find('=')?;
    Some((&trimmed[..idx], &trimmed[idx + 1..]))
}

fn parse_key(s: &str) -> Option<Key> {
    if let Some(open) = s.find('[') {
        let close = s.rfind(']')?;
        if close != s.len() - 1 || close <= open {
            return None;
        }
        let name = &s[..open];
        let locale = &s[open + 1..close];
        if valid_key_name(name) && !locale.is_empty() {
            return Some(Key {
                name: name.to_string(),
                locale: Some(locale.to_string()),
            });
        }
        return None;
    }
    valid_key_name(s).then(|| Key::plain(s))
}

fn valid_key_name(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_entry() {
        let file = parse("[Desktop Entry]\nType=Application\nName=Foo\n");
        assert_eq!(file.groups.len(), 1);
        assert_eq!(file.get_raw("Type"), Some("Application"));
        assert_eq!(file.get_raw("Name"), Some("Foo"));
    }

    #[test]
    fn parses_locale_key() {
        let file = parse("[Desktop Entry]\nName=Foo\nName[zh_CN]=foobar\n");
        let entry_group = file.entry_group().unwrap();
        let locale_items: Vec<&super::super::model::Item> = entry_group
            .items
            .iter()
            .filter(|i| matches!(i, Item::Entry { key, .. } if key.locale.is_some()))
            .collect();
        assert_eq!(locale_items.len(), 1);
        assert_eq!(file.get_raw("Name"), Some("Foo"));
    }

    #[test]
    fn keeps_unknown_lines_as_raw() {
        let file = parse("[Desktop Entry]\n?? garbage\nName=Foo\n");
        let group = file.entry_group().unwrap();
        assert!(
            group
                .items
                .iter()
                .any(|i| matches!(i, Item::Raw { text } if text == "?? garbage"))
        );
    }

    #[test]
    fn handles_crlf_and_missing_trailing_newline() {
        let file = parse("[Desktop Entry]\r\nName=Foo");
        assert_eq!(file.get_raw("Name"), Some("Foo"));
    }

    #[test]
    fn duplicate_keys_last_wins_on_read() {
        let file = parse("[Desktop Entry]\nName=A\nName=B\n");
        assert_eq!(file.get_raw("Name"), Some("B"));
    }

    #[test]
    fn items_before_group_go_to_anonymous_group() {
        let file = parse("stray line\n[Desktop Entry]\nName=Foo\n");
        assert_eq!(file.groups.len(), 2);
        assert_eq!(file.groups[0].name, "");
        assert_eq!(file.groups[1].name, "Desktop Entry");
    }
}
