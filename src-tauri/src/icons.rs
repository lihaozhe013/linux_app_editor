//! Strictly bounded icon discovery near an executable. Runs only on explicit
//! user request; fixed candidate directories, depth and file caps, no system
//! icon theme scanning, no recursion.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

const ICON_EXTENSIONS: &[&str] = &["png", "svg", "webp", "jpg", "jpeg", "ico", "xpm", "bmp"];

/// Fixed relative to the executable's directory. Depth <= 2 from there.
const NEARBY_DIRS: &[&str] = &[
    ".",
    "..",
    "../assets",
    "../icons",
    "../share/icons",
    "../../assets",
];

const MAX_FILES_PER_DIR: usize = 50;
const MAX_TOTAL_FILES: usize = 200;

#[derive(Debug, Clone, Serialize)]
pub struct IconCandidate {
    pub path: PathBuf,
    pub file_name: String,
    pub size_bytes: u64,
}

pub fn find_nearby_icons(executable: &Path) -> Vec<IconCandidate> {
    let Some(exe_dir) = executable.parent() else {
        return Vec::new();
    };

    let mut candidates: Vec<IconCandidate> = Vec::new();
    for relative in NEARBY_DIRS {
        if candidates.len() >= MAX_TOTAL_FILES {
            break;
        }
        let dir = exe_dir.join(relative);
        let Ok(read_dir) = fs::read_dir(&dir) else {
            continue;
        };
        // Deterministic order regardless of readdir order.
        let mut entries: Vec<_> = read_dir.flatten().collect();
        entries.sort_by_key(|e| e.file_name());

        let mut taken = 0;
        for entry in entries {
            if taken >= MAX_FILES_PER_DIR || candidates.len() >= MAX_TOTAL_FILES {
                break;
            }
            if !entry.file_type().is_ok_and(|t| t.is_file()) {
                continue;
            }
            let path = entry.path();
            let is_image = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| ICON_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()));
            if !is_image {
                continue;
            }
            taken += 1;
            candidates.push(IconCandidate {
                file_name: entry.file_name().to_string_lossy().into_owned(),
                size_bytes: entry.metadata().map(|m| m.len()).unwrap_or(0),
                path,
            });
        }
    }

    // Names containing "icon" surface first, then plain alphabetical.
    candidates.sort_by(|a, b| {
        let a_hit = a.file_name.to_ascii_lowercase().contains("icon");
        let b_hit = b.file_name.to_ascii_lowercase().contains("icon");
        b_hit
            .cmp(&a_hit)
            .then_with(|| a.file_name.cmp(&b.file_name))
    });
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(path: &Path, bytes: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    #[test]
    fn scans_only_whitelisted_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let exe_dir = dir.path().join("app/bin");
        let exe = exe_dir.join("foo");
        touch(&exe, b"#!/bin/sh\n");

        touch(&exe_dir.join("icon.png"), b"1");
        touch(&exe_dir.parent().unwrap().join("assets/logo.svg"), b"2");
        touch(&exe_dir.parent().unwrap().join("icons/img.xpm"), b"3");
        touch(
            &exe_dir.parent().unwrap().join("share/icons/big.webp"),
            b"4",
        );
        touch(&dir.path().join("assets/a.png"), b"5");
        // Not in any whitelisted directory.
        touch(&dir.path().join("deep/nested/other.png"), b"6");
        touch(&exe_dir.join("readme.txt"), b"7");
        touch(&exe_dir.parent().unwrap().join("nope.pdf"), b"8");

        let found = find_nearby_icons(&exe);
        let names: Vec<&str> = found.iter().map(|c| c.file_name.as_str()).collect();
        assert_eq!(
            names,
            vec!["icon.png", "a.png", "big.webp", "img.xpm", "logo.svg"]
        );
    }

    #[test]
    fn icon_named_files_sort_first() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("foo");
        touch(&exe, b"");
        touch(&dir.path().join("zzz.png"), b"");
        touch(&dir.path().join("app-icon.png"), b"");
        let found = find_nearby_icons(&exe);
        assert_eq!(found[0].file_name, "app-icon.png");
    }

    #[test]
    fn caps_files_per_directory() {
        // Files live in the parent of the executable's directory so the
        // scan stays inside this tempdir (".." is the only relevant dir).
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        let exe = root.join("bin").join("exe");
        touch(&exe, b"");
        for i in 0..80 {
            touch(&root.join(format!("f{i:02}.png")), b"x");
        }
        let found = find_nearby_icons(&exe);
        assert_eq!(found.len(), MAX_FILES_PER_DIR);
    }

    #[test]
    fn missing_executable_parent_is_empty() {
        assert!(find_nearby_icons(Path::new("foo")).is_empty());
    }
}
