//! Ordered document model for desktop entry files.
//!
//! The model preserves insertion order, comments, blank lines, unparsable
//! lines, locale keys (`Name[zh_CN]`), unknown keys and extra groups so that
//! a parse -> edit -> serialize cycle never destroys unrelated content.
//! Values are stored exactly as they appear in the file (raw, still escaped);
//! decoding/encoding of typed values happens in [`super::fields`].

pub const ENTRY_GROUP: &str = "Desktop Entry";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Key {
    pub name: String,
    pub locale: Option<String>,
}

impl Key {
    pub fn plain(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            locale: None,
        }
    }

    pub fn to_storage(&self) -> String {
        match &self.locale {
            Some(locale) => format!("{}[{}]", self.name, locale),
            None => self.name.clone(),
        }
    }
}

/// A single line inside a group. `Comment` and `Raw` keep the full original
/// line (including a leading `#` for comments) for byte-faithful round-trips.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Entry { key: Key, value: String },
    Comment { text: String },
    Blank,
    Raw { text: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub name: String,
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DesktopFile {
    pub groups: Vec<Group>,
}

impl DesktopFile {
    pub fn new_entry_file() -> Self {
        Self {
            groups: vec![Group {
                name: ENTRY_GROUP.to_string(),
                items: Vec::new(),
            }],
        }
    }

    pub fn entry_group(&self) -> Option<&Group> {
        self.groups.iter().find(|g| g.name == ENTRY_GROUP)
    }

    pub fn entry_group_mut(&mut self) -> Option<&mut Group> {
        self.groups.iter_mut().find(|g| g.name == ENTRY_GROUP)
    }

    pub fn ensure_entry_group(&mut self) -> &mut Group {
        if self.entry_group().is_none() {
            self.groups.insert(
                0,
                Group {
                    name: ENTRY_GROUP.to_string(),
                    items: Vec::new(),
                },
            );
        }
        self.entry_group_mut()
            .expect("entry group was just ensured to exist")
    }

    fn matching_entry(item: &Item, key_name: &str) -> bool {
        matches!(
            item,
            Item::Entry { key, .. }
                if key.name == key_name && key.locale.is_none()
        )
    }

    /// Read a plain (non-locale) key from the `Desktop Entry` group.
    /// Duplicate keys are undefined by the spec; last occurrence wins,
    /// matching GKeyFile behavior.
    pub fn get_raw(&self, key_name: &str) -> Option<&str> {
        self.entry_group()?
            .items
            .iter()
            .rev()
            .find_map(|item| match item {
                Item::Entry { key, value } if key.name == key_name && key.locale.is_none() => {
                    Some(value.as_str())
                }
                _ => None,
            })
    }

    /// Write a plain key. The first occurrence is updated in place (keeping
    /// its position), later duplicates are removed, and missing keys are
    /// appended at the end of the group.
    pub fn set_raw(&mut self, key_name: &str, value: &str) {
        let group = self.ensure_entry_group();
        let mut seen = false;
        group.items.retain_mut(|item| {
            if Self::matching_entry(item, key_name) {
                if seen {
                    return false;
                }
                seen = true;
                if let Item::Entry { value: v, .. } = item {
                    *v = value.to_string();
                }
            }
            true
        });
        if !seen {
            group.items.push(Item::Entry {
                key: Key::plain(key_name),
                value: value.to_string(),
            });
        }
    }

    pub fn remove_raw(&mut self, key_name: &str) -> bool {
        let Some(group) = self.entry_group_mut() else {
            return false;
        };
        let before = group.items.len();
        group
            .items
            .retain(|item| !Self::matching_entry(item, key_name));
        group.items.len() != before
    }

    pub fn desktop_action_names(&self) -> Vec<String> {
        const PREFIX: &str = "Desktop Action ";
        self.groups
            .iter()
            .filter_map(|g| g.name.strip_prefix(PREFIX))
            .map(str::to_string)
            .collect()
    }

    pub fn locale_key_count(&self) -> usize {
        self.groups
            .iter()
            .flat_map(|g| g.items.iter())
            .filter(|item| matches!(item, Item::Entry { key, .. } if key.locale.is_some()))
            .count()
    }

    /// Count plain keys in the `Desktop Entry` group that this tool does not
    /// know about (`X-*` keys included; they are user-defined by design).
    pub fn unknown_key_count(&self, known_keys: &[&str]) -> usize {
        self.entry_group()
            .map(|g| {
                g.items
                    .iter()
                    .filter(|item| {
                        matches!(item, Item::Entry { key, .. }
                            if key.locale.is_none() && !known_keys.contains(&key.name.as_str()))
                    })
                    .count()
            })
            .unwrap_or(0)
    }
}
