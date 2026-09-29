//! Integration tests for the Tauri command layer against a temp XDG dir.

use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};

use linux_app_editor_lib::commands::{
    create_launcher, delete_launcher, list_managed_launchers, open_desktop_entry,
    save_desktop_entry, CreateLauncherRequest, SaveRequest,
};
use linux_app_editor_lib::desktop_entry::exec::ExecSpec;
use linux_app_editor_lib::desktop_entry::fields::FieldPatch;

fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
}

struct XdgGuard {
    _dir: tempfile::TempDir,
    _guard: MutexGuard<'static, ()>,
}

impl XdgGuard {
    fn new() -> Self {
        let guard = env_lock();
        let dir = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var("XDG_DATA_HOME", dir.path()) };
        Self {
            _dir: dir,
            _guard: guard,
        }
    }

    fn app_dir(&self) -> PathBuf {
        self._dir.path().join("applications")
    }
}

fn request(stem: &str) -> CreateLauncherRequest {
    CreateLauncherRequest {
        filename_stem: stem.to_string(),
        name: "My App".into(),
        exec: ExecSpec {
            executable: "/opt/nonexistent/my-app".into(),
            arguments: vec!["--profile dev".into()],
        },
        icon: Some("my-app".into()),
        working_directory: None,
        terminal: false,
        comment: Some("A test app".into()),
        categories: Some(vec!["Development".into(), "Utility".into()]),
        overwrite: false,
    }
}

#[test]
fn create_launcher_writes_expected_file() {
    let xdg = XdgGuard::new();
    let outcome = create_launcher(request("my-app")).unwrap();
    let path = xdg.app_dir().join("my-app.desktop");
    assert_eq!(outcome.path, path);

    let content = fs::read_to_string(&path).unwrap();
    assert!(content.contains("Version=1.5\n"));
    assert!(content.contains("Type=Application\n"));
    assert!(content.contains("Name=My App\n"));
    assert!(content.contains("Exec=/opt/nonexistent/my-app --profile\\ dev\n")
        || content.contains("Exec=/opt/nonexistent/my-app \"--profile dev\"\n"));
    assert!(content.contains("Icon=my-app\n"));
    assert!(content.contains("Terminal=false\n"));
    assert!(content.contains("Categories=Development;Utility;\n"));
    assert!(content.contains("X-LauncherEditor-Managed=true\n"));
    assert!(content.contains("X-LauncherEditor-Version=1\n"));

    // Executable does not exist -> warning, but creation succeeded.
    assert!(outcome
        .warnings
        .iter()
        .any(|w| w.code == "executable-missing"));
}

#[test]
fn create_rejects_bad_filenames_and_requires_confirmation_to_overwrite() {
    let _xdg = XdgGuard::new();
    let bad = create_launcher(request("a/b"));
    assert_eq!(bad.unwrap_err().code(), "invalid-filename");

    create_launcher(request("dup")).unwrap();
    let err = create_launcher(request("dup")).unwrap_err();
    assert_eq!(err.code(), "already-exists");
    let mut req = request("dup");
    req.overwrite = true;
    create_launcher(req).unwrap();
}

#[test]
fn open_returns_fields_and_meta() {
    let xdg = XdgGuard::new();
    create_launcher(request("meta-app")).unwrap();
    fs::write(
        xdg.app_dir().join("extra.desktop"),
        "[Desktop Entry]\nName=Extra\nX-Custom=1\nName[de]=Extra\n\n[Desktop Action act]\nName=Act\nExec=x\n",
    )
    .unwrap();

    let opened = open_desktop_entry(
        xdg.app_dir().join("meta-app.desktop").to_string_lossy().into_owned(),
    )
    .unwrap();
    assert_eq!(opened.fields.name.as_deref(), Some("My App"));
    assert!(opened.fields.managed);
    assert_eq!(opened.meta.locale_key_count, 0);

    let extra = open_desktop_entry(
        xdg.app_dir().join("extra.desktop").to_string_lossy().into_owned(),
    )
    .unwrap();
    assert_eq!(extra.meta.unknown_key_count, 1);
    assert_eq!(extra.meta.locale_key_count, 1);
    assert_eq!(extra.meta.desktop_actions, vec!["act".to_string()]);
}

#[test]
fn save_in_place_preserves_unrelated_content() {
    let xdg = XdgGuard::new();
    create_launcher(request("edit-me")).unwrap();
    let path = xdg.app_dir().join("edit-me.desktop");

    // External edit adds an unknown key; it must survive our save.
    let mut content = fs::read_to_string(&path).unwrap();
    content.push_str("X-External=keep\nName[ja]=編集\n");
    fs::write(&path, content).unwrap();

    let outcome = save_desktop_entry(SaveRequest {
        path: path.to_string_lossy().into_owned(),
        new_path: None,
        fields: FieldPatch {
            name: Some(Some("Renamed".into())),
            terminal: Some(Some(true)),
            ..Default::default()
        },
        make_backup: false,
    })
    .unwrap();
    assert_eq!(outcome.path, path);

    let after = fs::read_to_string(&path).unwrap();
    assert!(after.contains("Name=Renamed\n"));
    assert!(after.contains("Terminal=true\n"));
    assert!(after.contains("X-External=keep\n"));
    assert!(after.contains("Name[ja]=編集\n"));
    assert!(after.contains("X-LauncherEditor-Managed=true\n"));
    assert!(!after.contains("Name=My App\n"));
}

#[test]
fn save_as_writes_new_file_and_leaves_original() {
    let xdg = XdgGuard::new();
    create_launcher(request("origin")).unwrap();
    let origin = xdg.app_dir().join("origin.desktop");
    let target = xdg.app_dir().join("copy.desktop");
    let before = fs::read_to_string(&origin).unwrap();

    let outcome = save_desktop_entry(SaveRequest {
        path: origin.to_string_lossy().into_owned(),
        new_path: Some(target.to_string_lossy().into_owned()),
        fields: FieldPatch {
            name: Some(Some("Copy".into())),
            ..Default::default()
        },
        make_backup: false,
    })
    .unwrap();
    assert_eq!(outcome.path, target);
    assert!(fs::read_to_string(&target).unwrap().contains("Name=Copy\n"));
    assert_eq!(fs::read_to_string(&origin).unwrap(), before);
}

#[test]
fn save_creates_backup_when_requested() {
    let xdg = XdgGuard::new();
    create_launcher(request("backed")).unwrap();
    let path = xdg.app_dir().join("backed.desktop");
    save_desktop_entry(SaveRequest {
        path: path.to_string_lossy().into_owned(),
        new_path: None,
        fields: FieldPatch {
            name: Some(Some("v2".into())),
            ..Default::default()
        },
        make_backup: true,
    })
    .unwrap();
    let bak = fs::read_to_string(xdg.app_dir().join("backed.desktop.bak")).unwrap();
    assert!(bak.contains("Name=My App\n"));
    assert!(fs::read_to_string(&path).unwrap().contains("Name=v2\n"));
}

#[test]
fn list_managed_shows_only_managed_launchers() {
    let xdg = XdgGuard::new();
    create_launcher(request("mine")).unwrap();
    fs::write(
        xdg.app_dir().join("foreign.desktop"),
        "[Desktop Entry]\nName=Foreign\nX-LauncherEditor-Managed=false\n",
    )
    .unwrap();
    fs::write(xdg.app_dir().join("plain.txt"), "not a desktop file").unwrap();

    let items = list_managed_launchers().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].file_name, "mine.desktop");
    assert_eq!(items[0].name.as_deref(), Some("My App"));
    assert!(items[0].executable_missing);
}

#[test]
fn delete_removes_only_regular_files() {
    let xdg = XdgGuard::new();
    create_launcher(request("doomed")).unwrap();
    let path = xdg.app_dir().join("doomed.desktop");
    delete_launcher(path.to_string_lossy().into_owned()).unwrap();
    assert!(!path.exists());

    let dir = xdg.app_dir().to_string_lossy().into_owned();
    assert_eq!(delete_launcher(dir).unwrap_err().code(), "io-error");
}

#[test]
fn save_validates_required_fields() {
    let xdg = XdgGuard::new();
    create_launcher(request("validated")).unwrap();
    let path = xdg.app_dir().join("validated.desktop");
    let err = save_desktop_entry(SaveRequest {
        path: path.to_string_lossy().into_owned(),
        new_path: None,
        fields: FieldPatch {
            name: Some(Some("   ".into())),
            ..Default::default()
        },
        make_backup: false,
    })
    .unwrap_err();
    assert_eq!(err.code(), "validation-failed");
}
