//! Read-only inspection of AppImage files.
//!
//! A type 2 AppImage is an ELF runtime with an `AI` marker at offset 8,
//! followed by an embedded SquashFS filesystem whose `hsqs` magic marks its
//! start. Icons and desktop-entry metadata are extracted in-process through
//! the pure-Rust SquashFS reader (`backhand`): the AppImage itself is never
//! executed and the payload is never extracted as a whole.

use std::ffi::OsStr;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};

use backhand::{FilesystemReader, InnerNode, Node, SquashfsFileReader};
use serde::Serialize;

use crate::desktop_entry::fields;
use crate::desktop_entry::parser;
use crate::error::{AppError, AppResult};
use crate::filesystem;

const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];
const APPIMAGE_MAGIC: [u8; 2] = *b"AI";
/// The current AppImage format; type 1 (ISO 9660 payload) is long obsolete.
const TYPE_2: u8 = 2;
const TYPE_1: u8 = 1;

/// Bytes from the start of the file scanned for the squashfs magic. Real
/// runtimes are well under 1 MiB; 16 MiB leaves a large margin while keeping
/// a corrupt file from forcing a full-payload scan.
const MAX_RUNTIME_SIZE: u64 = 16 * 1024 * 1024;
/// The payload cannot start before the ELF magic, the `AI` marker and the
/// one-byte type code at offsets 8..=10.
const MIN_PAYLOAD_OFFSET: u64 = 12;
const SCAN_CHUNK: usize = 64 * 1024;
const SQUASHFS_MAGIC: [u8; 4] = *b"hsqs";
/// The runtime ELF can contain stray copies of the squashfs magic (the
/// AppImageKit runtime does); never consider more candidates than this.
const MAX_MAGIC_CANDIDATES: usize = 16;
/// `.DirIcon` -> icon-file chains longer than this are treated as cycles.
const MAX_SYMLINK_HOPS: usize = 8;

/// Metadata extracted from an AppImage, with the icon copied into the
/// app-owned icons directory.
#[derive(Debug, Serialize)]
pub struct Extracted {
    pub icon_path: PathBuf,
    pub name: Option<String>,
    pub comment: Option<String>,
    pub categories: Vec<String>,
}

/// App-owned storage for icons extracted from AppImages, under the XDG data
/// home so `XDG_DATA_HOME` redirections (as used by the tests) keep working.
pub fn icons_dir() -> AppResult<PathBuf> {
    Ok(filesystem::data_home()?
        .join("linux-app-editor")
        .join("icons"))
}

/// Extract icon and desktop-entry metadata from `appimage` and write the icon
/// to [`icons_dir`]. Idempotent: an existing extracted icon is overwritten.
pub fn extract(appimage: &Path) -> AppResult<Extracted> {
    let mut file = File::open(appimage).map_err(|e| AppError::io(appimage, e))?;
    ensure_type2(&mut file, appimage)?;
    let fs_reader = open_payload(&mut file, appimage)?;

    let listing = Listing::collect(&fs_reader);

    let (name, comment, categories, icon_name, desktop_stem) = match listing.root_desktops.first() {
        Some(node) => {
            let content = node_bytes(&fs_reader, node)?;
            let meta = desktop_meta(&content);
            let stem = node.fullpath.file_stem().and_then(OsStr::to_str);
            (
                meta.name,
                meta.comment,
                meta.categories,
                meta.icon,
                stem.map(str::to_string),
            )
        }
        None => (None, None, Vec::new(), None, None),
    };

    let icon_bytes = listing
        .resolve_icon(icon_name.as_deref())
        .ok_or_else(|| AppError::IconNotFound(appimage.to_path_buf()))?;
    let Some(extension) = sniff_icon_format(&icon_bytes) else {
        return Err(AppError::AppImageReadFailed(format!(
            "{}: the embedded icon uses an unsupported image format",
            appimage.display()
        )));
    };

    let stem_source = desktop_stem
        .or_else(|| {
            appimage
                .file_stem()
                .and_then(OsStr::to_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| "icon".to_string());
    let icon_path = icons_dir()?.join(format!("{}.{}", sanitize_stem(&stem_source), extension));
    filesystem::atomic_write(&icon_path, &icon_bytes)?;

    Ok(Extracted {
        icon_path,
        name,
        comment,
        categories,
    })
}

/// The file must look like an AppImage at all: ELF magic plus the `AI`
/// marker, with a type byte we can read.
fn ensure_type2(file: &mut File, path: &Path) -> AppResult<()> {
    let mut header = [0u8; 11];
    let filled = read_up_to(file, &mut header).map_err(|e| AppError::io(path, e))?;
    if filled < header.len() || header[..4] != ELF_MAGIC || header[8..10] != APPIMAGE_MAGIC {
        return Err(AppError::NotAppImage(path.to_path_buf()));
    }
    match header[10] {
        TYPE_2 => Ok(()),
        TYPE_1 => Err(AppError::UnsupportedAppImageType {
            path: path.to_path_buf(),
            details: "type 1 (ISO 9660) AppImages are long obsolete".to_string(),
        }),
        other => Err(AppError::UnsupportedAppImageType {
            path: path.to_path_buf(),
            details: format!("unknown AppImage type code {other}"),
        }),
    }
}

/// Find the embedded SquashFS payload and open it. The runtime ELF can
/// contain stray copies of the squashfs magic (the AppImageKit runtime
/// does), so every magic occurrence within the scan limit is checked in
/// order: a manual superblock consistency check weeds out ELF garbage
/// before the full parser runs. Scanning stops at the first payload that
/// parses. Each attempt parses its own `try_clone` of the file descriptor
/// so a failed attempt cannot consume the handle; on success the returned
/// reader owns the clone.
fn open_payload(file: &mut File, path: &Path) -> AppResult<FilesystemReader<'static>> {
    let file_len = file.metadata().map_err(|e| AppError::io(path, e))?.len();
    file.seek(SeekFrom::Start(MIN_PAYLOAD_OFFSET))
        .map_err(|e| AppError::io(path, e))?;

    let mut last_error = None;
    let mut attempts = 0usize;
    let mut scanned = MIN_PAYLOAD_OFFSET;
    let mut carried: Vec<u8> = Vec::new();
    loop {
        if scanned > MAX_RUNTIME_SIZE || attempts >= MAX_MAGIC_CANDIDATES {
            break;
        }
        let mut chunk = vec![0u8; SCAN_CHUNK];
        // Explicit positioning: any earlier parse attempt seeks a descriptor
        // that shares this handle's file offset (try_clone is dup(2) on
        // Unix), so the scan must not rely on the sequential position.
        file.seek(SeekFrom::Start(scanned))
            .map_err(|e| AppError::io(path, e))?;
        let filled = read_up_to(file, &mut chunk).map_err(|e| AppError::io(path, e))?;
        if filled == 0 {
            break;
        }
        let carry_len = carried.len();
        carried.extend_from_slice(&chunk[..filled]);
        let window_start = scanned - carry_len as u64;

        let mut searched = 0usize;
        while let Some(relative) = memchr::memchr(SQUASHFS_MAGIC[0], &carried[searched..]) {
            let at = searched + relative;
            if at + SQUASHFS_MAGIC.len() > carried.len() {
                // Possibly a match spanning into the next chunk.
                break;
            }
            searched = at + 1;
            if carried[at..at + SQUASHFS_MAGIC.len()] != SQUASHFS_MAGIC {
                continue;
            }
            let offset = window_start + at as u64;
            if !superblock_sane(file, offset, file_len) {
                continue;
            }
            attempts += 1;
            let clone = file.try_clone().map_err(|e| AppError::io(path, e))?;
            match FilesystemReader::from_reader_with_offset(BufReader::new(clone), offset) {
                Ok(reader) => return Ok(reader),
                Err(e) => last_error = Some(e.to_string()),
            }
        }

        scanned += filled as u64;
        // The chunk is fully searched; only the bytes that could complete a
        // match spanning into the next chunk are carried over.
        let keep = SQUASHFS_MAGIC.len() - 1;
        carried.drain(..carried.len().saturating_sub(keep));
    }
    Err(match last_error {
        Some(e) => AppError::AppImageReadFailed(format!(
            "{}: embedded SquashFS is unreadable: {e}",
            path.display()
        )),
        None => AppError::AppImageReadFailed(format!(
            "{}: no consistent SquashFS payload found within the first {} MiB",
            path.display(),
            MAX_RUNTIME_SIZE / 1024 / 1024
        )),
    })
}

/// Minimum superblock consistency before handing an offset to the full
/// parser. Field offsets mirror `backhand::SuperBlock`: `frag_count` is a
/// `u32` at 16, `compressor` a `u16` at 20, `block_log` at 22, version at
/// 28..32 and `bytes_used` at 40. Reads at the offset without moving the
/// handle's file position, so interleaved candidate checks cannot disturb
/// the caller's scan.
fn superblock_sane(file: &File, offset: u64, file_len: u64) -> bool {
    if offset.saturating_add(96) > file_len {
        return false;
    }
    let mut block = [0u8; 96];

    #[cfg(unix)]
    let filled = {
        use std::os::unix::fs::FileExt;
        let mut read = 0usize;
        while read < block.len() {
            match file.read_at(&mut block[read..], offset + read as u64) {
                Ok(0) => break,
                Ok(n) => read += n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => return false,
            }
        }
        read
    };

    #[cfg(not(unix))]
    let filled = {
        let mut probe = match file.try_clone() {
            Ok(probe) => probe,
            Err(_) => return false,
        };
        if probe.seek(SeekFrom::Start(offset)).is_err() {
            return false;
        }
        read_up_to(&mut probe, &mut block).unwrap_or(0)
    };

    if filled < block.len() {
        return false;
    }
    let le_u32 = |range: std::ops::Range<usize>| {
        u32::from_le_bytes(block[range].try_into().expect("fixed-size slice"))
    };
    let le_u16 = |range: std::ops::Range<usize>| {
        u16::from_le_bytes(block[range].try_into().expect("fixed-size slice"))
    };

    let block_size = le_u32(12..16);
    let compressor = le_u16(20..22);
    let block_log = le_u16(22..24);
    let version_major = le_u16(28..30);
    let version_minor = le_u16(30..32);
    let bytes_used = u64::from_le_bytes(block[40..48].try_into().expect("fixed-size slice"));

    block_size.is_power_of_two()
        && (4096..=1_048_576).contains(&block_size)
        && block_log as u32 == (u32::BITS - 1) - block_size.leading_zeros()
        && compressor <= 6
        && (version_major, version_minor) == (4, 0)
        && bytes_used >= 96
        && offset.saturating_add(bytes_used) <= file_len
}

/// Node references collected during one pass over the SquashFS tree.
struct Listing<'a, 'b> {
    fs_reader: &'a FilesystemReader<'b>,
    files: Vec<&'a Node<SquashfsFileReader>>,
    dir_icon: Option<&'a Node<SquashfsFileReader>>,
    dir_icon_link: Option<PathBuf>,
    root_desktops: Vec<&'a Node<SquashfsFileReader>>,
    root_images: Vec<&'a Node<SquashfsFileReader>>,
}

impl<'a, 'b> Listing<'a, 'b> {
    fn collect(fs_reader: &'a FilesystemReader<'b>) -> Self {
        let mut listing = Self {
            fs_reader,
            files: Vec::new(),
            dir_icon: None,
            dir_icon_link: None,
            root_desktops: Vec::new(),
            root_images: Vec::new(),
        };
        for node in fs_reader.files() {
            if let InnerNode::Symlink(symlink) = &node.inner {
                if Self::root_child_name(node) == Some(".DirIcon") {
                    listing.dir_icon_link = Some(symlink.link.clone());
                }
                continue;
            }
            if !matches!(node.inner, InnerNode::File(_)) {
                continue;
            }
            listing.files.push(node);
            match Self::root_child_name(node) {
                Some(".DirIcon") => listing.dir_icon = Some(node),
                Some(name) if name.ends_with(".desktop") => listing.root_desktops.push(node),
                Some(name) if is_image_name(name) => listing.root_images.push(node),
                _ => {}
            }
        }
        listing
            .root_desktops
            .sort_by_key(|node| node.fullpath.clone());
        listing
            .root_images
            .sort_by_key(|node| node.fullpath.clone());
        listing
    }

    /// File name for entries directly below the image root.
    fn root_child_name(node: &Node<SquashfsFileReader>) -> Option<&str> {
        let parent = node.fullpath.parent()?;
        if parent.as_os_str() != "/" {
            return None;
        }
        node.fullpath.file_name().and_then(OsStr::to_str)
    }

    /// Icon bytes following the AppDir spec order: `.DirIcon` (file or
    /// symlink chain), then a root-level image named after the embedded
    /// `Icon=` value.
    fn resolve_icon(&self, icon_name: Option<&str>) -> Option<Vec<u8>> {
        if let Some(node) = self.dir_icon {
            return node_bytes(self.fs_reader, node).ok();
        }
        if let Some(link) = &self.dir_icon_link {
            let mut target = resolve_link_target(link);
            for _ in 0..MAX_SYMLINK_HOPS {
                let Some(node) = self.find_file(&target) else {
                    break;
                };
                if matches!(node.inner, InnerNode::File(_)) {
                    return node_bytes(self.fs_reader, node).ok();
                }
                let InnerNode::Symlink(next) = &node.inner else {
                    break;
                };
                target = resolve_link_target(&next.link);
            }
        }
        if let Some(name) = icon_name {
            // The spec recommends `Icon=` without an extension; match on the
            // file stem, case-insensitively.
            let wanted = name.to_ascii_lowercase();
            if let Some(node) = listing_find_stem(&self.root_images, &wanted) {
                return node_bytes(self.fs_reader, node).ok();
            }
        }
        None
    }

    fn find_file(&self, fullpath: &Path) -> Option<&'a Node<SquashfsFileReader>> {
        self.files
            .iter()
            .copied()
            .find(|node| node.fullpath == fullpath)
    }
}

fn listing_find_stem<'a>(
    nodes: &[&'a Node<SquashfsFileReader>],
    wanted: &str,
) -> Option<&'a Node<SquashfsFileReader>> {
    nodes.iter().copied().find(|node| {
        node.fullpath
            .file_stem()
            .and_then(OsStr::to_str)
            .is_some_and(|stem| stem.eq_ignore_ascii_case(wanted))
    })
}

fn node_bytes(
    fs_reader: &FilesystemReader<'_>,
    node: &Node<SquashfsFileReader>,
) -> AppResult<Vec<u8>> {
    let InnerNode::File(file) = &node.inner else {
        return Err(AppError::AppImageReadFailed(
            "expected a regular file node".to_string(),
        ));
    };
    let mut reader = fs_reader.file(file).reader_checked().map_err(|e| {
        AppError::AppImageReadFailed(format!("reading inside SquashFS failed: {e}"))
    })?;
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).map_err(|e| {
        AppError::AppImageReadFailed(format!("reading inside SquashFS failed: {e}"))
    })?;
    Ok(bytes)
}

/// SquashFS symlink targets are relative to the image root; normalize
/// without touching the real filesystem.
fn resolve_link_target(target: &Path) -> PathBuf {
    let mut normalized = PathBuf::from("/");
    for component in target.components() {
        match component {
            Component::Normal(part) => normalized.push(part),
            // Never pop above the image root.
            Component::ParentDir if normalized.file_name().is_some() => {
                normalized.pop();
            }
            _ => {}
        }
    }
    normalized
}

struct DesktopMeta {
    name: Option<String>,
    comment: Option<String>,
    categories: Vec<String>,
    icon: Option<String>,
}

fn desktop_meta(content: &[u8]) -> DesktopMeta {
    // Embedded desktop files are expected to be UTF-8; lossy keeps the
    // extraction useful when they are not.
    let text = String::from_utf8_lossy(content);
    let parsed = parser::parse(&text);
    let known = fields::read_fields(&parsed);
    DesktopMeta {
        name: fields::display_name(&parsed),
        comment: known.comment,
        categories: known.categories.unwrap_or_default(),
        icon: known.icon,
    }
}

fn is_image_name(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, ext)| {
        [
            "png", "svg", "webp", "jpg", "jpeg", "gif", "ico", "xpm", "bmp",
        ]
        .contains(&ext.to_ascii_lowercase().as_str())
    })
}

/// Detect the stored extension from content, not from the original name:
/// `.DirIcon` entries frequently carry no extension at all.
fn sniff_icon_format(bytes: &[u8]) -> Option<&'static str> {
    const HEAD: usize = 512;
    let head = &bytes[..bytes.len().min(HEAD)];
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some("png")
    } else if bytes.starts_with(b"RIFF") && bytes.len() >= 12 && &bytes[8..12] == b"WEBP" {
        Some("webp")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("jpg")
    } else if bytes.starts_with(b"GIF8") {
        Some("gif")
    } else if bytes.starts_with(&[0x00, 0x00, 0x01, 0x00]) {
        Some("ico")
    } else if bytes.starts_with(b"BM") {
        Some("bmp")
    } else if bytes.starts_with(b"/* XPM */") {
        Some("xpm")
    } else if std::str::from_utf8(head).is_ok_and(|text| text.contains("<svg")) {
        Some("svg")
    } else {
        None
    }
}

/// Keep only characters that are safe in a file name; anything else becomes
/// a dash, so AppImage names like `My App (1.2)` stay readable.
fn sanitize_stem(raw: &str) -> String {
    let sanitized: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '-'
            }
        })
        .collect();
    if sanitized.is_empty() {
        "icon".to_string()
    } else {
        sanitized
    }
}

fn read_up_to(file: &mut File, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match file.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(filled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env_lock::env_lock;
    use backhand::{FilesystemWriter, NodeHeader};
    use std::io::Cursor;

    const PNG_BYTES: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 1, 2, 3, 4];
    const SVG_BYTES: &[u8] = b"<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>";
    const DESKTOP_BYTES: &[u8] = b"[Desktop Entry]\nType=Application\nName=My App\nComment=Does things\nCategories=Development;Utility;\nIcon=my-app\n";

    /// Test-only XDG data home, restoring the variable on drop.
    struct DataHome {
        dir: tempfile::TempDir,
        #[allow(dead_code)]
        guard: std::sync::MutexGuard<'static, ()>,
    }

    impl DataHome {
        fn new() -> Self {
            let guard = env_lock();
            let dir = tempfile::tempdir().unwrap();
            unsafe { std::env::set_var("XDG_DATA_HOME", dir.path()) };
            Self { dir, guard }
        }

        fn icons(&self) -> PathBuf {
            self.dir.path().join("linux-app-editor/icons")
        }
    }

    impl Drop for DataHome {
        fn drop(&mut self) {
            unsafe { std::env::remove_var("XDG_DATA_HOME") };
        }
    }

    /// Builds a synthetic type 2 AppImage: fake ELF header + AI marker +
    /// SquashFS payload, all written in-process.
    fn write_appimage(path: &Path, payload: &mut FilesystemWriter<'_, '_, '_>) {
        let mut cursor = Cursor::new(Vec::new());
        payload.write(&mut cursor).unwrap();
        let mut image = vec![0u8; 16];
        image[..4].copy_from_slice(&ELF_MAGIC);
        image[8..10].copy_from_slice(&APPIMAGE_MAGIC);
        image[10] = TYPE_2;
        image.extend_from_slice(cursor.into_inner().as_slice());
        std::fs::write(path, image).unwrap();
    }

    fn payload() -> FilesystemWriter<'static, 'static, 'static> {
        FilesystemWriter::default()
    }

    #[test]
    fn skips_stray_squashfs_magic_inside_the_runtime() {
        let _data_home = DataHome::new();
        let dir = tempfile::tempdir().unwrap();
        let appimage = dir.path().join("my-app.AppImage");

        let mut writer = payload();
        writer
            .push_file(
                Cursor::new(PNG_BYTES.to_vec()),
                ".DirIcon",
                NodeHeader::default(),
            )
            .unwrap();
        let mut cursor = Cursor::new(Vec::new());
        writer.write(&mut cursor).unwrap();
        let squashfs = cursor.into_inner();

        // Fake runtime carrying a stray copy of the magic, as the real
        // AppImageKit runtime does.
        let mut image = vec![0u8; 16];
        image[..4].copy_from_slice(&ELF_MAGIC);
        image[8..10].copy_from_slice(&APPIMAGE_MAGIC);
        image[10] = TYPE_2;
        image.extend_from_slice(SQUASHFS_MAGIC.as_slice());
        image.extend_from_slice(&[0u8; 200]);
        image.extend_from_slice(&squashfs);
        std::fs::write(&appimage, image).unwrap();

        let extracted = extract(&appimage).unwrap();
        assert_eq!(std::fs::read(&extracted.icon_path).unwrap(), PNG_BYTES);
    }

    #[test]
    fn extracts_diricon_file_and_metadata() {
        let data_home = DataHome::new();
        let dir = tempfile::tempdir().unwrap();
        let appimage = dir.path().join("MyApp-1.2.3-x86_64.AppImage");

        let mut writer = payload();
        writer
            .push_file(
                Cursor::new(PNG_BYTES.to_vec()),
                ".DirIcon",
                NodeHeader::default(),
            )
            .unwrap();
        writer
            .push_file(
                Cursor::new(DESKTOP_BYTES.to_vec()),
                "my-app.desktop",
                NodeHeader::default(),
            )
            .unwrap();
        write_appimage(&appimage, &mut writer);

        let extracted = extract(&appimage).unwrap();
        assert_eq!(extracted.icon_path, data_home.icons().join("my-app.png"));
        assert_eq!(std::fs::read(&extracted.icon_path).unwrap(), PNG_BYTES);
        assert_eq!(extracted.name.as_deref(), Some("My App"));
        assert_eq!(extracted.comment.as_deref(), Some("Does things"));
        assert_eq!(extracted.categories, vec!["Development", "Utility"]);
    }

    #[test]
    fn resolves_diricon_symlink_into_nested_icon() {
        let _data_home = DataHome::new();
        let dir = tempfile::tempdir().unwrap();
        let appimage = dir.path().join("my-app.AppImage");

        let mut writer = payload();
        writer
            .push_symlink(
                "usr/share/icons/hicolor/128x128/apps/my-app.svg",
                ".DirIcon",
                NodeHeader::default(),
            )
            .unwrap();
        writer
            .push_dir_all(
                "usr/share/icons/hicolor/128x128/apps",
                NodeHeader::default(),
            )
            .unwrap();
        writer
            .push_file(
                Cursor::new(SVG_BYTES.to_vec()),
                "usr/share/icons/hicolor/128x128/apps/my-app.svg",
                NodeHeader::default(),
            )
            .unwrap();
        writer
            .push_file(
                Cursor::new(DESKTOP_BYTES.to_vec()),
                "my-app.desktop",
                NodeHeader::default(),
            )
            .unwrap();
        write_appimage(&appimage, &mut writer);

        let extracted = extract(&appimage).unwrap();
        assert_eq!(extracted.icon_path.extension().unwrap(), "svg");
        assert_eq!(std::fs::read(&extracted.icon_path).unwrap(), SVG_BYTES);
        assert_eq!(extracted.name.as_deref(), Some("My App"));
    }

    #[test]
    fn falls_back_to_icon_named_by_desktop_entry() {
        let _data_home = DataHome::new();
        let dir = tempfile::tempdir().unwrap();
        let appimage = dir.path().join("my-app.AppImage");

        let mut writer = payload();
        writer
            .push_file(
                Cursor::new(SVG_BYTES.to_vec()),
                "my-app.svg",
                NodeHeader::default(),
            )
            .unwrap();
        writer
            .push_file(
                Cursor::new(DESKTOP_BYTES.to_vec()),
                "my-app.desktop",
                NodeHeader::default(),
            )
            .unwrap();
        write_appimage(&appimage, &mut writer);

        let extracted = extract(&appimage).unwrap();
        assert_eq!(extracted.icon_path.extension().unwrap(), "svg");
        assert_eq!(std::fs::read(&extracted.icon_path).unwrap(), SVG_BYTES);
    }

    #[test]
    fn derives_stem_from_appimage_name_without_desktop_entry() {
        let data_home = DataHome::new();
        let dir = tempfile::tempdir().unwrap();
        let appimage = dir.path().join("Cool App 2-x86_64.AppImage");

        let mut writer = payload();
        writer
            .push_file(
                Cursor::new(PNG_BYTES.to_vec()),
                ".DirIcon",
                NodeHeader::default(),
            )
            .unwrap();
        write_appimage(&appimage, &mut writer);

        let extracted = extract(&appimage).unwrap();
        assert_eq!(
            extracted.icon_path,
            data_home.icons().join("Cool-App-2-x86_64.png")
        );
        assert_eq!(extracted.name, None);
        assert!(extracted.categories.is_empty());
    }

    #[test]
    fn rejects_files_that_are_not_appimages() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("script.sh");
        std::fs::write(&script, b"#!/bin/sh\necho hi\n").unwrap();
        let err = extract(&script).unwrap_err();
        assert_eq!(err.code(), "not-appimage");
    }

    #[test]
    fn rejects_type1_appimages() {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("old.AppImage");
        let mut bytes = vec![0u8; 64];
        bytes[..4].copy_from_slice(&ELF_MAGIC);
        bytes[8..10].copy_from_slice(&APPIMAGE_MAGIC);
        bytes[10] = TYPE_1;
        std::fs::write(&image, bytes).unwrap();
        let err = extract(&image).unwrap_err();
        assert_eq!(err.code(), "unsupported-appimage-type");
    }

    #[test]
    fn errors_when_no_icon_matches() {
        let _data_home = DataHome::new();
        let dir = tempfile::tempdir().unwrap();
        let appimage = dir.path().join("my-app.AppImage");

        let mut writer = payload();
        writer
            .push_file(
                Cursor::new(DESKTOP_BYTES.to_vec()),
                "my-app.desktop",
                NodeHeader::default(),
            )
            .unwrap();
        write_appimage(&appimage, &mut writer);

        let err = extract(&appimage).unwrap_err();
        assert_eq!(err.code(), "icon-not-found");
    }
}
